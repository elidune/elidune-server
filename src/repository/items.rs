//! Items domain methods on Repository.
//!
//! Uses marc-rs types (Leader, MarcFormat, etc.) where applicable; DB serialization
//! uses the associated char or int (e.g. media_type string from Leader record_type).

use std::collections::HashMap;
use chrono::Utc;
use sqlx::{FromRow, Row};
use sqlx::types::Json;

use super::Repository;
use crate::models::specimen::SpecimenShort;
use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        author::Author,
        author::Function,
        import_report::DuplicateCandidate,
        item::{Collection, Edition, Isbn, Item, ItemQuery, ItemShort, MeiliItemDocument, MediaType, Serie},
        specimen::Specimen,
    },
};

/// Internal row type for decoding ItemShort with JSONB author (specimens loaded separately).
#[derive(FromRow)]
struct ItemShortRow {
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

/// Row type for specimen short data from SQL (build SpecimenShort in Rust).
#[derive(FromRow)]
struct SpecimenShortRow {
    item_id: i64,
    id: i64,
    barcode: Option<String>,
    call_number: Option<String>,
    borrowable: bool,
    source_name: Option<String>,
    availability: Option<i64>,
}

impl From<SpecimenShortRow> for SpecimenShort {
    fn from(r: SpecimenShortRow) -> Self {
        Self {
            id: r.id,
            barcode: r.barcode,
            call_number: r.call_number,
            borrowable: r.borrowable,
            source_name: r.source_name,
            availability: r.availability,
        }
    }
}

impl From<ItemShortRow> for ItemShort {
    fn from(r: ItemShortRow) -> Self {
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
            specimens: Vec::new(),
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
    // READ (items)
    // =========================================================================

    /// Get item by numeric ID or by ISBN.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_by_id_or_isbn(&self, id_or_isbn: &str) -> AppResult<Item> {

        let query = r#"
            SELECT id, media_type, isbn,
                   publication_date, lang, lang_orig, title,
                   subject, audience_type, page_extent, format,
                   table_of_contents, accompanying_material,
                   abstract as abstract_, notes, keywords, 
                   edition_id,
                   collection_id, collection_sequence_number, collection_volume_number,
                   is_valid,
                   created_at, updated_at, archived_at
            FROM items
            WHERE (id = $1 OR isbn = $2) AND archived_at IS NULL
            "#;
            
        // query id and isbn in the same query
        let mut item = sqlx::query_as::<_, Item>(query)
        .bind(id_or_isbn.parse::<i64>().unwrap_or(0))
        .bind(Isbn::new(id_or_isbn).to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Item with id {} not found", id_or_isbn)))?;

 

        let id = item.id.ok_or_else(|| AppError::Internal("Item id is null".to_string()))?;

        item.authors = self.get_item_authors(id).await?;

        self.load_item_series(id, &mut item).await?;

        item.collection = sqlx::query_as::<_, Collection>(
            "SELECT id, key, primary_title, secondary_title, tertiary_title, issn, created_at, updated_at FROM collections WHERE id = $1",
        )
        .bind(item.collection_id)
        .fetch_optional(&self.pool)
        .await?;

        item.edition = sqlx::query_as::<_, Edition>(
            "SELECT id, publisher_name, place_of_publication, date, created_at, updated_at FROM editions WHERE id = $1",
        )
        .bind(item.edition_id)
        .fetch_optional(&self.pool)
        .await?;

        item.specimens = self.items_get_specimens(id).await?;

        Ok(item)
    }


  
    /// Load all authors for an item via the item_authors junction table
    async fn get_item_authors(&self, item_id: i64) -> AppResult<Vec<Author>> {
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.lastname, a.firstname, a.bio, a.notes, ia.function
            FROM item_authors ia
            JOIN authors a ON a.id = ia.author_id
            WHERE ia.item_id = $1
            ORDER BY ia.position
            "#,
        )
        .bind(item_id)
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

    /// Load N:M series links into `item` (series_ids, series_volume_numbers, series).
    async fn load_item_series(&self, item_id: i64, item: &mut Item) -> AppResult<()> {
        let rows = sqlx::query(
            r#"
            SELECT isx.series_id, isx.volume_number,
                   s.id, s.key, s.name, s.issn, s.created_at, s.updated_at
            FROM item_series isx
            INNER JOIN series s ON s.id = isx.series_id
            WHERE isx.item_id = $1
            ORDER BY isx.position
            "#,
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;

        item.series_ids.clear();
        item.series_volume_numbers.clear();
        item.series.clear();

        for row in rows {
            let sid: i64 = row.get("series_id");
            let vol: Option<i16> = row.get("volume_number");
            item.series_ids.push(sid);
            item.series_volume_numbers.push(vol);
            item.series.push(Serie {
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

    /// Resolve `item.series` (nested Serie payloads) into `series_ids` / `series_volume_numbers`,
    /// or keep explicit `series_ids` when `series` is empty.
    async fn resolve_series_ids_from_item(&self, item: &mut Item) -> AppResult<()> {
        if !item.series.is_empty() {
            let mut ids = Vec::new();
            let mut vols = Vec::new();
            for s in &item.series {
                if let Some(id) = self.process_serie(&Some(s.clone())).await? {
                    ids.push(id);
                    vols.push(s.volume_number);
                }
            }
            item.series_ids = ids;
            item.series_volume_numbers = vols;
        } else {
            while item.series_volume_numbers.len() < item.series_ids.len() {
                item.series_volume_numbers.push(None);
            }
            item.series_volume_numbers.truncate(item.series_ids.len());
        }
        Ok(())
    }

    /// Replace `item_series` rows for this item within an open transaction.
    async fn sync_item_series_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        item_id: i64,
        series_ids: &[i64],
        volumes: &[Option<i16>],
    ) -> AppResult<()> {
        sqlx::query("DELETE FROM item_series WHERE item_id = $1")
            .bind(item_id)
            .execute(&mut **tx)
            .await?;

        for (pos, &sid) in series_ids.iter().enumerate() {
            let vol = volumes.get(pos).copied().flatten();
            sqlx::query(
                r#"
                INSERT INTO item_series (item_id, series_id, position, volume_number)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(item_id)
            .bind(sid)
            .bind((pos + 1) as i16)
            .bind(vol)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }


    /// Get a short item representation by ID (includes author + specimens).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_short_by_id(&self, id: i64) -> AppResult<ItemShort> {
        let row: ItemShortRow = sqlx::query_as(
            r#"
            SELECT i.id, i.media_type, i.isbn, i.title,
                   i.publication_date as date, 0::smallint as status,
                   1::smallint as is_local, i.is_valid, i.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ia.function
                       )
                       FROM item_authors ia
                       JOIN authors a ON a.id = ia.author_id
                       WHERE ia.item_id = i.id
                       ORDER BY ia.position LIMIT 1
                   ) as author
            FROM items i
            WHERE i.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Item with id {} not found", id)))?;

        let mut short = ItemShort::from(row);
        let specimens_map = self.items_get_specimens_short_by_item_ids(&[short.id]).await?;
        short.specimens = specimens_map.get(&short.id).cloned().unwrap_or_default();
        Ok(short)
    }

    // =========================================================================
    // SEARCH
    // =========================================================================

    /// Search items with parameterized field filters. All non-freesearch ItemQuery fields
    /// are handled here. When freesearch is present, the CatalogService routes through
    /// Meilisearch instead; this path handles field-only filters and Meilisearch fallback.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_search(&self, query: &ItemQuery) -> AppResult<(Vec<ItemShort>, i64)> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(20).clamp(1, 200);
        let offset = (page - 1) * per_page;

        // Typed parameter enum to build dynamic queries safely.
        #[derive(Debug)]
        enum Param {
            Text(String),
            I16(i16),
        }

        let mut where_parts: Vec<String> = Vec::new();
        let mut params: Vec<Param> = Vec::new();

        // archive filter (no param)
        if query.archive.unwrap_or(false) {
            where_parts.push("i.archived_at IS NOT NULL".to_string());
        } else {
            where_parts.push("i.archived_at IS NULL".to_string());
        }

        // media_type (exact)
        if let Some(ref mt) = query.media_type {
            params.push(Param::Text(mt.clone()));
            where_parts.push(format!("i.media_type = ${}", params.len()));
        }

        // isbn (exact, normalised)
        if let Some(ref isbn) = query.isbn {
            params.push(Param::Text(isbn.to_string()));
            where_parts.push(format!("i.isbn = ${}", params.len()));
        }

        // barcode → specimen lookup
        if let Some(ref barcode) = query.barcode {
            params.push(Param::Text(barcode.clone()));
            where_parts.push(format!(
                "EXISTS (SELECT 1 FROM specimens s WHERE s.item_id = i.id AND s.barcode = ${})",
                params.len()
            ));
        }

        // audience_type (exact)
        if let Some(ref at) = query.audience_type {
            params.push(Param::Text(at.clone()));
            where_parts.push(format!("i.audience_type = ${}", params.len()));
        }

        // lang (exact)
        if let Some(ref lang) = query.lang {
            params.push(Param::Text(lang.clone()));
            where_parts.push(format!("i.lang = ${}", params.len()));
        }

        // title (accent-insensitive substring)
        if let Some(ref title) = query.title {
            params.push(Param::Text(format!("%{}%", like_escape(title))));
            let idx = params.len();
            where_parts.push(format!(
                "unaccent(lower(i.title)) LIKE unaccent(lower(${idx}))"
            ));
        }

        // subject
        if let Some(ref subject) = query.subject {
            params.push(Param::Text(format!("%{}%", like_escape(subject))));
            let idx = params.len();
            where_parts.push(format!(
                "unaccent(lower(i.subject)) LIKE unaccent(lower(${idx}))"
            ));
        }

        // keywords array
        if let Some(ref kw) = query.keywords {
            params.push(Param::Text(format!("%{}%", like_escape(kw))));
            let idx = params.len();
            where_parts.push(format!(
                "EXISTS (SELECT 1 FROM unnest(i.keywords) AS kw \
                 WHERE unaccent(lower(kw)) LIKE unaccent(lower(${idx})))"
            ));
        }

        // content → table_of_contents OR abstract
        if let Some(ref content) = query.content {
            params.push(Param::Text(format!("%{}%", like_escape(content))));
            let idx = params.len();
            where_parts.push(format!(
                "(unaccent(lower(i.table_of_contents)) LIKE unaccent(lower(${idx})) \
                 OR unaccent(lower(i.abstract)) LIKE unaccent(lower(${idx})))"
            ));
        }

        // author (lastname or firstname)
        if let Some(ref author) = query.author {
            params.push(Param::Text(format!("%{}%", like_escape(author))));
            let idx = params.len();
            where_parts.push(format!(
                "EXISTS (\
                    SELECT 1 FROM item_authors ia \
                    JOIN authors a ON a.id = ia.author_id \
                    WHERE ia.item_id = i.id \
                    AND (unaccent(lower(a.lastname)) LIKE unaccent(lower(${idx})) \
                         OR unaccent(lower(a.firstname)) LIKE unaccent(lower(${idx})))\
                )"
            ));
        }

        // editor → editions.publisher_name
        if let Some(ref editor) = query.editor {
            params.push(Param::Text(format!("%{}%", like_escape(editor))));
            let idx = params.len();
            where_parts.push(format!(
                "EXISTS (\
                    SELECT 1 FROM editions e \
                    WHERE e.id = i.edition_id \
                    AND unaccent(lower(e.publisher_name)) LIKE unaccent(lower(${idx}))\
                )"
            ));
        }

        // freesearch with ILIKE fallback when Meilisearch is not routing (rare / fallback path)
        if let Some(ref fs) = query.freesearch {
            let fs = fs.trim();
            if !fs.is_empty() {
                params.push(Param::Text(format!("%{}%", like_escape(fs))));
                let idx = params.len();
                where_parts.push(format!(
                    "(unaccent(lower(i.title)) LIKE unaccent(lower(${idx})) \
                     OR unaccent(lower(i.subject)) LIKE unaccent(lower(${idx})) \
                     OR unaccent(lower(i.notes)) LIKE unaccent(lower(${idx})))"
                ));
            }
        }

        let where_sql = if where_parts.is_empty() {
            "1=1".to_string()
        } else {
            where_parts.join(" AND ")
        };

        let order_sql = "i.title ASC NULLS LAST".to_string();

        // Single query with COUNT(*) OVER() to avoid two round-trips.
        let sql = format!(
            r#"
            SELECT i.id, i.media_type, i.isbn, i.title,
                   i.publication_date AS date, 0::smallint AS status,
                   1::smallint AS is_local, i.is_valid, i.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ia.function
                       )
                       FROM item_authors ia
                       JOIN authors a ON a.id = ia.author_id
                       WHERE ia.item_id = i.id
                       ORDER BY ia.position LIMIT 1
                   ) AS author,
                   COUNT(*) OVER() AS total_count
            FROM items i
            WHERE {where}
            ORDER BY {order}
            LIMIT {limit} OFFSET {offset}
            "#,
            where = where_sql,
            order = order_sql,
            limit = per_page,
            offset = offset,
        );

        // Bind parameters in order.
        use sqlx::Arguments;
        let mut pg_args = sqlx::postgres::PgArguments::default();
        for p in &params {
            match p {
                Param::Text(s) => pg_args.add(s.clone()),
                Param::I16(v) => pg_args.add(*v),
            }
        }

        #[derive(FromRow)]
        struct ItemShortWithCount {
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

        let rows: Vec<ItemShortWithCount> = sqlx::query_as_with(&sql, pg_args)
            .fetch_all(&self.pool)
            .await?;

        let total = rows.first().map(|r| r.total_count).unwrap_or(0);
        let item_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let specimens_map = self.items_get_specimens_short_by_item_ids(&item_ids).await?;

        let items: Vec<ItemShort> = rows
            .into_iter()
            .map(|r| {
                let mut short = ItemShort {
                    id: r.id,
                    media_type: r.media_type,
                    isbn: r.isbn,
                    title: r.title,
                    date: r.date,
                    status: r.status,
                    is_valid: r.is_valid,
                    archived_at: r.archived_at,
                    author: r.author.map(|j| j.0),
                    specimens: Vec::new(),
                };
                short.specimens = specimens_map.get(&short.id).cloned().unwrap_or_default();
                short
            })
            .collect();

        Ok((items, total))
    }

    /// List all items belonging to a series
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_by_series(&self, series_id: i64) -> AppResult<Vec<ItemShort>> {
        let rows: Vec<ItemShortRow> = sqlx::query_as(
            r#"
            SELECT i.id, i.media_type, i.isbn, i.title,
                   i.publication_date as date, 0::smallint as status,
                   1::smallint as is_local, i.is_valid, i.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ia.function
                       )
                       FROM item_authors ia
                       JOIN authors a ON a.id = ia.author_id
                       WHERE ia.item_id = i.id
                       ORDER BY ia.position LIMIT 1
                   ) as author
            FROM items i
            INNER JOIN item_series isx ON isx.item_id = i.id AND isx.series_id = $1
            WHERE i.archived_at IS NULL
            ORDER BY isx.volume_number NULLS LAST, i.title
            "#,
        )
        .bind(series_id)
        .fetch_all(&self.pool)
        .await?;

        let item_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let specimens_map = self.items_get_specimens_short_by_item_ids(&item_ids).await?;
        let items: Vec<ItemShort> = rows
            .into_iter()
            .map(|r| {
                let mut short = ItemShort::from(r);
                short.specimens = specimens_map.get(&short.id).cloned().unwrap_or_default();
                short
            })
            .collect();

        Ok(items)
    }

    // =========================================================================
    // MEILISEARCH DOCUMENT BUILDERS
    // =========================================================================

    /// Build a single Meilisearch document for the given item ID using a single JOIN query.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_meili_document(&self, id: i64) -> AppResult<Option<MeiliItemDocument>> {
        let doc = sqlx::query_as::<_, MeiliItemDocument>(
            r#"
            SELECT
                i.id,
                i.media_type,
                i.isbn::text AS isbn,
                i.title,
                COALESCE(
                    string_agg(DISTINCT concat_ws(' ', a.lastname, a.firstname), ', ')
                    FILTER (WHERE a.id IS NOT NULL),
                    ''
                ) AS author_names,
                i.subject,
                COALESCE(i.keywords, '{}') AS keywords,
                ed.publisher_name,
                COALESCE(
                    string_agg(DISTINCT se.name, ', ') FILTER (WHERE se.name IS NOT NULL),
                    ''
                ) AS series_name,
                co.primary_title AS collection_name,
                COALESCE(
                    array_agg(DISTINCT sp.barcode) FILTER (WHERE sp.barcode IS NOT NULL),
                    '{}'
                ) AS barcodes,
                COALESCE(
                    array_agg(DISTINCT sp.call_number) FILTER (WHERE sp.call_number IS NOT NULL),
                    '{}'
                ) AS call_numbers,
                i.abstract AS abstract_text,
                i.notes,
                i.table_of_contents,
                i.lang,
                i.audience_type,
                (i.archived_at IS NOT NULL) AS is_archived
            FROM items i
            LEFT JOIN item_authors ia ON ia.item_id = i.id
            LEFT JOIN authors a ON a.id = ia.author_id
            LEFT JOIN editions ed ON ed.id = i.edition_id
            LEFT JOIN item_series isx ON isx.item_id = i.id
            LEFT JOIN series se ON se.id = isx.series_id
            LEFT JOIN collections co ON co.id = i.collection_id
            LEFT JOIN specimens sp ON sp.item_id = i.id AND sp.archived_at IS NULL
            WHERE i.id = $1
            GROUP BY i.id, ed.publisher_name, co.primary_title
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(doc)
    }

    /// Build Meilisearch documents for all items (used for full reindex).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_all_meili_documents(&self) -> AppResult<Vec<MeiliItemDocument>> {
        let docs = sqlx::query_as::<_, MeiliItemDocument>(
            r#"
            SELECT
                i.id,
                i.media_type,
                i.isbn::text AS isbn,
                i.title,
                COALESCE(
                    string_agg(DISTINCT concat_ws(' ', a.lastname, a.firstname), ', ')
                    FILTER (WHERE a.id IS NOT NULL),
                    ''
                ) AS author_names,
                i.subject,
                COALESCE(i.keywords, '{}') AS keywords,
                ed.publisher_name,
                COALESCE(
                    string_agg(DISTINCT se.name, ', ') FILTER (WHERE se.name IS NOT NULL),
                    ''
                ) AS series_name,
                co.primary_title AS collection_name,
                COALESCE(
                    array_agg(DISTINCT sp.barcode) FILTER (WHERE sp.barcode IS NOT NULL),
                    '{}'
                ) AS barcodes,
                COALESCE(
                    array_agg(DISTINCT sp.call_number) FILTER (WHERE sp.call_number IS NOT NULL),
                    '{}'
                ) AS call_numbers,
                i.abstract AS abstract_text,
                i.notes,
                i.table_of_contents,
                i.lang,
                i.audience_type,
                (i.archived_at IS NOT NULL) AS is_archived
            FROM items i
            LEFT JOIN item_authors ia ON ia.item_id = i.id
            LEFT JOIN authors a ON a.id = ia.author_id
            LEFT JOIN editions ed ON ed.id = i.edition_id
            LEFT JOIN item_series isx ON isx.item_id = i.id
            LEFT JOIN series se ON se.id = isx.series_id
            LEFT JOIN collections co ON co.id = i.collection_id
            LEFT JOIN specimens sp ON sp.item_id = i.id AND sp.archived_at IS NULL
            GROUP BY i.id, ed.publisher_name, co.primary_title
            ORDER BY i.id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(docs)
    }

    /// Load ItemShort rows for the given IDs, preserving the input order (Meilisearch ranking).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_short_by_ids_ordered(&self, ids: &[i64]) -> AppResult<Vec<ItemShort>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<ItemShortRow> = sqlx::query_as(
            r#"
            SELECT i.id, i.media_type, i.isbn, i.title,
                   i.publication_date AS date, 0::smallint AS status,
                   1::smallint AS is_local, i.is_valid, i.archived_at,
                   (
                       SELECT jsonb_build_object(
                           'id', a.id::text,
                           'lastname', a.lastname,
                           'firstname', a.firstname,
                           'bio', a.bio,
                           'notes', a.notes,
                           'function', ia.function
                       )
                       FROM item_authors ia
                       JOIN authors a ON a.id = ia.author_id
                       WHERE ia.item_id = i.id
                       ORDER BY ia.position LIMIT 1
                   ) AS author
            FROM items i
            WHERE i.id = ANY($1)
            "#,
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        let id_to_index: std::collections::HashMap<i64, usize> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        let item_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let specimens_map = self.items_get_specimens_short_by_item_ids(&item_ids).await?;

        let mut items: Vec<(usize, ItemShort)> = rows
            .into_iter()
            .map(|r| {
                let pos = id_to_index.get(&r.id).copied().unwrap_or(usize::MAX);
                let mut short = ItemShort::from(r);
                short.specimens = specimens_map.get(&short.id).cloned().unwrap_or_default();
                (pos, short)
            })
            .collect();

        items.sort_by_key(|(pos, _)| *pos);
        Ok(items.into_iter().map(|(_, item)| item).collect())
    }

    // =========================================================================
    // CREATE
    // =========================================================================

    /// Create a new item.
    ///
    /// External entity resolution (series, collection, edition, author upserts) happens
    /// outside the transaction because those upserts are idempotent and safe to keep even
    /// on rollback.  The item row, series links, author links, and MARC record update are
    /// all committed atomically.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_create<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let now = Utc::now();

        item.updated_at = Some(now);
        item.created_at = Some(now);

        // Resolve external entities before opening the transaction.
        self.resolve_series_ids_from_item(item).await?;
        item.collection_id = self.process_collection(&item.collection).await?;
        item.edition_id = self.process_edition(&item.edition).await?;

        let mut tx = self.pool.begin().await?;

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO items (
                media_type, isbn, publication_date,
                lang, lang_orig, title, subject,
                audience_type, page_extent, format, table_of_contents, accompanying_material,
                abstract, notes, keywords, is_valid,
                collection_id, collection_sequence_number, collection_volume_number,
                edition_id, created_at, updated_at
            ) VALUES (
                $1, $2, $3,
                $4, $5, $6, $7,
                $8, $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19,
                $20, $21, $22
            ) RETURNING id
            "#,
        )
        .bind(&item.media_type)
        .bind(&item.isbn.as_ref().map(|i| i.to_string()))
        .bind(&item.publication_date)
        .bind(&item.lang)
        .bind(&item.lang_orig)
        .bind(&item.title)
        .bind(&item.subject)
        .bind(&item.audience_type)
        .bind(&item.page_extent)
        .bind(&item.format)
        .bind(&item.table_of_contents)
        .bind(&item.accompanying_material)
        .bind(&item.abstract_)
        .bind(&item.notes)
        .bind(&item.keywords)
        .bind(&item.is_valid)
        .bind(&item.collection_id)
        .bind(&item.collection_sequence_number)
        .bind(&item.collection_volume_number)
        .bind(&item.edition_id)
        .bind(&item.created_at)
        .bind(&item.updated_at)
        .fetch_one(&mut *tx)
        .await?;

        item.id = Some(id);

        self.sync_item_series_tx(&mut tx, id, &item.series_ids, &item.series_volume_numbers)
            .await?;
        self.sync_item_authors_tx(&mut tx, id, &item.authors).await?;

        // Build the MARC record from the current item state and persist it in the same tx.
        item.marc_record = Some(crate::marc::MarcRecord::from(&*item));
        sqlx::query("UPDATE items SET marc_record = $1 WHERE id = $2")
            .bind(serde_json::to_value(&item.marc_record).unwrap_or_default())
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        // Post-commit: re-load series links (for volume_number hydration).
        self.load_item_series(id, item).await?;

        Ok(item)
    }

    // =========================================================================
    // UPDATE
    // =========================================================================

    /// Update an existing item.
    ///
    /// The item row update, series sync, author sync, and MARC record update are wrapped
    /// in a single transaction so a partial failure cannot leave inconsistent linked data.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_update<'a>(&self, id: i64, item: &'a mut Item) -> AppResult<&'a mut Item> {
        item.updated_at = Some(Utc::now());
        item.id = Some(id);

        // Resolve external entities before the transaction.
        self.resolve_series_ids_from_item(item).await?;
        item.collection_id = self.process_collection(&item.collection).await?;
        item.edition_id = self.process_edition(&item.edition).await?;

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE items SET
                media_type = COALESCE($1::text, media_type),
                isbn = COALESCE($2::text, isbn),
                title = COALESCE($3::text, title),
                collection_id = $4,
                collection_sequence_number = $5,
                collection_volume_number = $6,
                edition_id = $7,
                updated_at = $8
            WHERE id = $9
            "#,
        )
        .bind(&item.media_type)
        .bind(&item.isbn.as_ref().map(|i| i.to_string()))
        .bind(&item.title)
        .bind(&item.collection_id)
        .bind(&item.collection_sequence_number)
        .bind(&item.collection_volume_number)
        .bind(&item.edition_id)
        .bind(&item.updated_at)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        self.sync_item_series_tx(&mut tx, id, &item.series_ids, &item.series_volume_numbers)
            .await?;

        if !item.authors.is_empty() {
            self.sync_item_authors_tx(&mut tx, id, &item.authors).await?;
        }

        // Re-derive the MARC record from the updated item and persist it.
        item.marc_record = Some(crate::marc::MarcRecord::from(&*item));
        sqlx::query("UPDATE items SET marc_record = $1 WHERE id = $2")
            .bind(serde_json::to_value(&item.marc_record).unwrap_or_default())
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        // Post-commit: re-load hydrated series.
        self.load_item_series(id, item).await?;

        Ok(item)
    }

 
    /// Update marc record for an item.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_update_marc_record_for_item(&self, item: &mut Item) -> AppResult<()> {
        
        if item.marc_record.is_none() {
            item.marc_record = sqlx::query_scalar::<_, Option<serde_json::Value>>(
                "SELECT marc_record FROM items WHERE id = $1",
            )
            .bind(item.id.unwrap_or(0))
            .fetch_optional(&self.pool)
            .await?
            .flatten()
            .and_then(|v| serde_json::from_value::<MarcRecord>(v).ok());
        }

        item.marc_record = Some(MarcRecord::from(&*item));

        sqlx::query(
            "UPDATE items SET marc_record = $1 WHERE id = $2",
        )
        .bind(serde_json::to_value(&item.marc_record).unwrap())
        .bind(item.id.unwrap_or(0))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // =========================================================================
    // DELETE (archive)
    // =========================================================================

    /// Delete an item (soft delete — sets archived_at)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_delete(&self, id: i64, force: bool) -> AppResult<()> {
        let now = Utc::now();

        let loans = self.loans_get_active_ids_for_item(id).await?;

        if loans.len() > 0 {
            if !force {
                return Err(AppError::BusinessRule(
                    "Item has borrowed specimens. Use force=true to delete anyway.".to_string()
                ));
            } else {
                for loan_id in loans {
                    self.loans_return(loan_id).await?;
                }
            }
            
        }

        sqlx::query(
            "UPDATE specimens SET archived_at = $1, updated_at = $1 WHERE item_id = $2 AND archived_at IS NULL"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "UPDATE items SET archived_at = $1, updated_at = $1 WHERE id = $2"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // =========================================================================
    // AUTHORS (item_authors junction)
    // =========================================================================

    /// Replace all authors for an item within an open transaction.
    /// Author rows are first resolved/created via the pool (idempotent upserts),
    /// then the junction rows are written through the transaction.
    async fn sync_item_authors_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        item_id: i64,
        authors: &[Author],
    ) -> AppResult<()> {
        // Resolve/create author records using the pool (outside the tx scope) so that
        // author data is never rolled back even if the item write fails later.
        let mut author_ids: Vec<Option<i64>> = Vec::with_capacity(authors.len());
        for author in authors {
            author_ids.push(self.ensure_author(author).await?);
        }

        sqlx::query("DELETE FROM item_authors WHERE item_id = $1")
            .bind(item_id)
            .execute(&mut **tx)
            .await?;

        for (idx, (author, author_id)) in authors.iter().zip(author_ids.iter()).enumerate() {
            let Some(author_id) = author_id else { continue };

            sqlx::query(
                r#"
                INSERT INTO item_authors (item_id, author_id, function, author_type, position)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (item_id, author_id, function) DO UPDATE SET position = $5
                "#,
            )
            .bind(item_id)
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

        let Some(ref primary_title) = collection.primary_title else {
            return Ok(None);
        };

        let key = normalize_key(primary_title);

        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM collections WHERE key = $1 OR primary_title = $2",
        )
        .bind(&key)
        .bind(primary_title)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = existing {
            Ok(Some(id))
        } else {
            let id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO collections (key, primary_title, secondary_title, tertiary_title, issn) VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(&key)
            .bind(primary_title)
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
    // SPECIMENS
    // =========================================================================

    /// Get specimens for an item (excludes archived specimens)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_specimens(&self, item_id: i64) -> AppResult<Vec<Specimen>> {
        let specimens = sqlx::query_as::<_, Specimen>(
            r#"
            SELECT s.id, s.item_id, s.source_id, s.barcode, s.call_number, s.volume_designation,
                   s.place, s.borrowable, s.circulation_status, s.notes, s.price,
                   s.created_at, s.updated_at, s.archived_at,
                   so.name as source_name,
                   (SELECT COUNT(*) FROM loans l WHERE l.specimen_id = s.id AND l.returned_at IS NULL) as availability
            FROM specimens s
            LEFT JOIN sources so ON s.source_id = so.id
            WHERE s.item_id = $1 AND s.archived_at IS NULL
            ORDER BY s.barcode
            "#,
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(specimens)
    }

    /// Get SpecimenShort for many items (excludes archived). Used to attach specimens to ItemShort lists.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_specimens_short_by_item_ids(
        &self,
        item_ids: &[i64],
    ) -> AppResult<HashMap<i64, Vec<SpecimenShort>>> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<SpecimenShortRow> = sqlx::query_as(
            r#"
            SELECT s.item_id, s.id, s.barcode, s.call_number, s.borrowable,
                   so.name as source_name,
                   (SELECT COUNT(*) FROM loans l WHERE l.specimen_id = s.id AND l.returned_at IS NULL) as availability
            FROM specimens s
            LEFT JOIN sources so ON s.source_id = so.id
            WHERE s.item_id = ANY($1) AND s.archived_at IS NULL
            ORDER BY s.item_id, s.barcode
            "#,
        )
        .bind(item_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut map: HashMap<i64, Vec<SpecimenShort>> = HashMap::new();
        for row in rows {
            map.entry(row.item_id)
                .or_default()
                .push(SpecimenShort::from(row));
        }
        Ok(map)
    }

    /// Create a specimen
    #[tracing::instrument(skip(self), err)]
    pub async fn items_create_specimen(&self, item_id: i64, specimen: &Specimen) -> AppResult<Specimen> {
        let now = Utc::now();
        let mut new_specimen = specimen.clone();
        let source_id = if let Some(id) = specimen.source_id {
            Some(id)
        } else if let Some(ref name) = specimen.source_name {
            Some(self.sources_find_or_create_by_name(name).await?)
        } else {
            None
        };
        new_specimen.source_id = source_id;

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO specimens (
                item_id, barcode, call_number, volume_designation, place, borrowable, notes, price, source_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
            RETURNING id
            "#,
        )
        .bind(item_id)
        .bind(&specimen.barcode)
        .bind(&specimen.call_number)
        .bind(&specimen.volume_designation)
        .bind(&specimen.place)
        .bind(specimen.borrowable)
        .bind(&specimen.notes)
        .bind(&specimen.price)
        .bind(source_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        new_specimen.id = Some(id);
        Ok(new_specimen)
    }

    /// Upsert a specimen
    #[tracing::instrument(skip(self), err)]
    pub async fn upsert_specimen<'a>(&self, specimen: &'a mut Specimen) -> AppResult<&'a mut Specimen> {
        let now = Utc::now();
        specimen.updated_at = Some(now);

        // 1) If the ID is already known, update directly by ID (ID has priority over barcode).
        if let Some(id) = specimen.id {
            sqlx::query(
                r#"
                UPDATE specimens SET
                    item_id = $1,
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
            .bind(&specimen.item_id)
            .bind(&specimen.barcode)
            .bind(&specimen.call_number)
            .bind(&specimen.volume_designation)
            .bind(&specimen.place)
            .bind(specimen.borrowable)
            .bind(&specimen.notes)
            .bind(&specimen.price)
            .bind(&specimen.source_id)
            .bind(&specimen.updated_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        } else {
            // 2) No ID: try to find an existing specimen by barcode.
            if let Some(ref barcode) = specimen.barcode {
                let existing_id = sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM specimens WHERE barcode = $1",
                )
                .bind(barcode)
                .fetch_optional(&self.pool)
                .await?;
                specimen.id = existing_id;
            }

            if let Some(id) = specimen.id {
                // A specimen already exists with this barcode: update it.
                sqlx::query(
                    r#"
                    UPDATE specimens SET
                        item_id = $1,
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
                .bind(&specimen.item_id)
                .bind(&specimen.barcode)
                .bind(&specimen.call_number)
                .bind(&specimen.volume_designation)
                .bind(&specimen.place)
                .bind(specimen.borrowable)
                .bind(&specimen.notes)
                .bind(&specimen.price)
                .bind(&specimen.source_id)
                .bind(&specimen.updated_at)
                .bind(id)
                .execute(&self.pool)
                .await?;
            } else {
                // 3) No specimen with this barcode: insert a new one.
                let id = sqlx::query_scalar::<_, i64>(
                    r#"
                    INSERT INTO specimens (
                        item_id,
                        barcode,
                        call_number,
                        volume_designation,
                        place,
                        borrowable,
                        notes,
                        price,
                        source_id,
                        created_at,
                        updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
                    RETURNING id
                    "#,
                )
                .bind(&specimen.item_id)
                .bind(&specimen.barcode)
                .bind(&specimen.call_number)
                .bind(&specimen.volume_designation)
                .bind(&specimen.place)
                .bind(specimen.borrowable)
                .bind(&specimen.notes)
                .bind(&specimen.price)
                .bind(&specimen.source_id)
                .bind(&specimen.updated_at)
                .fetch_one(&self.pool)
                .await?;

                specimen.id = Some(id);
            }
        }
       
        Ok(specimen)
    }

    /// Update a specimen
    #[tracing::instrument(skip(self), err)]
    pub async fn items_update_specimen<'a>(&self, specimen: &'a mut Specimen) -> AppResult<&'a mut Specimen> {
        let now = Utc::now();
        specimen.updated_at = Some(now);
        sqlx::query(
            r#"
            UPDATE specimens SET
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
        .bind(&specimen.barcode)
        .bind(&specimen.call_number)
        .bind(&specimen.volume_designation)
        .bind(&specimen.place)
        .bind(specimen.borrowable)
        .bind(&specimen.notes)
        .bind(&specimen.price)
        .bind(&specimen.source_id)
        .bind(&specimen.updated_at)
        .bind(specimen.id.unwrap_or(0))
        .execute(&self.pool)
        .await?;

        Ok(specimen)
    }

    /// Delete a specimen (soft delete — sets archived_at)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_delete_specimen(&self, id: i64, force: bool) -> AppResult<()> {
        let now = Utc::now();

        let item_id: Option<i64> = sqlx::query_scalar("SELECT item_id FROM specimens WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        let borrowed = self.loans_count_active_for_specimen(id).await?;

        if borrowed > 0 {
            if !force {
                return Err(AppError::BusinessRule(
                    "Specimen is currently borrowed. Use force=true to delete anyway.".to_string()
                ));
            }
            // Return all active loans for this specimen before archiving
            let loan_ids = self.loans_get_active_ids_for_specimen(id).await?;
            for loan_id in loan_ids {
                self.loans_return(loan_id).await?;
            }
        }

        sqlx::query(
            "UPDATE specimens SET archived_at = $1, updated_at = $1 WHERE id = $2"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if specimen barcode already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn items_specimen_barcode_exists(
        &self,
        barcode: &str,
        exclude_specimen_id: Option<i64>,
    ) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_specimen_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM specimens WHERE barcode = $1 AND id != $2)")
                .bind(barcode)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM specimens WHERE barcode = $1)")
                .bind(barcode)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(exists)
    }

    /// Get specimen id and archived_at by barcode
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_specimen_by_barcode(&self, barcode: &str) -> AppResult<Option<(i64, bool)>> {
        let row: Option<(i64, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, archived_at FROM specimens WHERE barcode = $1",
        )
        .bind(barcode)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, archived_at)| (id, archived_at.is_some())))
    }

    /// Reactivate an archived specimen and update its fields.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_reactivate_specimen(
        &self,
        specimen_id: i64,
        item_id: i64,
        specimen: &Specimen,
    ) -> AppResult<Specimen> {
        let now = Utc::now();
        let source_id = if let Some(id) = specimen.source_id {
            Some(id)
        } else if let Some(ref name) = specimen.source_name {
            Some(self.sources_find_or_create_by_name(name).await?)
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE specimens SET
                item_id = $1, barcode = $2, call_number = $3, volume_designation = $4,
                place = $5, borrowable = $6,
                notes = $7, price = $8, source_id = $9,
                archived_at = NULL,
                updated_at = $10
            WHERE id = $11
            "#,
        )
        .bind(item_id)
        .bind(&specimen.barcode)
        .bind(&specimen.call_number)
        .bind(&specimen.volume_designation)
        .bind(&specimen.place)
        .bind(specimen.borrowable)
        .bind(&specimen.notes)
        .bind(&specimen.price)
        .bind(source_id)
        .bind(now)
        .bind(specimen_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_as::<_, Specimen>(
            r#"
            SELECT s.id, s.item_id, s.source_id, s.barcode, s.call_number, s.volume_designation,
                   s.place, s.borrowable, s.circulation_status, s.notes, s.price,
                   s.created_at, s.updated_at, s.archived_at,
                   so.name as source_name,
                   (SELECT COUNT(*) FROM loans l WHERE l.specimen_id = s.id AND l.returned_at IS NULL) as availability
            FROM specimens s
            LEFT JOIN sources so ON s.source_id = so.id
            WHERE s.id = $1
            "#,
        )
        .bind(specimen_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    // =========================================================================
    // ISBN / BARCODE DUPLICATE CHECKS
    // =========================================================================

    /// Find an active (non-archived) item that has the given ISBN,
    /// optionally excluding a specific item id (useful during updates).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_find_active_by_isbn(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<Option<i64>> {
        let row: Option<i64> = if let Some(eid) = exclude_id {
            sqlx::query_scalar(
                "SELECT id FROM items WHERE isbn = $1 AND archived_at IS NULL AND id != $2 LIMIT 1",
            )
            .bind(isbn)
            .bind(eid)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT id FROM items WHERE isbn = $1 AND archived_at IS NULL LIMIT 1",
            )
            .bind(isbn)
            .fetch_optional(&self.pool)
            .await?
        };
        Ok(row)
    }

    /// Find an existing specimen by barcode and return its short representation,
    /// optionally excluding a specific specimen id (useful during updates).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_find_specimen_short_by_barcode(
        &self,
        barcode: &str,
        exclude_specimen_id: Option<i64>,
    ) -> AppResult<Option<SpecimenShort>> {
        let row: Option<SpecimenShortRow> = if let Some(eid) = exclude_specimen_id {
            sqlx::query_as(
                r#"
                SELECT s.item_id, s.id, s.barcode, s.call_number, s.borrowable,
                       so.name as source_name,
                       (SELECT COUNT(*) FROM loans l WHERE l.specimen_id = s.id AND l.returned_at IS NULL) as availability
                FROM specimens s
                LEFT JOIN sources so ON s.source_id = so.id
                WHERE s.barcode = $1 AND s.id != $2
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
                SELECT s.item_id, s.id, s.barcode, s.call_number, s.borrowable,
                       so.name as source_name,
                       (SELECT COUNT(*) FROM loans l WHERE l.specimen_id = s.id AND l.returned_at IS NULL) as availability
                FROM specimens s
                LEFT JOIN sources so ON s.source_id = so.id
                WHERE s.barcode = $1
                LIMIT 1
                "#,
            )
            .bind(barcode)
            .fetch_optional(&self.pool)
            .await?
        };
        Ok(row.map(SpecimenShort::from))
    }

    // =========================================================================
    // ISBN DEDUPLICATION (legacy — kept for backward compat)
    // =========================================================================

    /// Find an existing item by ISBN for import deduplication.
    /// Includes archived items. Returns the best candidate (non-archived first).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_find_by_isbn_for_import(&self, isbn: &str) -> AppResult<Option<DuplicateCandidate>> {
        let row: Option<(i64, Option<chrono::DateTime<Utc>>, i64)> = sqlx::query_as(
            r#"
            SELECT i.id,
                   i.archived_at,
                   (SELECT COUNT(*) FROM specimens s WHERE s.item_id = i.id AND s.archived_at IS NULL) AS specimen_count
            FROM items i
            WHERE i.isbn = $1
            ORDER BY (i.archived_at IS NULL) DESC, i.id DESC
            LIMIT 1
            "#,
        )
        .bind(isbn)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(item_id, archived_at, specimen_count)| DuplicateCandidate {
            item_id,
            archived_at,
            specimen_count,
        }))
    }

    /// Check if ISBN already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn items_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM items WHERE isbn = $1 AND id != $2)")
                .bind(isbn)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM items WHERE isbn = $1)")
                .bind(isbn)
                .fetch_one(&self.pool)
                .await?
        };

        Ok(exists)
    }

    /// Count non-archived specimens for a source (items domain owns specimens)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_count_specimens_for_source(&self, source_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM specimens WHERE source_id = $1 AND archived_at IS NULL",
        )
        .bind(source_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Reassign specimens from given source IDs to a new source
    #[tracing::instrument(skip(self), err)]
    pub async fn items_reassign_specimens_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        let result = sqlx::query("UPDATE specimens SET source_id = $1 WHERE source_id = ANY($2)")
            .bind(new_source_id)
            .bind(old_source_ids)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Reassign items from given source IDs to a new source
    #[tracing::instrument(skip(self), err)]
    pub async fn items_reassign_items_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64> {
        // Items no longer have a source_id; sources are attached to specimens.
        let _ = (old_source_ids, new_source_id);
        Ok(0)
    }
}


#[async_trait::async_trait]
impl crate::repository::ItemsRepository for Repository {
    async fn items_get_by_id_or_isbn(&self, id_or_isbn: &str) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::items_get_by_id_or_isbn(self, id_or_isbn).await
    }
    async fn items_get_short_by_id(&self, id: i64) -> crate::error::AppResult<crate::models::item::ItemShort> {
        Repository::items_get_short_by_id(self, id).await
    }
    async fn items_search(&self, query: &crate::models::item::ItemQuery) -> crate::error::AppResult<(Vec<crate::models::item::ItemShort>, i64)> {
        Repository::items_search(self, query).await
    }
    async fn items_get_by_series(&self, series_id: i64) -> crate::error::AppResult<Vec<crate::models::item::ItemShort>> {
        Repository::items_get_by_series(self, series_id).await
    }
    async fn items_get_meili_document(&self, id: i64) -> crate::error::AppResult<Option<crate::models::item::MeiliItemDocument>> {
        Repository::items_get_meili_document(self, id).await
    }
    async fn items_get_all_meili_documents(&self) -> crate::error::AppResult<Vec<crate::models::item::MeiliItemDocument>> {
        Repository::items_get_all_meili_documents(self).await
    }
    async fn items_get_short_by_ids_ordered(&self, ids: &[i64]) -> crate::error::AppResult<Vec<crate::models::item::ItemShort>> {
        Repository::items_get_short_by_ids_ordered(self, ids).await
    }
    async fn items_create<'a>(&self, item: &'a mut crate::models::item::Item) -> crate::error::AppResult<&'a mut crate::models::item::Item> {
        Repository::items_create(self, item).await
    }
    async fn items_update<'a>(&self, id: i64, item: &'a mut crate::models::item::Item) -> crate::error::AppResult<&'a mut crate::models::item::Item> {
        Repository::items_update(self, id, item).await
    }
    async fn items_delete(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::items_delete(self, id, force).await
    }
    async fn items_get_specimens(&self, item_id: i64) -> crate::error::AppResult<Vec<crate::models::specimen::Specimen>> {
        Repository::items_get_specimens(self, item_id).await
    }
    async fn items_get_specimens_short_by_item_ids(&self, item_ids: &[i64]) -> crate::error::AppResult<std::collections::HashMap<i64, Vec<crate::models::specimen::SpecimenShort>>> {
        Repository::items_get_specimens_short_by_item_ids(self, item_ids).await
    }
    async fn items_create_specimen(&self, item_id: i64, specimen: &crate::models::specimen::Specimen) -> crate::error::AppResult<crate::models::specimen::Specimen> {
        Repository::items_create_specimen(self, item_id, specimen).await
    }
    async fn upsert_specimen<'a>(&self, specimen: &'a mut crate::models::specimen::Specimen) -> crate::error::AppResult<&'a mut crate::models::specimen::Specimen> {
        Repository::upsert_specimen(self, specimen).await
    }
    async fn items_update_specimen<'a>(&self, specimen: &'a mut crate::models::specimen::Specimen) -> crate::error::AppResult<&'a mut crate::models::specimen::Specimen> {
        Repository::items_update_specimen(self, specimen).await
    }
    async fn items_delete_specimen(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::items_delete_specimen(self, id, force).await
    }
    async fn items_specimen_barcode_exists(&self, barcode: &str, exclude_specimen_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::items_specimen_barcode_exists(self, barcode, exclude_specimen_id).await
    }
    async fn items_get_specimen_by_barcode(&self, barcode: &str) -> crate::error::AppResult<Option<(i64, bool)>> {
        Repository::items_get_specimen_by_barcode(self, barcode).await
    }
    async fn items_reactivate_specimen(&self, specimen_id: i64, item_id: i64, specimen: &crate::models::specimen::Specimen) -> crate::error::AppResult<crate::models::specimen::Specimen> {
        Repository::items_reactivate_specimen(self, specimen_id, item_id, specimen).await
    }
    async fn items_find_active_by_isbn(&self, isbn: &str, exclude_id: Option<i64>) -> crate::error::AppResult<Option<i64>> {
        Repository::items_find_active_by_isbn(self, isbn, exclude_id).await
    }
    async fn items_find_specimen_short_by_barcode(&self, barcode: &str, exclude_specimen_id: Option<i64>) -> crate::error::AppResult<Option<crate::models::specimen::SpecimenShort>> {
        Repository::items_find_specimen_short_by_barcode(self, barcode, exclude_specimen_id).await
    }
    async fn items_find_by_isbn_for_import(&self, isbn: &str) -> crate::error::AppResult<Option<crate::models::import_report::DuplicateCandidate>> {
        Repository::items_find_by_isbn_for_import(self, isbn).await
    }
    async fn items_update_marc_record_for_item(&self, item: &mut crate::models::item::Item) -> crate::error::AppResult<()> {
        Repository::items_update_marc_record_for_item(self, item).await
    }
    async fn items_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::items_isbn_exists(self, isbn, exclude_id).await
    }
    async fn items_count_specimens_for_source(&self, source_id: i64) -> crate::error::AppResult<i64> {
        Repository::items_count_specimens_for_source(self, source_id).await
    }
    async fn items_reassign_specimens_source(&self, old_source_ids: &[i64], new_source_id: i64) -> crate::error::AppResult<i64> {
        Repository::items_reassign_specimens_source(self, old_source_ids, new_source_id).await
    }
    async fn items_reassign_items_source(&self, old_source_ids: &[i64], new_source_id: i64) -> crate::error::AppResult<i64> {
        Repository::items_reassign_items_source(self, old_source_ids, new_source_id).await
    }
}
