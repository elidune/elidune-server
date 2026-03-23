//! Sources domain methods on Repository

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres};

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::source::Source,
};

// Note: not `mockall::automock` — trait has `&str` parameters that mockall cannot derive for.
#[async_trait]
pub trait SourcesRepository: Send + Sync {
    async fn sources_list(&self, include_archived: bool) -> AppResult<Vec<Source>>;
    async fn sources_get_by_id(&self, id: i64) -> AppResult<Source>;
    async fn sources_rename(&self, id: i64, name: &str) -> AppResult<Source>;
    async fn sources_update(
        &self,
        id: i64,
        name: Option<&str>,
        default: Option<bool>,
    ) -> AppResult<Source>;
    async fn sources_count_active_specimens(&self, source_id: i64) -> AppResult<i64>;
    async fn sources_archive(&self, id: i64) -> AppResult<Source>;
    async fn sources_create(&self, name: &str, default: Option<bool>) -> AppResult<Source>;
    async fn sources_reassign_specimens(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64>;
    async fn sources_reassign_items(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64>;
    async fn sources_archive_many(&self, ids: &[i64]) -> AppResult<()>;
    async fn sources_find_or_create_by_name(&self, name: &str) -> AppResult<i64>;
    async fn sources_get_default(&self) -> AppResult<Option<Source>>;
    /// Expose the underlying pool so service-level transactions can be initiated.
    fn pool(&self) -> &Pool<Postgres>;
}

#[async_trait::async_trait]
impl SourcesRepository for Repository {
    async fn sources_list(&self, include_archived: bool) -> AppResult<Vec<Source>> {
        Repository::sources_list(self, include_archived).await
    }
    async fn sources_get_by_id(&self, id: i64) -> AppResult<Source> {
        Repository::sources_get_by_id(self, id).await
    }
    async fn sources_rename(&self, id: i64, name: &str) -> AppResult<Source> {
        Repository::sources_rename(self, id, name).await
    }
    async fn sources_update(&self, id: i64, name: Option<&str>, default: Option<bool>) -> AppResult<Source> {
        Repository::sources_update(self, id, name, default).await
    }
    async fn sources_count_active_specimens(&self, source_id: i64) -> AppResult<i64> {
        Repository::sources_count_active_specimens(self, source_id).await
    }
    async fn sources_archive(&self, id: i64) -> AppResult<Source> {
        Repository::sources_archive(self, id).await
    }
    async fn sources_create(&self, name: &str, default: Option<bool>) -> AppResult<Source> {
        Repository::sources_create(self, name, default).await
    }
    async fn sources_reassign_specimens(&self, old_source_ids: &[i64], new_source_id: i64) -> AppResult<i64> {
        Repository::sources_reassign_specimens(self, old_source_ids, new_source_id).await
    }
    async fn sources_reassign_items(&self, old_source_ids: &[i64], new_source_id: i64) -> AppResult<i64> {
        Repository::sources_reassign_items(self, old_source_ids, new_source_id).await
    }
    async fn sources_archive_many(&self, ids: &[i64]) -> AppResult<()> {
        Repository::sources_archive_many(self, ids).await
    }
    async fn sources_find_or_create_by_name(&self, name: &str) -> AppResult<i64> {
        Repository::sources_find_or_create_by_name(self, name).await
    }
    async fn sources_get_default(&self) -> AppResult<Option<Source>> {
        Repository::sources_get_default(self).await
    }
    fn pool(&self) -> &sqlx::Pool<sqlx::Postgres> {
        &self.pool
    }
}



impl Repository {
    /// List all sources, optionally filtering by archive status
    pub async fn sources_list(&self, include_archived: bool) -> AppResult<Vec<Source>> {
        let rows = if include_archived {
            sqlx::query_as::<_, Source>("SELECT * FROM sources ORDER BY name")
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query_as::<_, Source>(
                "SELECT * FROM sources WHERE is_archive IS NULL OR is_archive = 0 ORDER BY name",
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows)
    }

    /// Get source by ID
    pub async fn sources_get_by_id(&self, id: i64) -> AppResult<Source> {
        sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))
    }

    /// Rename a source
    pub async fn sources_rename(&self, id: i64, name: &str) -> AppResult<Source> {
        sqlx::query_as::<_, Source>("UPDATE sources SET name = $1 WHERE id = $2 RETURNING *")
            .bind(name)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))
    }

    /// Update a source (name and/or default status).
    ///
    /// When changing the default flag, both the clear-all and the target update are wrapped
    /// in a transaction to keep the invariant that exactly one source is default at all times.
    pub async fn sources_update(&self, id: i64, name: Option<&str>, default: Option<bool>) -> AppResult<Source> {
        if let Some(true) = default {
            let mut tx = self.pool.begin().await?;

            sqlx::query(r#"UPDATE sources SET "default" = false WHERE id != $1"#)
                .bind(id)
                .execute(&mut *tx)
                .await?;

            let source = match name {
                Some(n) => {
                    sqlx::query_as::<_, Source>(
                        r#"UPDATE sources SET name = $1, "default" = true WHERE id = $2 RETURNING *"#
                    )
                    .bind(n)
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
                }
                None => {
                    sqlx::query_as::<_, Source>(
                        r#"UPDATE sources SET "default" = true WHERE id = $1 RETURNING *"#
                    )
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
                }
            };

            tx.commit().await?;
            return source.ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)));
        }

        // No default change — simple update without a transaction.
        let source = match (name, default) {
            (Some(n), Some(dv)) => {
                sqlx::query_as::<_, Source>(
                    r#"UPDATE sources SET name = $1, "default" = $2 WHERE id = $3 RETURNING *"#
                )
                .bind(n)
                .bind(dv)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
            }
            (Some(n), None) => {
                sqlx::query_as::<_, Source>(
                    "UPDATE sources SET name = $1 WHERE id = $2 RETURNING *"
                )
                .bind(n)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
            }
            (None, Some(dv)) => {
                sqlx::query_as::<_, Source>(
                    r#"UPDATE sources SET "default" = $1 WHERE id = $2 RETURNING *"#
                )
                .bind(dv)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
            }
            (None, None) => {
                return Err(AppError::Validation("At least one field must be provided for update".to_string()));
            }
        };

        source.ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))
    }

    /// Count non-archived items (physical copies) linked to a source
    pub async fn sources_count_active_specimens(&self, source_id: i64) -> AppResult<i64> {
        self.biblios_count_items_for_source(source_id).await
    }

    /// Archive a source
    pub async fn sources_archive(&self, id: i64) -> AppResult<Source> {
        let now = Utc::now();
        sqlx::query_as::<_, Source>(
            "UPDATE sources SET is_archive = 1, archived_at = $1 WHERE id = $2 RETURNING *",
        )
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))
    }

    /// Create a new source.
    ///
    /// When `default = Some(true)`, the UPDATE-all + INSERT are wrapped in a transaction
    /// so the default flag is never lost if the INSERT fails.
    pub async fn sources_create(&self, name: &str, default: Option<bool>) -> AppResult<Source> {
        if default == Some(true) {
            let mut tx = self.pool.begin().await?;

            sqlx::query(r#"UPDATE sources SET "default" = false"#)
                .execute(&mut *tx)
                .await?;

            let row = sqlx::query_as::<_, Source>(
                r#"INSERT INTO sources (name, "default") VALUES ($1, true) RETURNING *"#,
            )
            .bind(name)
            .fetch_one(&mut *tx)
            .await?;

            tx.commit().await?;
            return Ok(row);
        }

        let row = sqlx::query_as::<_, Source>(
            r#"INSERT INTO sources (name, "default") VALUES ($1, false) RETURNING *"#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Reassign all physical items from given source IDs to a new source ID
    pub async fn sources_reassign_specimens(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        self.biblios_reassign_items_source(old_source_ids, new_source_id).await
    }

    /// Reassign all biblios from given source IDs to a new source ID (no-op: sources attach to items)
    pub async fn sources_reassign_items(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        self.biblios_reassign_biblios_source(old_source_ids, new_source_id).await
    }

    /// Archive multiple sources by IDs
    pub async fn sources_archive_many(&self, ids: &[i64]) -> AppResult<()> {
        let now = Utc::now();
        sqlx::query("UPDATE sources SET is_archive = 1, archived_at = $1 WHERE id = ANY($2)")
            .bind(now)
            .bind(ids)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Find source by name or create it. Returns source id.
    pub async fn sources_find_or_create_by_name(&self, name: &str) -> AppResult<i64> {
        if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
        {
            return Ok(id);
        }
        let id: i64 = sqlx::query_scalar(r#"INSERT INTO sources (name) VALUES ($1) RETURNING id"#)
            .bind(name)
            .fetch_one(&self.pool)
            .await?;
        Ok(id)
    }

    /// Get the default source (the one with default = true)
    pub async fn sources_get_default(&self) -> AppResult<Option<Source>> {
        let source = sqlx::query_as::<_, Source>(
            r#"SELECT * FROM sources WHERE "default" = true AND (is_archive IS NULL OR is_archive = 0) LIMIT 1"#
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(source)
    }
}

