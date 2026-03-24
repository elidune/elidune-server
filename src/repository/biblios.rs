//! Biblios domain methods on Repository.
//!
//! Uses marc-rs types (Leader, MarcFormat, etc.) where applicable; DB serialization
//! uses the associated char or int (e.g. media_type string from Leader record_type).

use std::collections::HashMap;
use chrono::Utc;
use sqlx::{FromRow, Row};
use sqlx::types::Json;

use super::Repository;
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
use async_trait::async_trait;

/// Contract for [`Repository`] biblio/item persistence. Implemented below; services may use
/// `Arc<dyn BibliosRepository>` for substitution in tests.
#[async_trait]
pub trait BibliosRepository: Send + Sync {
    async fn biblios_get_by_id_or_isbn(&self, id_or_isbn: &str) -> AppResult<Biblio>;
    async fn biblios_get_short_by_id(&self, id: i64) -> AppResult<BiblioShort>;
    async fn biblios_search(&self, query: &BiblioQuery) -> AppResult<(Vec<BiblioShort>, i64)>;
    async fn biblios_get_by_series(&self, series_id: i64) -> AppResult<Vec<BiblioShort>>;
    async fn biblios_get_by_collection(&self, collection_id: i64) -> AppResult<Vec<BiblioShort>>;
    async fn biblios_get_meili_document(&self, id: i64) -> AppResult<Option<MeiliBiblioDocument>>;
    async fn biblios_get_all_meili_documents(&self) -> AppResult<Vec<MeiliBiblioDocument>>;
    async fn biblios_get_short_by_ids_ordered(&self, ids: &[i64]) -> AppResult<Vec<BiblioShort>>;
    async fn biblios_create<'a>(&self, biblio: &'a mut Biblio) -> AppResult<&'a mut Biblio>;
    async fn biblios_update<'a>(&self, id: i64, biblio: &'a mut Biblio) -> AppResult<&'a mut Biblio>;
    async fn biblios_delete(&self, id: i64, force: bool) -> AppResult<()>;
    async fn biblios_get_items(&self, biblio_id: i64) -> AppResult<Vec<Item>>;
    async fn biblios_get_items_short_by_biblio_ids(
        &self,
        biblio_ids: &[i64],
    ) -> AppResult<HashMap<i64, Vec<ItemShort>>>;
    async fn biblios_create_item(&self, biblio_id: i64, item: &Item) -> AppResult<Item>;
    async fn upsert_item<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item>;
    async fn items_update<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item>;
    async fn items_delete(&self, id: i64, force: bool) -> AppResult<()>;
    async fn items_barcode_exists(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<bool>;
    async fn items_get_by_barcode(&self, barcode: &str) -> AppResult<Option<(i64, bool)>>;
    async fn items_reactivate(
        &self,
        item_id: i64,
        biblio_id: i64,
        item: &Item,
    ) -> AppResult<Item>;
    async fn biblios_find_active_by_isbn(
        &self,
        isbn: &str,
        exclude_id: Option<i64>,
    ) -> AppResult<Option<i64>>;
    async fn items_find_short_by_barcode(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<Option<ItemShort>>;
    async fn biblios_find_by_isbn_for_import(&self, isbn: &str) -> AppResult<Option<DuplicateCandidate>>;
    async fn biblios_update_marc_record(&self, biblio: &mut Biblio) -> AppResult<()>;
    async fn biblios_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<bool>;
    async fn biblios_count_items_for_source(&self, source_id: i64) -> AppResult<i64>;
    async fn biblios_reassign_items_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64>;
    async fn biblios_reassign_biblios_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64>;
}

#[async_trait::async_trait]
impl BibliosRepository for Repository {
    async fn biblios_get_by_id_or_isbn(&self, id_or_isbn: &str) -> crate::error::AppResult<crate::models::biblio::Biblio> {
        Repository::biblios_get_by_id_or_isbn(self, id_or_isbn).await
    }
    async fn biblios_get_short_by_id(&self, id: i64) -> crate::error::AppResult<crate::models::biblio::BiblioShort> {
        Repository::biblios_get_short_by_id(self, id).await
    }
    async fn biblios_search(&self, query: &crate::models::biblio::BiblioQuery) -> crate::error::AppResult<(Vec<crate::models::biblio::BiblioShort>, i64)> {
        Repository::biblios_search(self, query).await
    }
    async fn biblios_get_by_series(&self, series_id: i64) -> crate::error::AppResult<Vec<crate::models::biblio::BiblioShort>> {
        Repository::biblios_get_by_series(self, series_id).await
    }
    async fn biblios_get_by_collection(&self, collection_id: i64) -> crate::error::AppResult<Vec<crate::models::biblio::BiblioShort>> {
        Repository::biblios_get_by_collection(self, collection_id).await
    }
    async fn biblios_get_meili_document(&self, id: i64) -> crate::error::AppResult<Option<crate::models::biblio::MeiliBiblioDocument>> {
        Repository::biblios_get_meili_document(self, id).await
    }
    async fn biblios_get_all_meili_documents(&self) -> crate::error::AppResult<Vec<crate::models::biblio::MeiliBiblioDocument>> {
        Repository::biblios_get_all_meili_documents(self).await
    }
    async fn biblios_get_short_by_ids_ordered(&self, ids: &[i64]) -> crate::error::AppResult<Vec<crate::models::biblio::BiblioShort>> {
        Repository::biblios_get_short_by_ids_ordered(self, ids).await
    }
    async fn biblios_create<'a>(&self, biblio: &'a mut crate::models::biblio::Biblio) -> crate::error::AppResult<&'a mut crate::models::biblio::Biblio> {
        Repository::biblios_create(self, biblio).await
    }
    async fn biblios_update<'a>(&self, id: i64, biblio: &'a mut crate::models::biblio::Biblio) -> crate::error::AppResult<&'a mut crate::models::biblio::Biblio> {
        Repository::biblios_update(self, id, biblio).await
    }
    async fn biblios_delete(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::biblios_delete(self, id, force).await
    }
    async fn biblios_get_items(&self, biblio_id: i64) -> crate::error::AppResult<Vec<crate::models::item::Item>> {
        Repository::biblios_get_items(self, biblio_id).await
    }
    async fn biblios_get_items_short_by_biblio_ids(&self, biblio_ids: &[i64]) -> crate::error::AppResult<std::collections::HashMap<i64, Vec<crate::models::item::ItemShort>>> {
        Repository::biblios_get_items_short_by_biblio_ids(self, biblio_ids).await
    }
    async fn biblios_create_item(&self, biblio_id: i64, item: &crate::models::item::Item) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::biblios_create_item(self, biblio_id, item).await
    }
    async fn upsert_item<'a>(&self, item: &'a mut crate::models::item::Item) -> crate::error::AppResult<&'a mut crate::models::item::Item> {
        Repository::upsert_item(self, item).await
    }
    async fn items_update<'a>(&self, item: &'a mut crate::models::item::Item) -> crate::error::AppResult<&'a mut crate::models::item::Item> {
        Repository::items_update(self, item).await
    }
    async fn items_delete(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::items_delete(self, id, force).await
    }
    async fn items_barcode_exists(&self, barcode: &str, exclude_item_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::items_barcode_exists(self, barcode, exclude_item_id).await
    }
    async fn items_get_by_barcode(&self, barcode: &str) -> crate::error::AppResult<Option<(i64, bool)>> {
        Repository::items_get_by_barcode(self, barcode).await
    }
    async fn items_reactivate(&self, item_id: i64, biblio_id: i64, item: &crate::models::item::Item) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::items_reactivate(self, item_id, biblio_id, item).await
    }
    async fn biblios_find_active_by_isbn(&self, isbn: &str, exclude_id: Option<i64>) -> crate::error::AppResult<Option<i64>> {
        Repository::biblios_find_active_by_isbn(self, isbn, exclude_id).await
    }
    async fn items_find_short_by_barcode(&self, barcode: &str, exclude_item_id: Option<i64>) -> crate::error::AppResult<Option<crate::models::item::ItemShort>> {
        Repository::items_find_short_by_barcode(self, barcode, exclude_item_id).await
    }
    async fn biblios_find_by_isbn_for_import(&self, isbn: &str) -> crate::error::AppResult<Option<crate::models::import_report::DuplicateCandidate>> {
        Repository::biblios_find_by_isbn_for_import(self, isbn).await
    }
    async fn biblios_update_marc_record(&self, biblio: &mut crate::models::biblio::Biblio) -> crate::error::AppResult<()> {
        Repository::biblios_update_marc_record(self, biblio).await
    }
    async fn biblios_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::biblios_isbn_exists(self, isbn, exclude_id).await
    }
    async fn biblios_count_items_for_source(&self, source_id: i64) -> crate::error::AppResult<i64> {
        Repository::biblios_count_items_for_source(self, source_id).await
    }
    async fn biblios_reassign_items_source(&self, old_source_ids: &[i64], new_source_id: i64) -> crate::error::AppResult<i64> {
        Repository::biblios_reassign_items_source(self, old_source_ids, new_source_id).await
    }
    async fn biblios_reassign_biblios_source(&self, old_source_ids: &[i64], new_source_id: i64) -> crate::error::AppResult<i64> {
        Repository::biblios_reassign_biblios_source(self, old_source_ids, new_source_id).await
    }
}


/// Internal row type for decoding BiblioShort with JSONB author (items loaded separately).
#[derive(FromRow)]
struct BiblioShortRow {
    id: i64,
    media_type: MediaType,
    isbn: Option<Isbn>,
    title: Option<String>,
    date: Option<String>,
    status: i16,
    #[allow(dead_code)]
    is_local: i16,
    is_valid: Option<i16>,
    archived_at: Option<chrono::DateTime<Utc>>,
    author: Option<Json<Author>>,
}

/// Row type for item (physical copy) short data from SQL (build ItemShort in Rust).
#[derive(FromRow)]
struct ItemShortRow {
    biblio_id: i64,
    id: i64,
    barcode: Option<String>,
    call_number: Option<String>,
    borrowable: bool,
    source_name: Option<String>,
}

impl From<ItemShortRow> for ItemShort {
    fn from(r: ItemShortRow) -> Self {
        Self {
            id: r.id,
            barcode: r.barcode,
            call_number: r.call_number,
            borrowable: r.borrowable,
            source_name: r.source_name,
        }
    }
}

impl From<BiblioShortRow> for BiblioShort {
    fn from(r: BiblioShortRow) -> Self {
        Self {
            id: r.id,
            media_type: r.media_type,
            isbn: r.isbn,
            title: r.title,
            date: r.date,
            status: r.status,
            is_valid: r.is_valid,
            archived_at: r.archived_at,
            author: r.author.map(|j| j.0),
            items: Vec::new(),
        }
    }
}


/// Escape a string for use as a LIKE pattern (ESCAPE '\').
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}


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

impl Repository {
    // =========================================================================
    // READ (biblios)
    // =========================================================================

    /// Get biblio by numeric ID or by ISBN.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_by_id_or_isbn(&self, id_or_isbn: &str) -> AppResult<Biblio> {

        let query = r#"
            SELECT id, media_type, isbn,
                   publication_date, lang, lang_orig, title,
                   subject, audience_type, page_extent, format,
                   table_of_contents, accompanying_material,
                   abstract as abstract_, notes, keywords,
                   edition_id,
                   is_valid,
                   created_at, updated_at, archived_at
            FROM biblios
            WHERE (id = $1 OR isbn = $2)
            "#;

        let numeric_id = id_or_isbn.parse::<i64>().unwrap_or(0);
        let isbn_str = Isbn::new(id_or_isbn).to_string();

        let mut biblio = sqlx::query_as::<_, Biblio>(query)
            .bind(numeric_id)
            .bind(&isbn_str)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Biblio '{}' not found", id_or_isbn)))?;

        if biblio.archived_at.is_some() {
            return Err(AppError::Gone(format!("Biblio '{}' has been archived", id_or_isbn)));
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

    /// Load N:M series links into `biblio` (series_ids, series_volume_numbers, series).
    async fn load_biblio_series(&self, biblio_id: i64, biblio: &mut Biblio) -> AppResult<()> {
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

    /// Load N:M collection links into `biblio` (collection_ids, collection_volume_numbers, collections).
    async fn load_biblio_collections(&self, biblio_id: i64, biblio: &mut Biblio) -> AppResult<()> {
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

    // =========================================================================
    // SEARCH
    // =========================================================================

    /// Search biblios with parameterized field filters. All non-freesearch BiblioQuery fields
    /// are handled here. When freesearch is present, the CatalogService routes through
    /// Meilisearch instead; this path handles field-only filters and Meilisearch fallback.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_search(&self, query: &BiblioQuery) -> AppResult<(Vec<BiblioShort>, i64)> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(20).clamp(1, 200);
        let offset = (page - 1) * per_page;

        #[derive(Debug)]
        enum Param {
            Text(String),
            I16(i16),
            I64(i64),
        }

        let mut where_parts: Vec<String> = Vec::new();
        let mut params: Vec<Param> = Vec::new();

        if query.archive.unwrap_or(false) {
            where_parts.push("b.archived_at IS NOT NULL".to_string());
        } else {
            where_parts.push("b.archived_at IS NULL".to_string());
        }

        if let Some(ref mt) = query.media_type {
            params.push(Param::Text(mt.clone()));
            where_parts.push(format!("b.media_type = ${}", params.len()));
        }

        if let Some(ref isbn) = query.isbn {
            params.push(Param::Text(isbn.to_string()));
            where_parts.push(format!("b.isbn = ${}", params.len()));
        }

        // barcode → item lookup
        if let Some(ref barcode) = query.barcode {
            params.push(Param::Text(barcode.clone()));
            where_parts.push(format!(
                "EXISTS (SELECT 1 FROM items i WHERE i.biblio_id = b.id AND i.barcode = ${})",
                params.len()
            ));
        }

        if let Some(ref at) = query.audience_type {
            params.push(Param::Text(at.clone()));
            where_parts.push(format!("b.audience_type = ${}", params.len()));
        }

        if let Some(ref lang) = query.lang {
            params.push(Param::Text(lang.clone()));
            where_parts.push(format!("b.lang = ${}", params.len()));
        }

        if let Some(ref title) = query.title {
            params.push(Param::Text(format!("%{}%", like_escape(title))));
            let idx = params.len();
            where_parts.push(format!(
                "unaccent(lower(b.title)) LIKE unaccent(lower(${idx}))"
            ));
        }

        if let Some(ref subject) = query.subject {
            params.push(Param::Text(format!("%{}%", like_escape(subject))));
            let idx = params.len();
            where_parts.push(format!(
                "unaccent(lower(b.subject)) LIKE unaccent(lower(${idx}))"
            ));
        }

        if let Some(ref kw) = query.keywords {
            params.push(Param::Text(format!("%{}%", like_escape(kw))));
            let idx = params.len();
            where_parts.push(format!(
                "EXISTS (SELECT 1 FROM unnest(b.keywords) AS kw \
                 WHERE unaccent(lower(kw)) LIKE unaccent(lower(${idx})))"
            ));
        }

        if let Some(ref content) = query.content {
            params.push(Param::Text(format!("%{}%", like_escape(content))));
            let idx = params.len();
            where_parts.push(format!(
                "(unaccent(lower(b.table_of_contents)) LIKE unaccent(lower(${idx})) \
                 OR unaccent(lower(b.abstract)) LIKE unaccent(lower(${idx})))"
            ));
        }

        if let Some(ref author) = query.author {
            params.push(Param::Text(format!("%{}%", like_escape(author))));
            let idx = params.len();
            where_parts.push(format!(
                "EXISTS (\
                    SELECT 1 FROM biblio_authors ba \
                    JOIN authors a ON a.id = ba.author_id \
                    WHERE ba.biblio_id = b.id \
                    AND (unaccent(lower(a.lastname)) LIKE unaccent(lower(${idx})) \
                         OR unaccent(lower(a.firstname)) LIKE unaccent(lower(${idx})))\
                )"
            ));
        }

        if let Some(ref editor) = query.editor {
            params.push(Param::Text(format!("%{}%", like_escape(editor))));
            let idx = params.len();
            where_parts.push(format!(
                "EXISTS (\
                    SELECT 1 FROM editions e \
                    WHERE e.id = b.edition_id \
                    AND unaccent(lower(e.publisher_name)) LIKE unaccent(lower(${idx}))\
                )"
            ));
        }

        if query.serie.is_some() || query.serie_id.is_some() {
            let mut conds: Vec<String> = Vec::new();
            if let Some(ref serie) = query.serie {
                params.push(Param::Text(format!("%{}%", like_escape(serie))));
                let idx = params.len();
                conds.push(format!("unaccent(lower(s.name)) LIKE unaccent(lower(${idx}))"));
            }
            if let Some(serie_id) = query.serie_id {
                params.push(Param::I64(serie_id));
                let idx = params.len();
                conds.push(format!("s.id = ${idx}"));
            }
            where_parts.push(format!(
                "EXISTS (\
                    SELECT 1 FROM biblio_series bsx \
                    JOIN series s ON s.id = bsx.series_id \
                    WHERE bsx.biblio_id = b.id \
                    AND ({})\
                )",
                conds.join(" OR ")
            ));
        }

        if query.collection.is_some() || query.collection_id.is_some() {
            let mut conds: Vec<String> = Vec::new();
            if let Some(ref collection) = query.collection {
                params.push(Param::Text(format!("%{}%", like_escape(collection))));
                let idx = params.len();
                conds.push(format!("unaccent(lower(c.name)) LIKE unaccent(lower(${idx}))"));
            }
            if let Some(collection_id) = query.collection_id {
                params.push(Param::I64(collection_id));
                let idx = params.len();
                conds.push(format!("c.id = ${idx}"));
            }
            where_parts.push(format!(
                "EXISTS (\
                    SELECT 1 FROM biblio_collections bcx \
                    JOIN collections c ON c.id = bcx.collection_id \
                    WHERE bcx.biblio_id = b.id \
                    AND ({})\
                )",
                conds.join(" OR ")
            ));
        }

        if let Some(ref fs) = query.freesearch {
            let fs = fs.trim();
            if !fs.is_empty() {
                params.push(Param::Text(format!("%{}%", like_escape(fs))));
                let idx = params.len();
                where_parts.push(format!(
                    "(unaccent(lower(b.title)) LIKE unaccent(lower(${idx})) \
                     OR unaccent(lower(b.subject)) LIKE unaccent(lower(${idx})) \
                     OR unaccent(lower(b.notes)) LIKE unaccent(lower(${idx})))"
                ));
            }
        }

        let where_sql = if where_parts.is_empty() {
            "1=1".to_string()
        } else {
            where_parts.join(" AND ")
        };

        let order_sql = "b.title ASC NULLS LAST".to_string();

        let sql = format!(
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
                   ) AS author,
                   COUNT(*) OVER() AS total_count
            FROM biblios b
            WHERE {where}
            ORDER BY {order}
            LIMIT {limit} OFFSET {offset}
            "#,
            where = where_sql,
            order = order_sql,
            limit = per_page,
            offset = offset,
        );

        use sqlx::Arguments;
        let mut pg_args = sqlx::postgres::PgArguments::default();
        for p in &params {
            match p {
                Param::Text(s) => pg_args.add(s.clone()),
                Param::I16(v) => pg_args.add(*v),
                Param::I64(v) => pg_args.add(*v),
            }
        }

        #[derive(FromRow)]
        struct BiblioShortWithCount {
            id: i64,
            media_type: MediaType,
            isbn: Option<Isbn>,
            title: Option<String>,
            date: Option<String>,
            status: i16,
            #[allow(dead_code)]
            is_local: i16,
            is_valid: Option<i16>,
            archived_at: Option<chrono::DateTime<Utc>>,
            author: Option<sqlx::types::Json<Author>>,
            total_count: i64,
        }

        let rows: Vec<BiblioShortWithCount> = sqlx::query_as_with(&sql, pg_args)
            .fetch_all(&self.pool)
            .await?;

        let total = rows.first().map(|r| r.total_count).unwrap_or(0);
        let biblio_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let items_map = self.biblios_get_items_short_by_biblio_ids(&biblio_ids).await?;

        let biblios: Vec<BiblioShort> = rows
            .into_iter()
            .map(|r| {
                let mut short = BiblioShort {
                    id: r.id,
                    media_type: r.media_type,
                    isbn: r.isbn,
                    title: r.title,
                    date: r.date,
                    status: r.status,
                    is_valid: r.is_valid,
                    archived_at: r.archived_at,
                    author: r.author.map(|j| j.0),
                    items: Vec::new(),
                };
                short.items = items_map.get(&short.id).cloned().unwrap_or_default();
                short
            })
            .collect();

        Ok((biblios, total))
    }

    /// List all biblios belonging to a series
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_by_series(&self, series_id: i64) -> AppResult<Vec<BiblioShort>> {
        let rows: Vec<BiblioShortRow> = sqlx::query_as(
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
            INNER JOIN biblio_series bsx ON bsx.biblio_id = b.id AND bsx.series_id = $1
            WHERE b.archived_at IS NULL
            ORDER BY bsx.volume_number NULLS LAST, b.title
            "#,
        )
        .bind(series_id)
        .fetch_all(&self.pool)
        .await?;

        let biblio_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let items_map = self.biblios_get_items_short_by_biblio_ids(&biblio_ids).await?;
        let biblios: Vec<BiblioShort> = rows
            .into_iter()
            .map(|r| {
                let mut short = BiblioShort::from(r);
                short.items = items_map.get(&short.id).cloned().unwrap_or_default();
                short
            })
            .collect();

        Ok(biblios)
    }

    /// List all biblios belonging to a collection (ordered by volume number)
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_by_collection(&self, collection_id: i64) -> AppResult<Vec<BiblioShort>> {
        let rows: Vec<BiblioShortRow> = sqlx::query_as(
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
            INNER JOIN biblio_collections bcx ON bcx.biblio_id = b.id AND bcx.collection_id = $1
            WHERE b.archived_at IS NULL
            ORDER BY bcx.volume_number NULLS LAST, b.title
            "#,
        )
        .bind(collection_id)
        .fetch_all(&self.pool)
        .await?;

        let biblio_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let items_map = self.biblios_get_items_short_by_biblio_ids(&biblio_ids).await?;
        let biblios: Vec<BiblioShort> = rows
            .into_iter()
            .map(|r| {
                let mut short = BiblioShort::from(r);
                short.items = items_map.get(&short.id).cloned().unwrap_or_default();
                short
            })
            .collect();

        Ok(biblios)
    }

    // =========================================================================
    // MEILISEARCH DOCUMENT BUILDERS
    // =========================================================================

    /// Build a single Meilisearch document for the given biblio ID using a single JOIN query.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_meili_document(&self, id: i64) -> AppResult<Option<MeiliBiblioDocument>> {
        let doc = sqlx::query_as::<_, MeiliBiblioDocument>(
            r#"
            SELECT
                b.id,
                b.media_type,
                b.isbn::text AS isbn,
                b.title,
                COALESCE(
                    string_agg(DISTINCT concat_ws(' ', a.lastname, a.firstname), ', ')
                    FILTER (WHERE a.id IS NOT NULL),
                    ''
                ) AS author_names,
                b.subject,
                COALESCE(b.keywords, '{}') AS keywords,
                ed.publisher_name,
                COALESCE(
                    string_agg(DISTINCT se.name, ', ') FILTER (WHERE se.name IS NOT NULL),
                    ''
                ) AS series_name,
                COALESCE(
                    string_agg(DISTINCT co.name, ', ') FILTER (WHERE co.name IS NOT NULL),
                    ''
                ) AS collection_name,
                COALESCE(
                    array_agg(DISTINCT it.barcode) FILTER (WHERE it.barcode IS NOT NULL),
                    '{}'
                ) AS barcodes,
                COALESCE(
                    array_agg(DISTINCT it.call_number) FILTER (WHERE it.call_number IS NOT NULL),
                    '{}'
                ) AS call_numbers,
                b.abstract AS abstract_text,
                b.notes,
                b.table_of_contents,
                b.lang,
                b.audience_type,
                (b.archived_at IS NOT NULL) AS is_archived
            FROM biblios b
            LEFT JOIN biblio_authors ba ON ba.biblio_id = b.id
            LEFT JOIN authors a ON a.id = ba.author_id
            LEFT JOIN editions ed ON ed.id = b.edition_id
            LEFT JOIN biblio_series bsx ON bsx.biblio_id = b.id
            LEFT JOIN series se ON se.id = bsx.series_id
            LEFT JOIN biblio_collections bcx ON bcx.biblio_id = b.id
            LEFT JOIN collections co ON co.id = bcx.collection_id
            LEFT JOIN items it ON it.biblio_id = b.id AND it.archived_at IS NULL
            WHERE b.id = $1
            GROUP BY b.id, ed.publisher_name
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(doc)
    }

    /// Build Meilisearch documents for all biblios (used for full reindex).
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_all_meili_documents(&self) -> AppResult<Vec<MeiliBiblioDocument>> {
        let docs = sqlx::query_as::<_, MeiliBiblioDocument>(
            r#"
            SELECT
                b.id,
                b.media_type,
                b.isbn::text AS isbn,
                b.title,
                COALESCE(
                    string_agg(DISTINCT concat_ws(' ', a.lastname, a.firstname), ', ')
                    FILTER (WHERE a.id IS NOT NULL),
                    ''
                ) AS author_names,
                b.subject,
                COALESCE(b.keywords, '{}') AS keywords,
                ed.publisher_name,
                COALESCE(
                    string_agg(DISTINCT se.name, ', ') FILTER (WHERE se.name IS NOT NULL),
                    ''
                ) AS series_name,
                COALESCE(
                    string_agg(DISTINCT co.name, ', ') FILTER (WHERE co.name IS NOT NULL),
                    ''
                ) AS collection_name,
                COALESCE(
                    array_agg(DISTINCT it.barcode) FILTER (WHERE it.barcode IS NOT NULL),
                    '{}'
                ) AS barcodes,
                COALESCE(
                    array_agg(DISTINCT it.call_number) FILTER (WHERE it.call_number IS NOT NULL),
                    '{}'
                ) AS call_numbers,
                b.abstract AS abstract_text,
                b.notes,
                b.table_of_contents,
                b.lang,
                b.audience_type,
                (b.archived_at IS NOT NULL) AS is_archived
            FROM biblios b
            LEFT JOIN biblio_authors ba ON ba.biblio_id = b.id
            LEFT JOIN authors a ON a.id = ba.author_id
            LEFT JOIN editions ed ON ed.id = b.edition_id
            LEFT JOIN biblio_series bsx ON bsx.biblio_id = b.id
            LEFT JOIN series se ON se.id = bsx.series_id
            LEFT JOIN biblio_collections bcx ON bcx.biblio_id = b.id
            LEFT JOIN collections co ON co.id = bcx.collection_id
            LEFT JOIN items it ON it.biblio_id = b.id AND it.archived_at IS NULL
            GROUP BY b.id, ed.publisher_name
            ORDER BY b.id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(docs)
    }

    /// Load BiblioShort rows for the given IDs, preserving the input order (Meilisearch ranking).
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

    // =========================================================================
    // CREATE
    // =========================================================================

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
                lang, lang_orig, title, subject,
                audience_type, page_extent, format, table_of_contents, accompanying_material,
                abstract, notes, keywords, is_valid,
                edition_id, created_at, updated_at
            ) VALUES (
                $1, $2, $3,
                $4, $5, $6, $7,
                $8, $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19
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
        .bind(&biblio.audience_type)
        .bind(&biblio.page_extent)
        .bind(&biblio.format)
        .bind(&biblio.table_of_contents)
        .bind(&biblio.accompanying_material)
        .bind(&biblio.abstract_)
        .bind(&biblio.notes)
        .bind(&biblio.keywords)
        .bind(&biblio.is_valid)
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

    // =========================================================================
    // UPDATE
    // =========================================================================

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

    // =========================================================================
    // DELETE (archive)
    // =========================================================================

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

        // prefix barcode with ARCH_<timestamp>_<BARCODE>
        sqlx::query(
            "UPDATE items SET archived_at = $1, updated_at = $1, barcode = CONCAT('ARCH_', $2, '_', barcode) WHERE biblio_id = $3 AND archived_at IS NULL"
        )
        .bind(now)
        .bind(now.format("%Y%m%d%H%M%S").to_string())
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

    // =========================================================================
    // AUTHORS (biblio_authors junction)
    // =========================================================================

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

    // =========================================================================
    // ITEMS (physical copies — formerly specimens)
    // =========================================================================

    /// Get items (physical copies) for a biblio (excludes archived items)
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_items(&self, biblio_id: i64) -> AppResult<Vec<Item>> {
        let items = sqlx::query_as::<_, Item>(
            r#"
            SELECT i.id, i.biblio_id, i.source_id, i.barcode, i.call_number, i.volume_designation,
                   i.place, i.borrowable, i.circulation_status, i.notes, i.price,
                   i.created_at, i.updated_at, i.archived_at,
                   so.name as source_name,
                   (SELECT COUNT(*) FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as availability
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.biblio_id = $1 AND i.archived_at IS NULL
            ORDER BY i.barcode
            "#,
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    /// Get ItemShort for many biblios (excludes archived). Used to attach items to BiblioShort lists.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_items_short_by_biblio_ids(
        &self,
        biblio_ids: &[i64],
    ) -> AppResult<HashMap<i64, Vec<ItemShort>>> {
        if biblio_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<ItemShortRow> = sqlx::query_as(
            r#"
            SELECT i.biblio_id, i.id, i.barcode, i.call_number, i.borrowable,
                   so.name as source_name,
                   (SELECT COUNT(*) FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as availability
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.biblio_id = ANY($1) AND i.archived_at IS NULL
            ORDER BY i.biblio_id, i.barcode
            "#,
        )
        .bind(biblio_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut map: HashMap<i64, Vec<ItemShort>> = HashMap::new();
        for row in rows {
            map.entry(row.biblio_id)
                .or_default()
                .push(ItemShort::from(row));
        }
        Ok(map)
    }

    /// Create an item (physical copy) for a biblio
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_create_item(&self, biblio_id: i64, item: &Item) -> AppResult<Item> {
        let now = Utc::now();
        let mut new_item = item.clone();
        let source_id = if let Some(id) = item.source_id {
            Some(id)
        } else if let Some(ref name) = item.source_name {
            Some(self.sources_find_or_create_by_name(name).await?)
        } else {
            None
        };
        new_item.source_id = source_id;

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO items (
                biblio_id, barcode, call_number, volume_designation, place, borrowable, notes, price, source_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
            RETURNING id
            "#,
        )
        .bind(biblio_id)
        .bind(&item.barcode)
        .bind(&item.call_number)
        .bind(&item.volume_designation)
        .bind(&item.place)
        .bind(item.borrowable)
        .bind(&item.notes)
        .bind(&item.price)
        .bind(source_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        new_item.id = Some(id);
        Ok(new_item)
    }

    /// Upsert an item (physical copy)
    #[tracing::instrument(skip(self), err)]
    pub async fn upsert_item<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let now = Utc::now();
        item.updated_at = Some(now);

        if let Some(id) = item.id {
            sqlx::query(
                r#"
                UPDATE items SET
                    biblio_id = $1,
                    barcode = $2,
                    call_number = $3,
                    volume_designation = $4,
                    place = $5,
                    borrowable = $6,
                    notes = $7,
                    price = $8,
                    source_id = $9,
                    updated_at = $10
                WHERE id = $11
                "#,
            )
            .bind(&item.biblio_id)
            .bind(&item.barcode)
            .bind(&item.call_number)
            .bind(&item.volume_designation)
            .bind(&item.place)
            .bind(item.borrowable)
            .bind(&item.notes)
            .bind(&item.price)
            .bind(&item.source_id)
            .bind(&item.updated_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        } else {
            if let Some(ref barcode) = item.barcode {
                let existing_id = sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM items WHERE barcode = $1",
                )
                .bind(barcode)
                .fetch_optional(&self.pool)
                .await?;
                item.id = existing_id;
            }

            if let Some(id) = item.id {
                sqlx::query(
                    r#"
                    UPDATE items SET
                        biblio_id = $1,
                        barcode = $2,
                        call_number = $3,
                        volume_designation = $4,
                        place = $5,
                        borrowable = $6,
                        notes = $7,
                        price = $8,
                        source_id = $9,
                        updated_at = $10
                    WHERE id = $11
                    "#,
                )
                .bind(&item.biblio_id)
                .bind(&item.barcode)
                .bind(&item.call_number)
                .bind(&item.volume_designation)
                .bind(&item.place)
                .bind(item.borrowable)
                .bind(&item.notes)
                .bind(&item.price)
                .bind(&item.source_id)
                .bind(&item.updated_at)
                .bind(id)
                .execute(&self.pool)
                .await?;
            } else {
                let id = sqlx::query_scalar::<_, i64>(
                    r#"
                    INSERT INTO items (
                        biblio_id, barcode, call_number, volume_designation,
                        place, borrowable, notes, price, source_id, created_at, updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
                    RETURNING id
                    "#,
                )
                .bind(&item.biblio_id)
                .bind(&item.barcode)
                .bind(&item.call_number)
                .bind(&item.volume_designation)
                .bind(&item.place)
                .bind(item.borrowable)
                .bind(&item.notes)
                .bind(&item.price)
                .bind(&item.source_id)
                .bind(&item.updated_at)
                .fetch_one(&self.pool)
                .await?;

                item.id = Some(id);
            }
        }
       
        Ok(item)
    }

    /// Update an item (physical copy)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_update<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let now = Utc::now();
        item.updated_at = Some(now);
        sqlx::query(
            r#"
            UPDATE items SET
                barcode = COALESCE($1, barcode),
                call_number = COALESCE($2, call_number),
                volume_designation = COALESCE($3, volume_designation),
                place = COALESCE($4, place),
                borrowable = COALESCE($5, borrowable),
                notes = COALESCE($6, notes),
                price = COALESCE($7, price),
                source_id = COALESCE($8, source_id),
                updated_at = $9
            WHERE id = $10
            "#
        )
        .bind(&item.barcode)
        .bind(&item.call_number)
        .bind(&item.volume_designation)
        .bind(&item.place)
        .bind(item.borrowable)
        .bind(&item.notes)
        .bind(&item.price)
        .bind(&item.source_id)
        .bind(&item.updated_at)
        .bind(item.id.unwrap_or(0))
        .execute(&self.pool)
        .await?;

        Ok(item)
    }

    /// Delete an item (physical copy — soft delete, sets archived_at)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_delete(&self, id: i64, force: bool) -> AppResult<()> {
        let now = Utc::now();

        let borrowed = self.loans_count_active_for_item(id).await?;

        if borrowed > 0 {
            if !force {
                return Err(AppError::BusinessRule(
                    "Item is currently borrowed. Use force=true to delete anyway.".to_string()
                ));
            }
            let loan_ids = self.loans_get_active_ids_for_item(id).await?;
            for loan_id in loan_ids {
                self.loans_return(loan_id).await?;
            }
        }

        sqlx::query(
            "UPDATE items SET archived_at = $1, updated_at = $1, barcode = CONCAT('ARCH_', $2, '_', barcode) WHERE biblio_id = $3 AND archived_at IS NULL"
        )
        .bind(now)
        .bind(now.format("%Y%m%d%H%M%S").to_string())
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if item barcode already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn items_barcode_exists(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_item_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM items WHERE barcode = $1 AND id != $2)")
                .bind(barcode)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM items WHERE barcode = $1)")
                .bind(barcode)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(exists)
    }

    /// Get item id and archived_at by barcode
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_by_barcode(&self, barcode: &str) -> AppResult<Option<(i64, bool)>> {
        let row: Option<(i64, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, archived_at FROM items WHERE barcode = $1",
        )
        .bind(barcode)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, archived_at)| (id, archived_at.is_some())))
    }

    /// Reactivate an archived item and update its fields.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_reactivate(
        &self,
        item_id: i64,
        biblio_id: i64,
        item: &Item,
    ) -> AppResult<Item> {
        let now = Utc::now();
        let source_id = if let Some(id) = item.source_id {
            Some(id)
        } else if let Some(ref name) = item.source_name {
            Some(self.sources_find_or_create_by_name(name).await?)
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE items SET
                biblio_id = $1, barcode = $2, call_number = $3, volume_designation = $4,
                place = $5, borrowable = $6,
                notes = $7, price = $8, source_id = $9,
                archived_at = NULL,
                updated_at = $10
            WHERE id = $11
            "#,
        )
        .bind(biblio_id)
        .bind(&item.barcode)
        .bind(&item.call_number)
        .bind(&item.volume_designation)
        .bind(&item.place)
        .bind(item.borrowable)
        .bind(&item.notes)
        .bind(&item.price)
        .bind(source_id)
        .bind(now)
        .bind(item_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_as::<_, Item>(
            r#"
            SELECT i.id, i.biblio_id, i.source_id, i.barcode, i.call_number, i.volume_designation,
                   i.place, i.borrowable, i.circulation_status, i.notes, i.price,
                   i.created_at, i.updated_at, i.archived_at,
                   so.name as source_name,
                   (SELECT COUNT(*) FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as availability
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.id = $1
            "#,
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    // =========================================================================
    // ISBN / BARCODE DUPLICATE CHECKS
    // =========================================================================

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

    /// Find an existing item by barcode and return its short representation.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_find_short_by_barcode(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<Option<ItemShort>> {
        let row: Option<ItemShortRow> = if let Some(eid) = exclude_item_id {
            sqlx::query_as(
                r#"
                SELECT i.biblio_id, i.id, i.barcode, i.call_number, i.borrowable,
                       so.name as source_name,
                       (SELECT COUNT(*) FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as availability
                FROM items i
                LEFT JOIN sources so ON i.source_id = so.id
                WHERE i.barcode = $1 AND i.id != $2 AND i.archived_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(barcode)
            .bind(eid)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT i.biblio_id, i.id, i.barcode, i.call_number, i.borrowable,
                       so.name as source_name,
                       (SELECT COUNT(*) FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as availability
                FROM items i
                LEFT JOIN sources so ON i.source_id = so.id
                WHERE i.barcode = $1 AND i.archived_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(barcode)
            .fetch_optional(&self.pool)
            .await?
        };
        Ok(row.map(ItemShort::from))
    }

    // =========================================================================
    // ISBN DEDUPLICATION
    // =========================================================================

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


