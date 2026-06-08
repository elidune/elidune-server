//! Biblios repository — read operations.

use std::collections::HashMap;

use chrono::Utc;
use sqlx::{FromRow, Row};
use sqlx::types::Json;

use super::super::Repository;
use crate::models::item::ItemShort;
use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        author::Author,
        author::Function,
        import_report::DuplicateCandidate,
        biblio::{Collection, Edition, Isbn, Biblio, BiblioQuery, BiblioShort, MeiliBiblioDocument, MediaType, Serie},
        item::Item,
    },
};
use super::BiblioShortRow;

impl Repository {
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_by_id(&self, id: i64) -> AppResult<Biblio> {

        let query = r#"
            SELECT id, media_type, isbn,
                   publication_date, lang, lang_orig, title,
                   subject, dewey, audience_type, page_extent, format,
                   table_of_contents, accompanying_material,
                   abstract as abstract_, notes, keywords,
                   edition_id,
                   is_valid,
                   created_at, updated_at, archived_at
            FROM biblios
            WHERE id = $1
            "#;

        let mut biblio = sqlx::query_as::<_, Biblio>(query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Biblio '{}' not found", id)))?;

        if biblio.archived_at.is_some() {
            return Err(AppError::Gone(format!("Biblio '{}' has been archived", id)));
        }

        let id = biblio.id.ok_or_else(|| AppError::Internal("Biblio id is null".to_string()))?;

        biblio.authors = self.get_biblio_authors(id).await?;
        self.load_biblio_series(id, &mut biblio).await?;
        self.load_biblio_collections(id, &mut biblio).await?;

        biblio.edition = sqlx::query_as::<_, Edition>(
            "SELECT id, publisher_name, place_of_publication, date, created_at, updated_at FROM editions WHERE id = $1",
        )
        .bind(biblio.edition_id)
        .fetch_optional(&self.pool)
        .await?;

        biblio.items = self.biblios_get_items(id).await?;

        Ok(biblio)
    }

    /// Load all authors for a biblio via the biblio_authors junction table
    async fn get_biblio_authors(&self, biblio_id: i64) -> AppResult<Vec<Author>> {
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.lastname, a.firstname, a.bio, a.notes, ba.function
            FROM biblio_authors ba
            JOIN authors a ON a.id = ba.author_id
            WHERE ba.biblio_id = $1
            ORDER BY ba.position
            "#,
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| Author {
                id: r.get("id"),
                key: None,
                lastname: r.get("lastname"),
                firstname: r.get("firstname"),
                bio: r.get::<Option<String>, _>("bio"),
                notes: r.get::<Option<String>, _>("notes"),
                function: r.get::<Option<Function>, _>("function"),
            })
            .collect())
    }
    pub(crate) async fn load_biblio_series(&self, biblio_id: i64, biblio: &mut Biblio) -> AppResult<()> {
        let rows = sqlx::query(
            r#"
            SELECT bsx.series_id, bsx.volume_number,
                   s.id, s.key, s.name, s.issn, s.created_at, s.updated_at
            FROM biblio_series bsx
            INNER JOIN series s ON s.id = bsx.series_id
            WHERE bsx.biblio_id = $1
            ORDER BY bsx.position
            "#,
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;

        biblio.series_ids.clear();
        biblio.series_volume_numbers.clear();
        biblio.series.clear();

        for row in rows {
            let sid: i64 = row.get("series_id");
            let vol: Option<i16> = row.get("volume_number");
            biblio.series_ids.push(sid);
            biblio.series_volume_numbers.push(vol);
            biblio.series.push(Serie {
                id: row.get("id"),
                key: row.get("key"),
                name: row.get("name"),
                issn: row.get("issn"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                volume_number: vol,
            });
        }
        Ok(())
    }
    pub(crate) async fn load_biblio_collections(&self, biblio_id: i64, biblio: &mut Biblio) -> AppResult<()> {
        let rows = sqlx::query(
            r#"
            SELECT bcx.collection_id, bcx.volume_number,
                   c.id, c.key, c.name, c.secondary_title, c.tertiary_title, c.issn,
                   c.created_at, c.updated_at
            FROM biblio_collections bcx
            INNER JOIN collections c ON c.id = bcx.collection_id
            WHERE bcx.biblio_id = $1
            ORDER BY bcx.position
            "#,
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;

        biblio.collection_ids.clear();
        biblio.collection_volume_numbers.clear();
        biblio.collections.clear();

        for row in rows {
            let cid: i64 = row.get("collection_id");
            let vol: Option<i16> = row.get("volume_number");
            biblio.collection_ids.push(cid);
            biblio.collection_volume_numbers.push(vol);
            biblio.collections.push(Collection {
                id: row.get("id"),
                key: row.get("key"),
                name: row.get("name"),
                secondary_title: row.get("secondary_title"),
                tertiary_title: row.get("tertiary_title"),
                issn: row.get("issn"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                volume_number: vol,
            });
        }
        Ok(())
    }
    /// Get a short biblio representation by ID (includes author + items).
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_short_by_id(&self, id: i64) -> AppResult<BiblioShort> {
        let row: BiblioShortRow = sqlx::query_as(
            r#"
            SELECT b.id, b.media_type, b.isbn, b.title,
                   b.publication_date as date, 0::smallint as status,
                   1::smallint as is_local, b.is_valid, b.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ba.function
                       )
                       FROM biblio_authors ba
                       JOIN authors a ON a.id = ba.author_id
                       WHERE ba.biblio_id = b.id
                       ORDER BY ba.position LIMIT 1
                   ) as author
            FROM biblios b
            WHERE b.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Biblio with id {} not found", id)))?;

        let mut short = BiblioShort::from(row);
        let items_map = self.biblios_get_items_short_by_biblio_ids(&[short.id]).await?;
        short.items = items_map.get(&short.id).cloned().unwrap_or_default();
        Ok(short)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_short_by_ids_ordered(&self, ids: &[i64]) -> AppResult<Vec<BiblioShort>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<BiblioShortRow> = sqlx::query_as(
            r#"
            SELECT b.id, b.media_type, b.isbn, b.title,
                   b.publication_date AS date, 0::smallint AS status,
                   1::smallint AS is_local, b.is_valid, b.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ba.function
                       )
                       FROM biblio_authors ba
                       JOIN authors a ON a.id = ba.author_id
                       WHERE ba.biblio_id = b.id
                       ORDER BY ba.position LIMIT 1
                   ) AS author
            FROM biblios b
            WHERE b.id = ANY($1)
            "#,
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        let id_to_index: std::collections::HashMap<i64, usize> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        let biblio_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let items_map = self.biblios_get_items_short_by_biblio_ids(&biblio_ids).await?;

        let mut biblios: Vec<(usize, BiblioShort)> = rows
            .into_iter()
            .map(|r| {
                let pos = id_to_index.get(&r.id).copied().unwrap_or(usize::MAX);
                let mut short = BiblioShort::from(r);
                short.items = items_map.get(&short.id).cloned().unwrap_or_default();
                (pos, short)
            })
            .collect();

        biblios.sort_by_key(|(pos, _)| *pos);
        Ok(biblios.into_iter().map(|(_, biblio)| biblio).collect())
    }

    /// Batch-load [`BiblioShort`] metadata (author, title, …) with **empty** `items`.
    /// Used when items are attached separately (e.g. one copy per hold).
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_short_metadata_map_by_biblio_ids(
        &self,
        biblio_ids: &[i64],
    ) -> AppResult<HashMap<i64, BiblioShort>> {
        if biblio_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<BiblioShortRow> = sqlx::query_as(
            r#"
            SELECT b.id, b.media_type, b.isbn, b.title,
                   b.publication_date AS date, 0::smallint AS status,
                   1::smallint AS is_local, b.is_valid, b.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ba.function
                       )
                       FROM biblio_authors ba
                       JOIN authors a ON a.id = ba.author_id
                       WHERE ba.biblio_id = b.id
                       ORDER BY ba.position LIMIT 1
                   ) AS author
            FROM biblios b
            WHERE b.id = ANY($1)
            "#,
        )
        .bind(biblio_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let mut short = BiblioShort::from(r);
                short.items = Vec::new();
                (short.id, short)
            })
            .collect())
    }
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_marc_record_optional(&self, biblio_id: i64) -> AppResult<Option<crate::marc::MarcRecord>> {
        let json_opt: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT marc_record FROM biblios WHERE id = $1",
        )
        .bind(biblio_id)
        .fetch_one(&self.pool)
        .await?;
        let Some(json) = json_opt else {
            return Ok(None);
        };
        if json.is_null() {
            return Ok(None);
        }
        serde_json::from_value(json).map_err(|e| {
            AppError::Internal(format!("Invalid marc_record JSON for biblio {}: {}", biblio_id, e))
        })
        .map(Some)
    }
}
