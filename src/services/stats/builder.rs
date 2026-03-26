//! Run validated builder queries (cache → SQL → executor).

use sqlx::PgPool;

use crate::error::AppResult;
use crate::models::stats_builder::{StatsBuilderBody, StatsTableResponse};
use crate::services::redis::RedisService;

use super::{cache, executor, query_builder, validator};

/// Execute a flexible stats query with optional Redis caching.
pub async fn run_stats_query(
    pool: &PgPool,
    redis: Option<&RedisService>,
    body: &StatsBuilderBody,
) -> AppResult<StatsTableResponse> {
    validator::validate(body)?;

    let key = cache::cache_key(body);
    if let Some(r) = redis {
        if let Some(cached) = cache::get(r, &key).await {
            return Ok(cached);
        }
    }

    let built = query_builder::build_sql(body)?;
    let limit = body.limit.unwrap_or(1000).min(10_000);
    let offset = body.offset.unwrap_or(0);

    let response = executor::execute(
        pool,
        &built.data_sql,
        &built.count_sql,
        &built.binds,
        limit,
        offset,
    )
    .await?;

    if let Some(r) = redis {
        let _ = cache::set(r, &key, &response).await;
    }

    Ok(response)
}
