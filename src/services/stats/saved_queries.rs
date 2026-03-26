//! Persisted stats queries (`saved_queries` table).

use sqlx::PgPool;

use crate::error::AppError;
use crate::models::stats_builder::{SavedStatsQuery, SavedStatsQueryWrite, StatsBuilderBody};

pub async fn list_for_user(
    pool: &PgPool,
    user_id: i64,
    is_admin: bool,
) -> Result<Vec<SavedStatsQuery>, AppError> {
    let rows: Vec<SavedStatsQueryRow> = if is_admin {
        sqlx::query_as::<_, SavedStatsQueryRow>(
            r#"
            SELECT id, name, description, query_json, user_id, is_shared, created_at, updated_at
            FROM saved_queries
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, SavedStatsQueryRow>(
            r#"
            SELECT id, name, description, query_json, user_id, is_shared, created_at, updated_at
            FROM saved_queries
            WHERE user_id = $1 OR is_shared = TRUE
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| AppError::Internal(format!("List saved queries: {}", e)))?;

    rows.into_iter()
        .map(row_to_public)
        .collect::<Result<Vec<_>, _>>()
}

pub async fn get_by_id(
    pool: &PgPool,
    id: i64,
    user_id: i64,
    is_admin: bool,
) -> Result<Option<SavedStatsQuery>, AppError> {
    let row = sqlx::query_as::<_, SavedStatsQueryRow>(
        r#"
        SELECT id, name, description, query_json, user_id, is_shared, created_at, updated_at
        FROM saved_queries WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(format!("Get saved query: {}", e)))?;

    let Some(r) = row else {
        return Ok(None);
    };

    if !is_admin && r.user_id != user_id && !r.is_shared {
        return Err(AppError::Authorization(
            "Cannot access this saved query".into(),
        ));
    }

    Ok(Some(row_to_public(r)?))
}

pub async fn insert(
    pool: &PgPool,
    user_id: i64,
    body: &SavedStatsQueryWrite,
) -> Result<SavedStatsQuery, AppError> {
    let query_json = serde_json::to_value(&body.query)
        .map_err(|e| AppError::Internal(format!("Serialize query: {}", e)))?;

    let row = sqlx::query_as::<_, SavedStatsQueryRow>(
        r#"
        INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, description, query_json, user_id, is_shared, created_at, updated_at
        "#,
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(query_json)
    .bind(user_id)
    .bind(body.is_shared)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(format!("Insert saved query: {}", e)))?;

    row_to_public(row)
}

pub async fn update(
    pool: &PgPool,
    id: i64,
    user_id: i64,
    is_admin: bool,
    body: &SavedStatsQueryWrite,
) -> Result<SavedStatsQuery, AppError> {
    let owner = sqlx::query_scalar::<_, i64>("SELECT user_id FROM saved_queries WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("Saved query lookup: {}", e)))?
        .ok_or_else(|| AppError::NotFound("Saved query not found".into()))?;

    if !is_admin && owner != user_id {
        return Err(AppError::Authorization(
            "Only the owner can update this saved query".into(),
        ));
    }

    let query_json = serde_json::to_value(&body.query)
        .map_err(|e| AppError::Internal(format!("Serialize query: {}", e)))?;

    let row = sqlx::query_as::<_, SavedStatsQueryRow>(
        r#"
        UPDATE saved_queries
        SET name = $2, description = $3, query_json = $4, is_shared = $5, updated_at = NOW()
        WHERE id = $1
        RETURNING id, name, description, query_json, user_id, is_shared, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(query_json)
    .bind(body.is_shared)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(format!("Update saved query: {}", e)))?;

    row_to_public(row)
}

pub async fn delete_by_id(
    pool: &PgPool,
    id: i64,
    user_id: i64,
    is_admin: bool,
) -> Result<(), AppError> {
    let owner = sqlx::query_scalar::<_, i64>("SELECT user_id FROM saved_queries WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("Saved query lookup: {}", e)))?
        .ok_or_else(|| AppError::NotFound("Saved query not found".into()))?;

    if !is_admin && owner != user_id {
        return Err(AppError::Authorization(
            "Only the owner can delete this saved query".into(),
        ));
    }

    sqlx::query("DELETE FROM saved_queries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Internal(format!("Delete saved query: {}", e)))?;

    Ok(())
}

#[derive(sqlx::FromRow)]
struct SavedStatsQueryRow {
    id: i64,
    name: String,
    description: Option<String>,
    query_json: serde_json::Value,
    user_id: i64,
    is_shared: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn row_to_public(row: SavedStatsQueryRow) -> Result<SavedStatsQuery, AppError> {
    let query: StatsBuilderBody = serde_json::from_value(row.query_json)
        .map_err(|e| AppError::Internal(format!("Invalid stored query_json: {}", e)))?;
    Ok(SavedStatsQuery {
        id: row.id,
        name: row.name,
        description: row.description,
        query,
        user_id: row.user_id,
        is_shared: row.is_shared,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
