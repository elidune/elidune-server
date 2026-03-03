//! MARC batch import service.
//!
//! This service allows reading raw UNIMARC data, caching individual MARC
//! records in Redis under `marc:record:<BATCH_ID>:<ID>` keys, and importing
//! them into the local catalog.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;
use z3950_rs::marc_rs::{parse, Encoding, FormatEncoding, MarcFormat};

use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::item::{Item, ItemShort},
};

use super::{catalog::CatalogService, redis::RedisService};

/// Internal value stored in Redis for a MARC record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMarcRecord {
    /// Source identifier chosen by caller (e.g. Z39.50 server, file source).
    pub source_id: i64,
    /// Raw MARC record as parsed by marc-rs.
    pub record: MarcRecord,
}

/// Error information for a single record during batch import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarcBatchImportError {
    /// Redis record key (`marc:record:<batch_id>:<id>`).
    pub record_key: String,
    /// Human-readable error message.
    pub error: String,
}

/// Report returned after importing one or more records from a batch.
#[serde_as]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MarcBatchImportReport {
    /// Batch identifier.
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    /// Number of successfully imported records.
    pub imported: usize,
    /// Detailed list of records that failed to import.
    pub failed: Vec<MarcBatchImportError>,
}

/// Service handling MARC batch parsing, caching and import.
#[derive(Clone)]
pub struct MarcService {
    catalog: CatalogService,
    redis: RedisService,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EnqueueResult {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    pub items: Vec<ItemShort>,
}

impl MarcService {
    pub fn new(catalog: CatalogService, redis: RedisService) -> Self {
        Self { catalog, redis }
    }

    fn redis_key(batch_id: i64, record_id: usize) -> String {
        format!("marc:record:{}:{}", batch_id, record_id)
    }

    /// Read UNIMARC binary data, cache each record in Redis and return a
    /// preview as `ItemShort`.
    ///
    /// - `data`: raw UNIMARC bytes (ISO 2709, UTF-8).
    /// - `source_id`: identifier of the logical source to attach to this batch.
    ///
    /// Returns the generated `batch_id` and the list of preview items.
    pub async fn enqueue_unimarc_batch(
        &self,
        data: &[u8],
        source_id: i64,
    ) -> AppResult<EnqueueResult> {
        let format_encoding = FormatEncoding::new(MarcFormat::Unimarc, Encoding::Utf8);
        let records = parse(data, format_encoding)
            .map_err(|e| AppError::Validation(format!("UNIMARC parse error: {}", e)))?;

        let batch_id: i64 = snowflaked::Generator::new(1).generate::<i64>();

        let mut conn = self.redis.get_connection().await?;

        let mut items_short = Vec::with_capacity(records.len());

        for (idx, record) in records.into_iter().enumerate() {
            let key = Self::redis_key(batch_id, idx);

            let cached = CachedMarcRecord {
                source_id,
                record: record.clone(),
            };

            let json_str = serde_json::to_string(&cached)
                .map_err(|e| AppError::Internal(format!("Failed to serialize MARC record: {}", e)))?;

            // Store record
            redis::cmd("SETEX")
            .arg(&key)
            .arg(60 * 60 * 24) // 24 hours
            .arg(&json_str)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to store MARC record in Redis: {}", e)))?;
        
          
            // Build preview item from MARC record.
            let mut item: Item = record.into();
            item.id.replace(idx as i64);

            let short = ItemShort::from(item);
            items_short.push(short);
        }

        Ok(EnqueueResult { batch_id, items: items_short })
    }

    /// Import MARC records from a cached batch into the catalog.
    ///
    /// - If `record_id` is `None`, imports all records for the `batch_id`.
    /// - If `record_id` is `Some`, imports only the specified record.
    ///
    /// On error for a given record, the error is captured in the report and
    /// processing continues with the next record.
    pub async fn import_from_batch(
        &self,
        batch_id: i64,
        record_id: Option<usize>,
    ) -> AppResult<MarcBatchImportReport> {
        let mut conn = self.redis.get_connection().await?;

        let keys: Vec<String> = if let Some(rid) = record_id {
            vec![Self::redis_key(batch_id, rid)]
        } else {
            let pattern = format!("marc:record:{}:*", batch_id);
            redis::cmd("KEYS")
                .arg(&pattern)
                .query_async::<_, Vec<String>>(&mut conn)
                .await
                .map_err(|e| {
                    AppError::Internal(format!("Failed to list MARC batch keys in Redis: {}", e))
                })?
        };

        let mut imported = 0usize;
        let mut failed = Vec::new();

        for key in keys {
            let json_str: Option<String> = conn
                .get(&key)
                .await
                .map_err(|e| {
                    AppError::Internal(format!("Failed to get MARC record from Redis: {}", e))
                })?;

            let Some(json_str) = json_str else {
                failed.push(MarcBatchImportError {
                    record_key: key.clone(),
                    error: "Record not found in Redis".to_string(),
                });
                continue;
            };

            let cached: CachedMarcRecord = match serde_json::from_str(&json_str) {
                Ok(v) => v,
                Err(e) => {
                    failed.push(MarcBatchImportError {
                        record_key: key.clone(),
                        error: format!("JSON decode error: {}", e),
                    });
                    continue;
                }
            };

           
            match self.catalog.create_item(cached.record.into(), Some(cached.source_id), false, None).await {
                Ok((_created, _report)) => {
                    imported += 1;
                }
                Err(e) => {
                    failed.push(MarcBatchImportError {
                        record_key: key.clone(),
                        error: format!("{}", e),
                    });
                }
            }
        }

        Ok(MarcBatchImportReport {
            batch_id,
            imported,
            failed,
        })
    }
}

