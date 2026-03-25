//! MARC batch import service.
//!
//! This service allows reading raw UNIMARC data, caching individual MARC
//! records in Redis under `marc:record:<BATCH_ID>:<ID>` keys, and importing
//! them into the local catalog.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;
use z3950_rs::marc_rs::{Encoding, MarcFormat, parse_records};

use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        biblio::{Biblio, BiblioShort},
        item::Item,
    },
};

use super::{catalog::CatalogService, redis::RedisService, task_manager::TaskHandle};

/// Internal value stored in Redis for a MARC record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMarcRecord {
    /// Source identifier chosen by caller (e.g. Z39.50 server, file source).
    pub source_id: i64,
    /// Raw MARC record as parsed by marc-rs.
    pub record: MarcRecord,
}

/// Error information for a single record during batch import.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarcBatchImportError {
    pub key: String,
    /// Human-readable error message.
    pub error: String,
    /// ID of the existing biblio when the failure is a duplicate ISBN conflict.
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<i64>,
}

/// Report returned after importing one or more records from a batch.
#[serde_as]
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarcBatchImportReport {
    /// Batch identifier.
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    /// Number of successfully imported records.
    pub imported: Vec<String>,
    /// Detailed list of records that failed to import.
    pub failed: Vec<MarcBatchImportError>,
}

/// Summary of a MARC batch cached in Redis.
#[serde_as]
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarcBatchInfo {
    /// Unique batch identifier (Snowflake ID).
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    /// Number of records in this batch.
    pub record_count: usize,
    /// Remaining TTL in seconds (-1 if no expiry, -2 if key gone).
    pub ttl_seconds: i64,
}

/// Service handling MARC batch parsing, caching and import.
#[derive(Clone)]
pub struct MarcService {
    catalog: CatalogService,
    redis: RedisService,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueResult {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    pub biblios: Vec<BiblioShort>,
}

impl MarcService {
    pub fn new(catalog: CatalogService, redis: RedisService) -> Self {
        Self { catalog, redis }
    }

    fn redis_key(batch_id: i64, record_id: usize) -> String {
        format!("marc:record:{}:{}", batch_id, record_id)
    }

    fn item_key_from_record_key(record_key: &str) -> String {
        record_key.rsplit(':').next().unwrap().to_string()
    }

    /// Read UNIMARC binary data, cache each record in Redis and return a
    /// preview as `ItemShort`.
    ///
    /// - `data`: raw UNIMARC bytes (ISO 2709, UTF-8).
    /// - `source_id`: identifier of the logical source to attach to this batch.
    ///
    /// Returns the generated `batch_id` and the list of preview items.
    #[tracing::instrument(skip(self), err)]
    pub async fn enqueue_unimarc_batch(
        &self,
        data: &[u8],
    ) -> AppResult<EnqueueResult> {


        let records = parse_records(&data).map_err(|e| AppError::Validation(format!("UNIMARC parse error: {}", e)))?;


        let batch_id: i64 = snowflaked::Generator::new(1).generate::<i64>();

        let mut conn = self.redis.get_connection().await?;

        let mut biblios_short = Vec::with_capacity(records.len());
        let mut index = 0;

        for record in records.into_iter() {

            
            for (idx, _) in record.local.items.iter().enumerate() {

                let mut record = record.clone();

                record.local.items.swap(0, idx);
                record.local.items.truncate(1);

                let json_str: String = serde_json::to_string(&record)
                .map_err(|e| AppError::Internal(format!("Failed to serialize MARC record: {}", e)))?;

                // Store record
                redis::cmd("SETEX")
                .arg(&Self::redis_key(batch_id, index))
                .arg(60 * 60 * 24) // 24 hours
                .arg(&json_str)
                .query_async::<_, ()>(&mut conn)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to store MARC record in Redis: {}", e)))?;

                // Build preview biblio from MARC record.
                let mut biblio: Biblio = record.into();
                biblio.id.replace(index as i64);

                let short = BiblioShort::from(biblio);
                biblios_short.push(short);

                index += 1;
            }


          
        }

        Ok(EnqueueResult { batch_id, biblios: biblios_short })
    }

    /// List all MARC batches currently cached in Redis.
    ///
    /// Scans keys matching `marc:record:*`, groups them by `batch_id`, and
    /// fetches the TTL of one representative key per batch.
    #[tracing::instrument(skip(self), err)]
    pub async fn list_marc_batches(&self) -> AppResult<Vec<MarcBatchInfo>> {
        let mut conn = self.redis.get_connection().await?;

        let all_keys: Vec<String> = redis::cmd("KEYS")
            .arg("marc:record:*")
            .query_async::<_, Vec<String>>(&mut conn)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to list MARC batch keys in Redis: {}", e)))?;

        // Group keys by batch_id. Key format: marc:record:<batch_id>:<idx>
        let mut batch_map: std::collections::HashMap<i64, (usize, String)> = std::collections::HashMap::new();
        for key in &all_keys {
            let parts: Vec<&str> = key.splitn(4, ':').collect();
            if parts.len() != 4 {
                continue;
            }
            let Ok(batch_id) = parts[2].parse::<i64>() else {
                continue;
            };
            let entry = batch_map.entry(batch_id).or_insert((0, key.clone()));
            entry.0 += 1;
        }

        let mut batches = Vec::with_capacity(batch_map.len());
        for (batch_id, (record_count, sample_key)) in batch_map {
            let ttl_seconds: i64 = redis::cmd("TTL")
                .arg(&sample_key)
                .query_async::<_, i64>(&mut conn)
                .await
                .unwrap_or(-2);

            batches.push(MarcBatchInfo { batch_id, record_count, ttl_seconds });
        }

        // Sort newest first (higher Snowflake ID = newer).
        batches.sort_by(|a, b| b.batch_id.cmp(&a.batch_id));

        Ok(batches)
    }

    /// Reload a cached MARC batch from Redis and return it as an [`EnqueueResult`].
    ///
    /// Useful to re-display a batch that was uploaded earlier without re-uploading
    /// the file.
    #[tracing::instrument(skip(self), err)]
    pub async fn load_marc_batch(&self, batch_id: i64) -> AppResult<EnqueueResult> {
        let mut conn = self.redis.get_connection().await?;

        let pattern = format!("marc:record:{}:*", batch_id);
        let mut keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async::<_, Vec<String>>(&mut conn)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to list MARC batch keys in Redis: {}", e)))?;

        if keys.is_empty() {
            return Err(AppError::NotFound(format!("MARC batch {} not found in cache", batch_id)));
        }

        // Sort by record index so the preview order matches the original upload.
        keys.sort_by_key(|k| {
            k.rsplit(':').next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0)
        });

        let mut biblios_short = Vec::with_capacity(keys.len());

        for key in &keys {
            let record_idx: usize = key
                .rsplit(':')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let json_str: Option<String> = conn
                .get(key)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to get MARC record from Redis: {}", e)))?;

            let Some(json_str) = json_str else {
                continue;
            };

            let record: MarcRecord = serde_json::from_str(&json_str)
                .map_err(|e| AppError::Internal(format!("Failed to deserialize MARC record: {}", e)))?;

            let mut biblio: Biblio = record.into();
            biblio.id.replace(record_idx as i64);

            biblios_short.push(BiblioShort::from(biblio));
        }

        Ok(EnqueueResult { batch_id, biblios: biblios_short })
    }

    /// Import MARC records from a cached batch into the catalog.
    ///
    /// - If `record_id` is `None`, imports all records for the `batch_id`.
    /// - If `record_id` is `Some`, imports only the specified record.
    ///
    /// On error for a given record, the error is captured in the report and
    /// processing continues with the next record.
    ///
    /// When `task_handle` is provided, per-record progress is reported via
    /// [`TaskHandle::set_progress`].
    #[tracing::instrument(skip(self, task_handle), err)]
    pub async fn import_from_batch(
        &self,
        batch_id: i64,
        source_id: i64,
        record_id: Option<usize>,
        allow_duplicate_isbn: bool,
        confirm_replace_existing_id: Option<i64>,
        task_handle: Option<TaskHandle>,
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

        let mut imported = Vec::new();
        let mut failed = Vec::new();
        let total = keys.len();

        for (idx, key) in keys.iter().enumerate() {
            let key = key.clone();
            
            
            
            let json_str: Option<String> = conn
                .get(&key)
                .await
                .map_err(|e| {
                    AppError::Internal(format!("Failed to get MARC record from Redis: {}", e))
                })?;

            let Some(json_str) = json_str else {
                failed.push(MarcBatchImportError {
                    key: Self::item_key_from_record_key(&key),
                    error: "Record not found in Redis".to_string(),
                    existing_id: None,
                });
                continue;
            };

            let record: MarcRecord = match serde_json::from_str(&json_str) {
                Ok(v) => v,
                Err(e) => {
                    failed.push(MarcBatchImportError {
                        key: Self::item_key_from_record_key(&key),
                        error: format!("JSON decode error: {}", e),
                        existing_id: None,
                    });
                    continue;
                }
            };

            let mut biblio: Biblio = record.into();
            for item in &mut biblio.items {
                item.source_id = Some(source_id);
            }

            match self.catalog.create_biblio(biblio, allow_duplicate_isbn, confirm_replace_existing_id).await {
                Ok((_biblio, _report)) => {
                    imported.push(Self::item_key_from_record_key(&key));
                }
                Err(AppError::DuplicateNeedsConfirmation { existing_id, message, .. }) => {
                    failed.push(MarcBatchImportError {
                        key: Self::item_key_from_record_key(&key),
                        error: message,
                        existing_id: Some(existing_id),
                    });
                    continue;
                }
                Err(e) => {
                    failed.push(MarcBatchImportError {
                        key: Self::item_key_from_record_key(&key),
                        error: format!("Failed to create item: {}", e),
                        existing_id: None,
                    });
                    continue;
                }
            }
            if let Some(ref handle) = task_handle {
                handle
                    .set_progress(
                        idx + 1,
                        total,
                        Some(serde_json::json!({
                            "imported": imported,
                            "failed": failed,
                        }))
                    )
                    .await;
            }

        }


        Ok(MarcBatchImportReport {
            batch_id,
            imported,
            failed,
        })
    }
}

