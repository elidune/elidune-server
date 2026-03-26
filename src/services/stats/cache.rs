//! Optional Redis cache for identical stats builder payloads.

use sha2::{Digest, Sha256};

use crate::error::AppResult;
use crate::models::stats_builder::{StatsBuilderBody, StatsTableResponse};
use crate::services::redis::RedisService;

const CACHE_TTL_SECS: u64 = 300;
const CACHE_PREFIX: &str = "elidune:stats:";

pub fn cache_key(query: &StatsBuilderBody) -> String {
    let json = serde_json::to_string(query).unwrap_or_default();
    let hash = hex::encode(Sha256::digest(json.as_bytes()));
    format!("{}{}", CACHE_PREFIX, hash)
}

pub async fn get(redis: &RedisService, key: &str) -> Option<StatsTableResponse> {
    let mut conn = redis.get_connection().await.ok()?;
    use redis::AsyncCommands;
    let data: Option<String> = conn.get(key).await.ok()?;
    data.and_then(|s| serde_json::from_str(&s).ok())
}

pub async fn set(redis: &RedisService, key: &str, response: &StatsTableResponse) -> AppResult<()> {
    let json = serde_json::to_string(response)
        .map_err(|e| crate::error::AppError::Internal(format!("Cache serialize: {}", e)))?;
    let mut conn = redis.get_connection().await?;
    use redis::AsyncCommands;
    conn.set_ex::<_, _, ()>(key, json, CACHE_TTL_SECS)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Cache set: {}", e)))?;
    Ok(())
}
