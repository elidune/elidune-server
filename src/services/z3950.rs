//! Z39.50 client service for remote catalog searches
//!
//! Uses the z3950-rs crate for Z39.50 protocol communication.

use serde_json;
use redis::AsyncCommands;

use z3950_rs::marc_rs::{ MarcFormat, Record as MarcRecord};
use z3950_rs::{Client, QueryLanguage};
use crate::{
    api::z3950::{ImportItem, Z3950SearchQuery, Z3950ServerConfig},
    error::{AppError, AppResult},
    models::{
        biblio::{Biblio, Isbn},
        import_report::{ImportAction, ImportReport},
        item::Item,
    },
    repository::Repository,
    services::catalog::CatalogService,
    services::redis::RedisService,
};

/// Z39.50 server configuration (from `z3950servers` row) for connect / query.
#[derive(Debug, Clone)]
pub struct Z3950Server {
    pub id: i64,
    pub name: String,
    pub address: String,
    pub port: i32,
    pub database: String,
    pub login: Option<String>,
    pub password: Option<String>,
    #[allow(dead_code)]
    pub format: Option<MarcFormat>,
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

        let server_rows = self
            .repository
            .z3950_servers_list_active_for_search(query.server_id)
            .await?;

        if server_rows.is_empty() {
            tracing::warn!("No active Z39.50 servers found in database");
            return Err(AppError::Z3950("No active Z39.50 servers configured".to_string()));
        }

        let servers: Vec<Z3950Server> = server_rows
            .into_iter()
            .map(|row| Z3950Server {
                id: row.id,
                name: row.name.unwrap_or_default(),
                address: row.address.unwrap_or_default(),
                port: row.port.unwrap_or(2200),
                database: row.database.unwrap_or_default(),
                format: None,
                login: row.login,
                password: row.password,
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

    /// Load one **active** Z39.50 server by id (same filter as search).
    #[tracing::instrument(skip(self), err)]
    pub async fn load_active_server(&self, server_id: i64) -> AppResult<Z3950Server> {
        let rows = self
            .repository
            .z3950_servers_list_active_for_search(Some(server_id))
            .await?;
        let row = rows.into_iter().next().ok_or_else(|| {
            AppError::NotFound("Z39.50 server not found or not active".to_string())
        })?;
        Ok(Z3950Server {
            id: row.id,
            name: row.name.unwrap_or_default(),
            address: row.address.unwrap_or_default(),
            port: row.port.unwrap_or(2200),
            database: row.database.unwrap_or_default(),
            format: None,
            login: row.login,
            password: row.password,
        })
    }

    /// Open a TCP/Z39.50 session to the given server. Caller must [`Client::close`] when finished.
    #[tracing::instrument(skip(server), fields(server = %server.name))]
    pub async fn connect_server(server: &Z3950Server) -> AppResult<Client> {
        let addr = format!("{}:{}", server.address, server.port);
        tracing::debug!("Z39.50 connect: {} (database: {})", addr, server.database);

        let credentials = if let (Some(ref login), Some(ref password)) = (&server.login, &server.password) {
            Some((login.as_str(), password.as_str()))
        } else {
            None
        };

        let client = if let Some((login, password)) = credentials {
            Client::connect_with_credentials(&addr, Some((login, password)))
                .await
                .map_err(|e| {
                    tracing::warn!("Failed to connect to Z39.50 server {}: {}", server.name, e);
                    AppError::Z3950(format!("Failed to connect to Z39.50 server: {}", e))
                })?
        } else {
            Client::connect(&addr).await.map_err(|e| {
                tracing::warn!("Failed to connect to Z39.50 server {}: {}", server.name, e);
                AppError::Z3950(format!("Failed to connect to Z39.50 server: {}", e))
            })?
        };

        Ok(client)
    }

    /// CQL search + MARC present on an **existing** connection. Does **not** close the client.
    #[tracing::instrument(skip(client, query), fields(server = %server.name))]
    pub async fn query(
        client: &mut Client,
        server: &Z3950Server,
        query: &Z3950SearchQuery,
    ) -> AppResult<Vec<MarcRecord>> {
        tracing::debug!("Z39.50 query: {:?}", query);

        let databases = if server.database.is_empty() {
            &["default" as &str]
        } else {
            &[server.database.as_str()]
        };

        let search_response = client
            .search(databases, QueryLanguage::CQL(query.query.clone()))
            .await
            .map_err(|e| {
                tracing::warn!("Z39.50 search failed on {}: {}", server.name, e);
                AppError::Z3950(format!("Z39.50 search failed: {}", e))
            })?;

        let hits = usize::try_from(&search_response.result_count).unwrap_or_else(|_| {
            search_response
                .result_count
                .to_string()
                .parse::<usize>()
                .unwrap_or(0)
        });
        tracing::debug!("Z39.50 search returned {} hits on {}", hits, server.name);

        if hits == 0 {
            return Ok(Vec::new());
        }

        let count = std::cmp::min(hits, query.max_results.unwrap_or(50) as usize);
        let records = client
            .present_marc(1, count as i64)
            .await
            .map_err(|e| {
                tracing::warn!("Z39.50 present failed on {}: {}", server.name, e);
                AppError::Z3950(format!("Z39.50 present failed: {}", e))
            })?;

        tracing::info!("z3950-rs returned {} MARC records from {}", records.len(), server.name);
        Ok(records)
    }

    /// Connect, search, present, then close — convenience for one-shot calls.
    pub(crate) async fn query_server(
        &self,
        server: &Z3950Server,
        query: &Z3950SearchQuery,
    ) -> AppResult<Vec<MarcRecord>> {
        tracing::info!("Z39.50 search starting on server: {}", server.name);
        let mut client = Self::connect_server(server).await?;
        let out = Self::query(&mut client, server, query).await;
        let _ = client.close().await;
        out
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

    

    /// Staff UI: all Z39.50 server rows.
    pub async fn get_servers_for_settings(&self) -> AppResult<Vec<Z3950ServerConfig>> {
        let rows = self.repository.z3950_servers_list_all().await?;
        Ok(rows
            .into_iter()
            .map(|r| Z3950ServerConfig {
                id: r.id,
                name: r.name.unwrap_or_default(),
                address: r.address.unwrap_or_default(),
                port: r.port.unwrap_or(2200),
                database: r.database,
                format: r.format,
                login: r.login,
                password: r.password,
                encoding: r.encoding.unwrap_or_else(|| "utf-8".to_string()),
                is_active: r.activated.unwrap_or(false),
            })
            .collect())
    }

    /// Staff UI: upsert Z39.50 servers (id &gt; 0 update, id == 0 insert).
    pub async fn update_servers_for_settings(
        &self,
        servers: Vec<Z3950ServerConfig>,
    ) -> AppResult<Vec<Z3950ServerConfig>> {
        for server in servers {
            if server.id > 0 {
                self.repository
                    .z3950_server_update(
                        server.id,
                        &server.name,
                        &server.address,
                        server.port,
                        &server.database,
                        &server.format,
                        &server.login,
                        &server.password,
                        &server.encoding,
                        server.is_active,
                    )
                    .await?;
            } else {
                self.repository
                    .z3950_server_insert(
                        &server.name,
                        &server.address,
                        server.port,
                        &server.database,
                        &server.format,
                        &server.login,
                        &server.password,
                        &server.encoding,
                        server.is_active,
                    )
                    .await?;
            }
        }
        self.get_servers_for_settings().await
    }
}


