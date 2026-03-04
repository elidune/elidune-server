//! Sources domain methods on Repository

use chrono::Utc;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::source::Source,
};

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

    /// Update a source (name and/or default status)
    pub async fn sources_update(&self, id: i64, name: Option<&str>, default: Option<bool>) -> AppResult<Source> {
        // If setting this source as default, first unset all other default sources
        if let Some(true) = default {
            sqlx::query(r#"UPDATE sources SET "default" = false WHERE id != $1"#)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }

        // Build update query based on what fields are provided
        let source = match (name, default) {
            (Some(name), Some(default_val)) => {
                sqlx::query_as::<_, Source>(
                    r#"UPDATE sources SET name = $1, "default" = $2 WHERE id = $3 RETURNING *"#
                )
                .bind(name)
                .bind(default_val)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
            }
            (Some(name), None) => {
                sqlx::query_as::<_, Source>(
                    "UPDATE sources SET name = $1 WHERE id = $2 RETURNING *"
                )
                .bind(name)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
            }
            (None, Some(default_val)) => {
                sqlx::query_as::<_, Source>(
                    r#"UPDATE sources SET "default" = $1 WHERE id = $2 RETURNING *"#
                )
                .bind(default_val)
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

    /// Count non-archived specimens linked to a source
    pub async fn sources_count_active_specimens(&self, source_id: i64) -> AppResult<i64> {
        self.items_count_specimens_for_source(source_id).await
    }

    /// Archive a source
    pub async fn sources_archive(&self, id: i64) -> AppResult<Source> {
        let now = Utc::now();
        sqlx::query_as::<_, Source>(
            "UPDATE sources SET is_archive = 1, archive_date = $1 WHERE id = $2 RETURNING *",
        )
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))
    }

    /// Create a new source
    pub async fn sources_create(&self, name: &str, default: Option<bool>) -> AppResult<Source> {
        if default == Some(true) {
            sqlx::query(r#"UPDATE sources SET "default" = false"#)
                .execute(&self.pool)
                .await?;
        }
        let default_val = default.unwrap_or(false);
        let row = sqlx::query_as::<_, Source>(
            r#"INSERT INTO sources (name, "default") VALUES ($1, $2) RETURNING *"#,
        )
        .bind(name)
        .bind(default_val)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Reassign all specimens from given source IDs to a new source ID
    pub async fn sources_reassign_specimens(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        self.items_reassign_specimens_source(old_source_ids, new_source_id).await
    }

    /// Reassign all items from given source IDs to a new source ID
    pub async fn sources_reassign_items(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        self.items_reassign_items_source(old_source_ids, new_source_id).await
    }

    /// Archive multiple sources by IDs
    pub async fn sources_archive_many(&self, ids: &[i64]) -> AppResult<()> {
        let now = Utc::now();
        sqlx::query("UPDATE sources SET is_archive = 1, archive_date = $1 WHERE id = ANY($2)")
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

