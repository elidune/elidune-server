//! Z39.50 client service for remote catalog searches
//!
//! Uses the z3950-rs crate for Z39.50 protocol communication.

use sqlx::Row;
use serde_json;
use redis::AsyncCommands;

use z3950_rs::marc_rs::{ MarcFormat, Record as MarcRecord};
use z3950_rs::{Client, QueryLanguage};
use crate::{
    api::z3950::{ImportItem, Z3950SearchQuery},
    error::{AppError, AppResult},
    models::{
        biblio::Biblio,
        import_report::{ImportAction, ImportReport},
        item::Item,
    },
    repository::Repository,
    services::catalog::CatalogService,
    services::redis::RedisService,
};

/// Z39.50 server configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Z3950Server {
    id: i64,
    name: String,
    address: String,
    port: i32,
    database: String,
    login: Option<String>,
    password: Option<String>,
    format: Option<MarcFormat>,
}

#[derive(Clone)]
pub struct Z3950Service {
    repository: Repository,
    catalog: CatalogService,
    redis: RedisService,
    cache_ttl_seconds: u64,
}

impl Z3950Service {
    pub fn new(
        repository: Repository,
        catalog: CatalogService,
        redis: RedisService,
        cache_ttl_seconds: u64,
    ) -> Self {
        Self { repository, catalog, redis, cache_ttl_seconds }
    }

    /// Search remote catalogs via Z39.50
    #[tracing::instrument(skip(self), err)]
    pub async fn search(&self, query: &Z3950SearchQuery) -> AppResult<(Vec<Biblio>, i32, String)> {
        tracing::info!("Z39.50 search started");
        tracing::debug!("Search params - query: {}", query.query);

        let pool = &self.repository.pool;

        // Get active Z39.50 servers
        let server_query = if let Some(server_id) = query.server_id {
            tracing::debug!("Searching specific server ID: {}", server_id);
            sqlx::query(
                "SELECT id, name, address, port, database, format, login, password, encoding FROM z3950servers WHERE id = $1 AND activated = 1"
            )
            .bind(server_id)
        } else {
            tracing::debug!("Searching all active servers");
            sqlx::query(
                "SELECT id, name, address, port, database, format, login, password, encoding FROM z3950servers WHERE activated = 1"
            )
        };

        let server_rows = server_query.fetch_all(pool).await?;

        if server_rows.is_empty() {
            tracing::warn!("No active Z39.50 servers found in database");
            return Err(AppError::Z3950("No active Z39.50 servers configured".to_string()));
        }

        let servers: Vec<Z3950Server> = server_rows
            .iter()
            .map(|row| Z3950Server {
                id: row.get("id"),
                name: row.get("name"),
                address: row.get("address"),
                port: row.get("port"),
                database: row.get("database"),
                format: None,
                login: row.get("login"),
                password: row.get("password"),
            })
            .collect();

        tracing::info!("Found {} active Z39.50 servers: {:?}", 
            servers.len(), 
            servers.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        // Build PQF query string
        let max_results = query.max_results.unwrap_or(50) as usize;
        

        let mut all_biblios = Vec::new();
        let mut sources = Vec::new();
        let search_start = std::time::Instant::now();

        // Query each server
        for (idx, server) in servers.iter().enumerate() {
            tracing::info!("Querying server {}/{}: {}", idx + 1, servers.len(), server.name);
            
            match self.query_server(server, &query).await {
                Ok(records) => {
                    tracing::info!("Server {} returned {} records", server.name, records.len());
                    
                    if !records.is_empty() {
                        sources.push(server.name.clone());
                        let len = records.len();
                        for (rec_idx, record) in records.into_iter().enumerate() {
                            tracing::debug!("Processing record {}/{}", rec_idx + 1, len);
                            
                            match self.upsert_cache_record(&record).await {
                                Ok(id) => {
                                    tracing::debug!("Cached record as remote_biblio id={:?}", id);
                                    let mut biblio = Biblio::from(record);
                                    biblio.id = Some(id.parse::<i64>().unwrap_or(0));
                                    all_biblios.push(biblio);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to cache record {}: {}", rec_idx + 1, e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to query server {}: {}", server.name, e);
                }
            }

            // Stop if we have enough results
            if all_biblios.len() >= max_results {
                tracing::debug!("Reached max results ({}), stopping server queries", max_results);
                break;
            }
        }

        let search_elapsed = search_start.elapsed();
        tracing::info!("Z39.50 live search completed in {:?}, found {} biblios", search_elapsed, all_biblios.len());

        let total = all_biblios.len() as i32;
        let source = if sources.is_empty() {
            "cache".to_string()
        } else {
            sources.join(", ")
        };

        tracing::info!("Z39.50 search complete: {} results from {}", total, source);
        Ok((all_biblios, total, source))
    }

  

    /// Query a single Z39.50 server using z3950-rs
    async fn query_server(
        &self,
        server: &Z3950Server,
        query: &Z3950SearchQuery,
    ) -> AppResult<Vec<MarcRecord>> {
        // Build connection address: host:port
        let addr = format!("{}:{}", server.address, server.port);
        
        tracing::info!("Z39.50 search starting on server: {}", server.name);
        tracing::debug!("Z39.50 connection: {} (database: {})", addr, server.database);
        tracing::debug!("Z39.50 query: {:?}", query);

        // Connect to server
        let credentials = if let (Some(ref login), Some(ref password)) = (&server.login, &server.password) {
            Some((login.as_str(), password.as_str()))
        } else {
            None
        };

        let mut client = if let Some((login, password)) = credentials {
            Client::connect_with_credentials(&addr, Some((login, password))).await
                .map_err(|e| {
                    tracing::warn!("Failed to connect to Z39.50 server {}: {}", server.name, e);
                    AppError::Z3950(format!("Failed to connect to Z39.50 server: {}", e))
                })?
        } else {
            Client::connect(&addr).await
                .map_err(|e| {
                    tracing::warn!("Failed to connect to Z39.50 server {}: {}", server.name, e);
                    AppError::Z3950(format!("Failed to connect to Z39.50 server: {}", e))
                })?
        };

        // Search
        let databases = if server.database.is_empty() {
            &["default" as &str]
        } else {
            &[server.database.as_str()]
        };

        let search_response = client.search(databases, QueryLanguage::CQL(query.query.clone())).await
            .map_err(|e| {
                tracing::warn!("Z39.50 search failed on {}: {}", server.name, e);
                AppError::Z3950(format!("Z39.50 search failed: {}", e))
            })?;

        // Convert Integer to usize
        let hits = usize::try_from(&search_response.result_count)
            .unwrap_or_else(|_| search_response.result_count.to_string().parse::<usize>().unwrap_or(0));
        tracing::debug!("Z39.50 search returned {} hits on {}", hits, server.name);

        if hits == 0 {
            let _ = client.close().await;
            return Ok(Vec::new());
        }

        // Present records
        let count = std::cmp::min(hits, query.max_results.unwrap_or(50) as usize);
        let records = client.present_marc(1, count as i64).await
            .map_err(|e| {
                tracing::warn!("Z39.50 present failed on {}: {}", server.name, e);
                AppError::Z3950(format!("Z39.50 present failed: {}", e))
            })?;

        // Close connection
        let _ = client.close().await;

        // z3950_rs::MarcRecord is already a marc_rs::Record
        tracing::info!("z3950-rs returned {} MARC records from {}", records.len(), server.name);
        Ok(records)
    }


    /// Get Redis key for a cached item
    fn get_redis_key(id: &i64) -> String {
        format!("z3950:item:{}", id)
    }

   
    /// Upsert a MARC record in Redis cache and return ItemRemoteShort
    async fn upsert_cache_record(
        &self,
        record: &MarcRecord,
    ) -> AppResult<String> {

        
              
        // Serialize to JSON and store in Redis
        let json_str = serde_json::to_string(&record)
            .map_err(|e| AppError::Internal(format!("Failed to serialize item to JSON: {}", e)))?;
        
        let mut conn = self.redis.get_connection().await?;
        

        let id: i64 = snowflaked::Generator::new(1).generate::<i64>();

        // Store record
        redis::cmd("SETEX")
            .arg(&Self::get_redis_key(&id))
            .arg(self.cache_ttl_seconds)
            .arg(&json_str)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to store item in Redis: {}", e)))?;
        
      
        
        tracing::debug!("Cached item in Redis with key: {}, TTL: {}s", id, self.cache_ttl_seconds);
        
        // Convert to ItemRemoteShort (return string key for API)
        Ok(id.to_string())
    }

    /// Search in cached items from Redis


  

    /// Import a record from Z39.50 cache into local catalog.
    /// Applies ISBN deduplication via CatalogService::create_biblio; then creates physical items when action is Created.
    #[tracing::instrument(skip(self), err)]
    pub async fn import_record(
        &self,
        biblio_id: i64,
        items: Option<Vec<ImportItem>>,
        confirm_replace_existing_id: Option<i64>,
    ) -> AppResult<(Biblio, ImportReport)> {
        let mut conn = self.redis.get_connection().await?;

        let redis_key = Self::get_redis_key(&biblio_id);
        let json_str: Option<String> = conn
            .get(&redis_key)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get biblio from Redis: {}", e)))?;

        let marc_record: MarcRecord = serde_json::from_str(
            &json_str.ok_or_else(|| AppError::NotFound("Remote biblio not found in cache".to_string()))?
        )
        .map_err(|e| AppError::Internal(format!("Failed to deserialize biblio from Redis: {}", e)))?;

        let biblio: Biblio = marc_record.into();
        let (mut biblio, report) = self
            .catalog
            .create_biblio(biblio, false, confirm_replace_existing_id)
            .await?;

        if report.action == ImportAction::Created {
            if let (Some(items_list), Some(created_biblio_id)) = (items, biblio.id) {
                for s in items_list {
                    let item: Item = s.into();
                    let _ = self.catalog.create_item(created_biblio_id, item).await?;
                }
                biblio = self
                    .repository
                    .biblios_get_by_id(created_biblio_id)
                    .await?;
            }
        }

        Ok((biblio, report))
    }
}


