//! CRUD operations for catalog reference entities: series and collections.

use async_trait::async_trait;
use chrono::Utc;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::biblio::{
        Collection, CollectionQuery, CreateCollection, CreateSerie, Serie, SerieQuery, UpdateCollection,
        UpdateSerie,
    },
};

#[async_trait]
pub trait CatalogEntitiesRepository: Send + Sync {
    // ── Series ────────────────────────────────────────────────────────────────
    async fn series_list(&self, query: &SerieQuery) -> AppResult<(Vec<Serie>, i64)>;
    async fn series_get(&self, id: i64) -> AppResult<Serie>;
    async fn series_create(&self, data: &CreateSerie) -> AppResult<Serie>;
    async fn series_update(&self, id: i64, data: &UpdateSerie) -> AppResult<Serie>;
    async fn series_delete(&self, id: i64) -> AppResult<()>;

    // ── Collections ───────────────────────────────────────────────────────────
    async fn collections_list(&self, query: &CollectionQuery) -> AppResult<(Vec<Collection>, i64)>;
    async fn collections_get(&self, id: i64) -> AppResult<Collection>;
    async fn collections_create(&self, data: &CreateCollection) -> AppResult<Collection>;
    async fn collections_update(&self, id: i64, data: &UpdateCollection) -> AppResult<Collection>;
    async fn collections_delete(&self, id: i64) -> AppResult<()>;
}

#[async_trait]
impl CatalogEntitiesRepository for Repository {
    async fn series_list(&self, query: &SerieQuery) -> AppResult<(Vec<Serie>, i64)> {
        Repository::series_list(self, query).await
    }
    async fn series_get(&self, id: i64) -> AppResult<Serie> {
        Repository::series_get(self, id).await
    }
    async fn series_create(&self, data: &CreateSerie) -> AppResult<Serie> {
        Repository::series_create(self, data).await
    }
    async fn series_update(&self, id: i64, data: &UpdateSerie) -> AppResult<Serie> {
        Repository::series_update(self, id, data).await
    }
    async fn series_delete(&self, id: i64) -> AppResult<()> {
        Repository::series_delete(self, id).await
    }
    async fn collections_list(&self, query: &CollectionQuery) -> AppResult<(Vec<Collection>, i64)> {
        Repository::collections_list(self, query).await
    }
    async fn collections_get(&self, id: i64) -> AppResult<Collection> {
        Repository::collections_get(self, id).await
    }
    async fn collections_create(&self, data: &CreateCollection) -> AppResult<Collection> {
        Repository::collections_create(self, data).await
    }
    async fn collections_update(&self, id: i64, data: &UpdateCollection) -> AppResult<Collection> {
        Repository::collections_update(self, id, data).await
    }
    async fn collections_delete(&self, id: i64) -> AppResult<()> {
        Repository::collections_delete(self, id).await
    }
}

impl Repository {
    fn normalize_key(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .map(|c| match c {
                'à' | 'á' | 'â' | 'ã' | 'ä' => 'a',
                'è' | 'é' | 'ê' | 'ë' => 'e',
                'ì' | 'í' | 'î' | 'ï' => 'i',
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
                'ù' | 'ú' | 'û' | 'ü' => 'u',
                'ç' => 'c',
                'ñ' => 'n',
                c if c.is_alphanumeric() => c,
                _ => '_',
            })
            .collect::<String>()
            .replace("__", "_")
            .trim_matches('_')
            .to_string()
    }

    // =========================================================================
    // SERIES
    // =========================================================================

    pub async fn series_list(&self, query: &SerieQuery) -> AppResult<(Vec<Serie>, i64)> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let (rows, total) = if let Some(ref name) = query.name {
            let pattern = format!("%{}%", name.replace('%', "\\%").replace('_', "\\_"));
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM series WHERE unaccent(lower(name)) LIKE unaccent(lower($1))",
            )
            .bind(&pattern)
            .fetch_one(&self.pool)
            .await?;

            let rows: Vec<Serie> = sqlx::query_as(
                r#"SELECT id, key, name, issn, created_at, updated_at
                   FROM series
                   WHERE unaccent(lower(name)) LIKE unaccent(lower($1))
                   ORDER BY name ASC
                   LIMIT $2 OFFSET $3"#,
            )
            .bind(&pattern)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

            (rows, total)
        } else {
            let total: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM series").fetch_one(&self.pool).await?;

            let rows: Vec<Serie> = sqlx::query_as(
                "SELECT id, key, name, issn, created_at, updated_at FROM series ORDER BY name ASC LIMIT $1 OFFSET $2",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

            (rows, total)
        };

        Ok((rows, total))
    }

    pub async fn series_get(&self, id: i64) -> AppResult<Serie> {
        sqlx::query_as(
            "SELECT id, key, name, issn, created_at, updated_at FROM series WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Series {id} not found")))
    }

    pub async fn series_create(&self, data: &CreateSerie) -> AppResult<Serie> {
        let now = Utc::now();
        let key = data.key.as_deref().map(Self::normalize_key)
            .unwrap_or_else(|| Self::normalize_key(&data.name));

        let id: i64 = sqlx::query_scalar(
            r#"INSERT INTO series (key, name, issn, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $4) RETURNING id"#,
        )
        .bind(&key)
        .bind(&data.name)
        .bind(&data.issn)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                AppError::Conflict(format!("A series with key '{}' already exists", key))
            } else {
                AppError::Internal(e.to_string())
            }
        })?;

        self.series_get(id).await
    }

    pub async fn series_update(&self, id: i64, data: &UpdateSerie) -> AppResult<Serie> {
        let now = Utc::now();

        let updated = sqlx::query_scalar::<_, bool>(
            r#"UPDATE series SET
                   name       = COALESCE($1, name),
                   key        = COALESCE($2, key),
                   issn       = COALESCE($3, issn),
                   updated_at = $4
               WHERE id = $5
               RETURNING true"#,
        )
        .bind(&data.name)
        .bind(&data.key)
        .bind(&data.issn)
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if updated.is_none() {
            return Err(AppError::NotFound(format!("Series {id} not found")));
        }

        self.series_get(id).await
    }

    pub async fn series_delete(&self, id: i64) -> AppResult<()> {
        let used: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM biblio_series WHERE series_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;

        if used > 0 {
            return Err(AppError::Conflict(format!(
                "Series {id} is still linked to {used} biblio(s) and cannot be deleted"
            )));
        }

        let deleted = sqlx::query_scalar::<_, bool>(
            "DELETE FROM series WHERE id = $1 RETURNING true",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if deleted.is_none() {
            return Err(AppError::NotFound(format!("Series {id} not found")));
        }

        Ok(())
    }

    // =========================================================================
    // COLLECTIONS
    // =========================================================================

    pub async fn collections_list(&self, query: &CollectionQuery) -> AppResult<(Vec<Collection>, i64)> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(50).min(200);
        let offset = (page - 1) * per_page;

        let (rows, total) = if let Some(ref name) = query.name {
            let pattern = format!("%{}%", name.replace('%', "\\%").replace('_', "\\_"));
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM collections WHERE unaccent(lower(name)) LIKE unaccent(lower($1))",
            )
            .bind(&pattern)
            .fetch_one(&self.pool)
            .await?;

            let rows: Vec<Collection> = sqlx::query_as(
                r#"SELECT id, key, name, secondary_title, tertiary_title, issn, created_at, updated_at
                   FROM collections
                   WHERE unaccent(lower(name)) LIKE unaccent(lower($1))
                   ORDER BY name ASC
                   LIMIT $2 OFFSET $3"#,
            )
            .bind(&pattern)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

            (rows, total)
        } else {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM collections")
                .fetch_one(&self.pool)
                .await?;

            let rows: Vec<Collection> = sqlx::query_as(
                r#"SELECT id, key, name, secondary_title, tertiary_title, issn, created_at, updated_at
                   FROM collections ORDER BY name ASC LIMIT $1 OFFSET $2"#,
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

            (rows, total)
        };

        Ok((rows, total))
    }

    pub async fn collections_get(&self, id: i64) -> AppResult<Collection> {
        sqlx::query_as(
            "SELECT id, key, name, secondary_title, tertiary_title, issn, created_at, updated_at FROM collections WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Collection {id} not found")))
    }

    pub async fn collections_create(&self, data: &CreateCollection) -> AppResult<Collection> {
        let now = Utc::now();
        let key = data.key.as_deref().map(Self::normalize_key)
            .unwrap_or_else(|| Self::normalize_key(&data.name));

        let id: i64 = sqlx::query_scalar(
            r#"INSERT INTO collections (key, name, secondary_title, tertiary_title, issn, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $6) RETURNING id"#,
        )
        .bind(&key)
        .bind(&data.name)
        .bind(&data.secondary_title)
        .bind(&data.tertiary_title)
        .bind(&data.issn)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                AppError::Conflict(format!("A collection with key '{}' already exists", key))
            } else {
                AppError::Internal(e.to_string())
            }
        })?;

        self.collections_get(id).await
    }

    pub async fn collections_update(&self, id: i64, data: &UpdateCollection) -> AppResult<Collection> {
        let now = Utc::now();

        let updated = sqlx::query_scalar::<_, bool>(
            r#"UPDATE collections SET
                   name            = COALESCE($1, name),
                   key             = COALESCE($2, key),
                   secondary_title = COALESCE($3, secondary_title),
                   tertiary_title  = COALESCE($4, tertiary_title),
                   issn            = COALESCE($5, issn),
                   updated_at      = $6
               WHERE id = $7
               RETURNING true"#,
        )
        .bind(&data.name)
        .bind(&data.key)
        .bind(&data.secondary_title)
        .bind(&data.tertiary_title)
        .bind(&data.issn)
        .bind(now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if updated.is_none() {
            return Err(AppError::NotFound(format!("Collection {id} not found")));
        }

        self.collections_get(id).await
    }

    pub async fn collections_delete(&self, id: i64) -> AppResult<()> {
        let used: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM biblio_collections WHERE collection_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;

        if used > 0 {
            return Err(AppError::Conflict(format!(
                "Collection {id} is still linked to {used} biblio(s) and cannot be deleted"
            )));
        }

        let deleted = sqlx::query_scalar::<_, bool>(
            "DELETE FROM collections WHERE id = $1 RETURNING true",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if deleted.is_none() {
            return Err(AppError::NotFound(format!("Collection {id} not found")));
        }

        Ok(())
    }

}
