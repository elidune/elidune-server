//! Biblios repository — write operations.

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

use super::{dedupe_junction_links, normalize_key};

impl Repository {
    /// Resolve `biblio.series` (nested Serie payloads) into `series_ids` / `series_volume_numbers`,
    /// or keep explicit `series_ids` when `series` is empty.
    async fn resolve_series_ids_from_biblio(&self, biblio: &mut Biblio) -> AppResult<()> {
        if !biblio.series.is_empty() {
            let mut ids = Vec::new();
            let mut vols = Vec::new();
            for s in &biblio.series {
                if let Some(id) = self.process_serie(&Some(s.clone())).await? {
                    ids.push(id);
                    vols.push(s.volume_number);
                }
            }
            biblio.series_ids = ids;
            biblio.series_volume_numbers = vols;
        } else {
            while biblio.series_volume_numbers.len() < biblio.series_ids.len() {
                biblio.series_volume_numbers.push(None);
            }
            biblio.series_volume_numbers.truncate(biblio.series_ids.len());
        }
        Ok(())
    }

    /// Resolve `biblio.collections` (nested Collection payloads) into `collection_ids` / `collection_volume_numbers`.
    async fn resolve_collection_ids_from_biblio(&self, biblio: &mut Biblio) -> AppResult<()> {
        if !biblio.collections.is_empty() {
            let mut ids = Vec::new();
            let mut vols = Vec::new();
            for c in &biblio.collections {
                if let Some(id) = self.process_collection(&Some(c.clone())).await? {
                    ids.push(id);
                    vols.push(c.volume_number);
                }
            }
            biblio.collection_ids = ids;
            biblio.collection_volume_numbers = vols;
        } else {
            while biblio.collection_volume_numbers.len() < biblio.collection_ids.len() {
                biblio.collection_volume_numbers.push(None);
            }
            biblio.collection_volume_numbers.truncate(biblio.collection_ids.len());
        }
        Ok(())
    }
    /// Replace `biblio_collections` rows for this biblio within an open transaction.
    async fn sync_biblio_collections_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        biblio_id: i64,
        collection_ids: &[i64],
        volumes: &[Option<i16>],
    ) -> AppResult<()> {
        sqlx::query("DELETE FROM biblio_collections WHERE biblio_id = $1")
            .bind(biblio_id)
            .execute(&mut **tx)
            .await?;

        let (collection_ids, volumes) = dedupe_junction_links(collection_ids, volumes);
        for (pos, &cid) in collection_ids.iter().enumerate() {
            let vol = volumes.get(pos).copied().flatten();
            sqlx::query(
                r#"
                INSERT INTO biblio_collections (biblio_id, collection_id, position, volume_number)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(biblio_id)
            .bind(cid)
            .bind((pos + 1) as i16)
            .bind(vol)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Replace `biblio_series` rows for this biblio within an open transaction.
    async fn sync_biblio_series_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        biblio_id: i64,
        series_ids: &[i64],
        volumes: &[Option<i16>],
    ) -> AppResult<()> {
        sqlx::query("DELETE FROM biblio_series WHERE biblio_id = $1")
            .bind(biblio_id)
            .execute(&mut **tx)
            .await?;

        let (series_ids, volumes) = dedupe_junction_links(series_ids, volumes);
        for (pos, &sid) in series_ids.iter().enumerate() {
            let vol = volumes.get(pos).copied().flatten();
            sqlx::query(
                r#"
                INSERT INTO biblio_series (biblio_id, series_id, position, volume_number)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(biblio_id)
            .bind(sid)
            .bind((pos + 1) as i16)
            .bind(vol)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
    /// Create a new biblio.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_create<'a>(&self, biblio: &'a mut Biblio) -> AppResult<&'a mut Biblio> {
        let now = Utc::now();

        biblio.updated_at = Some(now);
        biblio.created_at = Some(now);

        self.resolve_series_ids_from_biblio(biblio).await?;
        self.resolve_collection_ids_from_biblio(biblio).await?;
        biblio.edition_id = self.process_edition(&biblio.edition).await?;

        let mut tx = self.pool.begin().await?;

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO biblios (
                media_type, isbn, publication_date,
                lang, lang_orig, title, subject, dewey,
                audience_type, page_extent, format, table_of_contents, accompanying_material,
                abstract, notes, keywords, is_valid,
                edition_id, created_at, updated_at
            ) VALUES (
                $1, $2, $3,
                $4, $5, $6, $7, $8,
                $9, $10, $11, $12, $13,
                $14, $15, $16, $17,
                $18, $19, $20
            ) RETURNING id
            "#,
        )
        .bind(&biblio.media_type)
        .bind(&biblio.isbn.as_ref().map(|i| i.to_string()))
        .bind(&biblio.publication_date)
        .bind(&biblio.lang)
        .bind(&biblio.lang_orig)
        .bind(&biblio.title)
        .bind(&biblio.subject)
        .bind(&biblio.dewey)
        .bind(&biblio.audience_type)
        .bind(&biblio.page_extent)
        .bind(&biblio.format)
        .bind(&biblio.table_of_contents)
        .bind(&biblio.accompanying_material)
        .bind(&biblio.abstract_)
        .bind(&biblio.notes)
        .bind(&biblio.keywords)
        .bind(biblio.is_valid.unwrap_or(true))
        .bind(&biblio.edition_id)
        .bind(&biblio.created_at)
        .bind(&biblio.updated_at)
        .fetch_one(&mut *tx)
        .await?;

        biblio.id = Some(id);

        self.sync_biblio_series_tx(&mut tx, id, &biblio.series_ids, &biblio.series_volume_numbers)
            .await?;
        self.sync_biblio_collections_tx(&mut tx, id, &biblio.collection_ids, &biblio.collection_volume_numbers)
            .await?;
        self.sync_biblio_authors_tx(&mut tx, id, &biblio.authors).await?;

        biblio.marc_record = Some(crate::marc::MarcRecord::from(&*biblio));
        sqlx::query("UPDATE biblios SET marc_record = $1 WHERE id = $2")
            .bind(serde_json::to_value(&biblio.marc_record).unwrap_or_default())
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        self.load_biblio_series(id, biblio).await?;
        self.load_biblio_collections(id, biblio).await?;

        Ok(biblio)
    }

    /// Active biblios with a non-empty ISBN. When `force_rebuild` is false, only rows with `marc_record IS NULL`.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_list_ids_for_z3950_refresh(&self, rebuild_all: bool) -> AppResult<Vec<i64>> {
        let rows: Vec<(i64,)> = if rebuild_all {
            sqlx::query_as(
                r#"
                SELECT id FROM biblios
                WHERE archived_at IS NULL
                  AND isbn IS NOT NULL AND TRIM(isbn) <> ''
                ORDER BY id
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT id FROM biblios
                WHERE isbn IS NOT NULL AND TRIM(isbn) <> ''
                  AND marc_record IS NULL
                ORDER BY id
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
    /// Update an existing biblio.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_update<'a>(&self, id: i64, biblio: &'a mut Biblio) -> AppResult<&'a mut Biblio> {
        biblio.updated_at = Some(Utc::now());
        biblio.id = Some(id);

        self.resolve_series_ids_from_biblio(biblio).await?;
        self.resolve_collection_ids_from_biblio(biblio).await?;
        biblio.edition_id = self.process_edition(&biblio.edition).await?;

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE biblios SET
                media_type = COALESCE($1::text, media_type),
                isbn = COALESCE($2::text, isbn),
                title = COALESCE($3::text, title),
                edition_id = $4,
                updated_at = $5
            WHERE id = $6
            "#,
        )
        .bind(&biblio.media_type)
        .bind(&biblio.isbn.as_ref().map(|i| i.to_string()))
        .bind(&biblio.title)
        .bind(&biblio.edition_id)
        .bind(&biblio.updated_at)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        self.sync_biblio_series_tx(&mut tx, id, &biblio.series_ids, &biblio.series_volume_numbers)
            .await?;
        self.sync_biblio_collections_tx(&mut tx, id, &biblio.collection_ids, &biblio.collection_volume_numbers)
            .await?;

        if !biblio.authors.is_empty() {
            self.sync_biblio_authors_tx(&mut tx, id, &biblio.authors).await?;
        }

        biblio.marc_record = Some(crate::marc::MarcRecord::from(&*biblio));
        sqlx::query("UPDATE biblios SET marc_record = $1 WHERE id = $2")
            .bind(serde_json::to_value(&biblio.marc_record).unwrap_or_default())
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        self.load_biblio_series(id, biblio).await?;
        self.load_biblio_collections(id, biblio).await?;

        Ok(biblio)
    }

    /// Full bibliographic replace (same column set as [`Self::biblios_create`]), preserving the biblio primary key.
    /// Physical items are **not** modified here — pass the desired `items` slice (typically existing copies) on `biblio`.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_full_bibliographic_replace<'a>(
        &self,
        id: i64,
        biblio: &'a mut Biblio,
    ) -> AppResult<&'a mut Biblio> {
        let now = Utc::now();
        biblio.updated_at = Some(now);
        biblio.id = Some(id);

        self.resolve_series_ids_from_biblio(biblio).await?;
        self.resolve_collection_ids_from_biblio(biblio).await?;
        biblio.edition_id = self.process_edition(&biblio.edition).await?;

        let marc_json = serde_json::to_value(&biblio.marc_record).unwrap_or(serde_json::Value::Null);

        let mut tx = self.pool.begin().await?;

        let n = sqlx::query(
            r#"
            UPDATE biblios SET
                media_type = $1,
                isbn = $2,
                publication_date = $3,
                lang = $4,
                lang_orig = $5,
                title = $6,
                subject = $7,
                dewey = $8,
                audience_type = $9,
                page_extent = $10,
                format = $11,
                table_of_contents = $12,
                accompanying_material = $13,
                abstract = $14,
                notes = $15,
                keywords = $16,
                is_valid = $17,
                edition_id = $18,
                updated_at = $19,
                marc_record = $20
            WHERE id = $21 AND archived_at IS NULL
            "#,
        )
        .bind(&biblio.media_type)
        .bind(&biblio.isbn.as_ref().map(|i| i.to_string()))
        .bind(&biblio.publication_date)
        .bind(&biblio.lang)
        .bind(&biblio.lang_orig)
        .bind(&biblio.title)
        .bind(&biblio.subject)
        .bind(&biblio.dewey)
        .bind(&biblio.audience_type)
        .bind(&biblio.page_extent)
        .bind(&biblio.format)
        .bind(&biblio.table_of_contents)
        .bind(&biblio.accompanying_material)
        .bind(&biblio.abstract_)
        .bind(&biblio.notes)
        .bind(&biblio.keywords)
        .bind(biblio.is_valid.unwrap_or(true))
        .bind(&biblio.edition_id)
        .bind(&biblio.updated_at)
        .bind(&marc_json)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if n == 0 {
            tx.rollback().await?;
            return Err(AppError::NotFound(format!("biblio {} not found or archived", id)));
        }

        self.sync_biblio_series_tx(&mut tx, id, &biblio.series_ids, &biblio.series_volume_numbers)
            .await?;
        self.sync_biblio_collections_tx(&mut tx, id, &biblio.collection_ids, &biblio.collection_volume_numbers)
            .await?;
        self.sync_biblio_authors_tx(&mut tx, id, &biblio.authors).await?;

        tx.commit().await?;

        self.load_biblio_series(id, biblio).await?;
        self.load_biblio_collections(id, biblio).await?;

        Ok(biblio)
    }

    /// Update marc record for a biblio.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_update_marc_record(&self, biblio: &mut Biblio) -> AppResult<()> {
        if biblio.marc_record.is_none() {
            biblio.marc_record = sqlx::query_scalar::<_, Option<serde_json::Value>>(
                "SELECT marc_record FROM biblios WHERE id = $1",
            )
            .bind(biblio.id.unwrap_or(0))
            .fetch_optional(&self.pool)
            .await?
            .flatten()
            .and_then(|v| serde_json::from_value::<MarcRecord>(v).ok());
        }

        biblio.marc_record = Some(MarcRecord::from(&*biblio));

        sqlx::query(
            "UPDATE biblios SET marc_record = $1 WHERE id = $2",
        )
        .bind(serde_json::to_value(&biblio.marc_record).unwrap())
        .bind(biblio.id.unwrap_or(0))
        .execute(&self.pool)
        .await?;

        Ok(())
    }
    /// Delete a biblio (soft delete — sets archived_at)
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_delete(&self, id: i64, force: bool) -> AppResult<()> {
        let now = Utc::now();

        let loans = self.loans_get_active_ids_for_biblio(id).await?;

        if loans.len() > 0 {
            if !force {
                return Err(AppError::BusinessRule(
                    "Biblio has borrowed items. Use force=true to delete anyway.".to_string()
                ));
            } else {
                for loan_id in loans {
                    self.loans_return(loan_id).await?;
                }
            }
        }

        // Include item id so archived barcodes stay unique (idx_items_barcode_unique applies to all rows).
        sqlx::query(
            "UPDATE items SET archived_at = $1, updated_at = $1, barcode = CONCAT('ARCH_', id::text, '_', COALESCE(barcode, '')) WHERE biblio_id = $2 AND archived_at IS NULL"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE biblios SET archived_at = $1, updated_at = $1 WHERE id = $2"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Archive a biblio when it has no active physical copies left.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_archive_if_orphan(&self, biblio_id: i64) -> AppResult<bool> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE biblios SET archived_at = $1, updated_at = $1
            WHERE id = $2
              AND archived_at IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM items WHERE biblio_id = $2 AND archived_at IS NULL
              )
            "#,
        )
        .bind(now)
        .bind(biblio_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
    /// Replace all authors for a biblio within an open transaction.
    async fn sync_biblio_authors_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        biblio_id: i64,
        authors: &[Author],
    ) -> AppResult<()> {
        let mut author_ids: Vec<Option<i64>> = Vec::with_capacity(authors.len());
        for author in authors {
            author_ids.push(self.ensure_author(author).await?);
        }

        sqlx::query("DELETE FROM biblio_authors WHERE biblio_id = $1")
            .bind(biblio_id)
            .execute(&mut **tx)
            .await?;

        for (idx, (author, author_id)) in authors.iter().zip(author_ids.iter()).enumerate() {
            let Some(author_id) = author_id else { continue };

            sqlx::query(
                r#"
                INSERT INTO biblio_authors (biblio_id, author_id, function, author_type, position)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (biblio_id, author_id, function) DO UPDATE SET position = $5
                "#,
            )
            .bind(biblio_id)
            .bind(author_id)
            .bind(&author.function)
            .bind(0i16)
            .bind((idx + 1) as i16)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    /// Insert author if new, or return existing id (uses pool, idempotent).
    async fn ensure_author(&self, author: &Author) -> AppResult<Option<i64>> {
        if author.id != 0 {
            return Ok(Some(author.id));
        }

        let Some(ref lastname) = author.lastname else {
            return Ok(None);
        };

        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM authors WHERE lastname = $1 AND firstname IS NOT DISTINCT FROM $2",
        )
        .bind(lastname)
        .bind(&author.firstname)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = existing {
            Ok(Some(id))
        } else {
            let id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO authors (lastname, firstname) VALUES ($1, $2) RETURNING id",
            )
            .bind(lastname)
            .bind(&author.firstname)
            .fetch_one(&self.pool)
            .await?;
            Ok(Some(id))
        }
    }

    // =========================================================================
    // SERIES / COLLECTIONS / EDITIONS
    // =========================================================================

    async fn process_serie(&self, serie: &Option<Serie>) -> AppResult<Option<i64>> {
        let Some(serie) = serie else {
            return Ok(None);
        };

        if let Some(id) = serie.id {
            return Ok(Some(id));
        }

        let Some(ref name) = serie.name else {
            return Ok(None);
        };

        let key = normalize_key(name);

        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM series WHERE key = $1 OR name = $2")
            .bind(&key)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(id) = existing {
            Ok(Some(id))
        } else {
            let id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO series (key, name, issn) VALUES ($1, $2, $3) RETURNING id"
            )
            .bind(&key)
            .bind(name)
            .bind(&serie.issn)
            .fetch_one(&self.pool)
            .await?;
            Ok(Some(id))
        }
    }

    async fn process_collection(&self, collection: &Option<Collection>) -> AppResult<Option<i64>> {
        let Some(collection) = collection else {
            return Ok(None);
        };

        if let Some(id) = collection.id {
            return Ok(Some(id));
        }

        let Some(ref name) = collection.name else {
            return Ok(None);
        };

        let key = normalize_key(name);

        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM collections WHERE key = $1 OR name = $2",
        )
        .bind(&key)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = existing {
            Ok(Some(id))
        } else {
            let id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO collections (key, name, secondary_title, tertiary_title, issn) VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(&key)
            .bind(name)
            .bind(&collection.secondary_title)
            .bind(&collection.tertiary_title)
            .bind(&collection.issn)
            .fetch_one(&self.pool)
            .await?;
            Ok(Some(id))
        }
    }

    async fn process_edition(&self, edition: &Option<Edition>) -> AppResult<Option<i64>> {
        let Some(edition) = edition else {
            return Ok(None);
        };

        if let Some(id) = edition.id {
            if id != 0 {
                return Ok(Some(id));
            }
            return Ok(None);
        }

        let Some(ref publisher_name) = edition.publisher_name else {
            return Ok(None);
        };

        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM editions WHERE publisher_name = $1",
        )
        .bind(publisher_name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = existing {
            Ok(Some(id))
        } else {
            let id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO editions (publisher_name, place_of_publication, date) VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(publisher_name)
            .bind(&edition.place_of_publication)
            .bind(&edition.date)
            .fetch_one(&self.pool)
            .await?;
            Ok(Some(id))
        }
    }
    /// Find an active (non-archived) biblio that has the given ISBN.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_find_active_by_isbn(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<Option<i64>> {
        let row: Option<i64> = if let Some(eid) = exclude_id {
            sqlx::query_scalar(
                "SELECT id FROM biblios WHERE isbn = $1 AND archived_at IS NULL AND id != $2 LIMIT 1",
            )
            .bind(isbn)
            .bind(eid)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT id FROM biblios WHERE isbn = $1 AND archived_at IS NULL LIMIT 1",
            )
            .bind(isbn)
            .fetch_optional(&self.pool)
            .await?
        };
        Ok(row)
    }
    /// Find an existing biblio by ISBN for import deduplication.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_find_by_isbn_for_import(&self, isbn: &str) -> AppResult<Option<DuplicateCandidate>> {
        let row: Option<(i64, Option<chrono::DateTime<Utc>>, i64)> = sqlx::query_as(
            r#"
            SELECT b.id,
                   b.archived_at,
                   (SELECT COUNT(*) FROM items i WHERE i.biblio_id = b.id AND i.archived_at IS NULL) AS item_count
            FROM biblios b
            WHERE b.isbn = $1
            ORDER BY (b.archived_at IS NULL) DESC, b.id DESC
            LIMIT 1
            "#,
        )
        .bind(isbn)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(biblio_id, archived_at, item_count)| DuplicateCandidate {
            biblio_id,
            archived_at,
            item_count,
        }))
    }

    /// Check if ISBN already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM biblios WHERE isbn = $1 AND id != $2)")
                .bind(isbn)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM biblios WHERE isbn = $1)")
                .bind(isbn)
                .fetch_one(&self.pool)
                .await?
        };

        Ok(exists)
    }

    /// Count non-archived items (physical copies) for a source
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_count_items_for_source(&self, source_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM items WHERE source_id = $1 AND archived_at IS NULL",
        )
        .bind(source_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Reassign items (physical copies) from given source IDs to a new source
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_reassign_items_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        let result = sqlx::query("UPDATE items SET source_id = $1 WHERE source_id = ANY($2)")
            .bind(new_source_id)
            .bind(old_source_ids)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Reassign biblios from given source IDs to a new source (no-op: sources are attached to items)
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_reassign_biblios_source(
        &self,
        _old_source_ids: &[i64],
        _new_source_id: i64,
    ) -> AppResult<i64> {
        Ok(0)
    }
}
