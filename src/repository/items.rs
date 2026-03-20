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
        import_report::DuplicateCandidate,
        item::{Collection, Edition, Isbn, Item, ItemQuery, ItemShort, MediaType, Serie},
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

// =============================================================================
// Google-like query parser → PostgreSQL tsquery
// =============================================================================

/// A parsed search token from the freesearch input string.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SearchToken {
    And(Vec<SearchToken>),
    Or(Vec<SearchToken>),
    /// Single word/prefix term. `prefix = true` appends `:*` for prefix matching.
    Term { word: String, prefix: bool },
    /// Quoted phrase: words must appear in order (tsquery `<->` adjacency).
    Phrase(Vec<String>),
    /// Negation: the inner token must NOT match.
    Not(Box<SearchToken>),
}

/// Parse a freesearch string into a [`SearchToken`] tree.
///
/// Supported syntax (mirrors Google search):
/// - `word`           → prefix term (AND with others)
/// - `"exact phrase"` → phrase match
/// - `-word`          → exclude term
/// - `-"phrase"`      → exclude phrase
/// - `word OR word`   → OR group
/// - `+word`          → required (same as bare AND, explicit)
fn parse_search_query(input: &str) -> Option<SearchToken> {
    let tokens = tokenize_search_input(input);
    if tokens.is_empty() {
        return None;
    }
    Some(build_token_tree(tokens))
}

#[derive(Debug, Clone)]
enum RawToken {
    Word(String),
    Phrase(String),
    Negate,
    OrKeyword,
}

fn tokenize_search_input(input: &str) -> Vec<RawToken> {
    let mut out = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
            }
            '"' => {
                chars.next();
                let mut phrase = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    phrase.push(c);
                }
                let trimmed = phrase.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(RawToken::Phrase(trimmed));
                }
            }
            '-' => {
                chars.next();
                out.push(RawToken::Negate);
            }
            '+' => {
                chars.next();
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '"' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                if !word.is_empty() {
                    if word.eq_ignore_ascii_case("OR") {
                        out.push(RawToken::OrKeyword);
                    } else {
                        out.push(RawToken::Word(word));
                    }
                }
            }
        }
    }
    out
}

fn build_token_tree(raw: Vec<RawToken>) -> SearchToken {
    let mut or_groups: Vec<Vec<SearchToken>> = vec![Vec::new()];
    let mut negate_next = false;

    for tok in raw {
        match tok {
            RawToken::Negate => {
                negate_next = true;
            }
            RawToken::OrKeyword => {
                or_groups.push(Vec::new());
                negate_next = false;
            }
            RawToken::Word(w) => {
                let term = SearchToken::Term {
                    word: tsquery_sanitize(&w),
                    prefix: true,
                };
                let node = if negate_next {
                    SearchToken::Not(Box::new(term))
                } else {
                    term
                };
                negate_next = false;
                or_groups.last_mut().unwrap().push(node);
            }
            RawToken::Phrase(p) => {
                let words: Vec<String> = p
                    .split_whitespace()
                    .map(|s| tsquery_sanitize(s))
                    .filter(|s| !s.is_empty())
                    .collect();
                if words.is_empty() {
                    negate_next = false;
                    continue;
                }
                let phrase = SearchToken::Phrase(words);
                let node = if negate_next {
                    SearchToken::Not(Box::new(phrase))
                } else {
                    phrase
                };
                negate_next = false;
                or_groups.last_mut().unwrap().push(node);
            }
        }
    }

    let or_parts: Vec<SearchToken> = or_groups
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(|mut g| {
            if g.len() == 1 {
                g.remove(0)
            } else {
                SearchToken::And(g)
            }
        })
        .collect();

    match or_parts.len() {
        0 => SearchToken::And(vec![]),
        1 => or_parts.into_iter().next().unwrap(),
        _ => SearchToken::Or(or_parts),
    }
}

/// Render a [`SearchToken`] tree to a PostgreSQL tsquery string.
fn render_tsquery(token: &SearchToken) -> Option<String> {
    match token {
        SearchToken::And(children) => {
            let parts: Vec<String> = children.iter().filter_map(render_tsquery).collect();
            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.into_iter().next().unwrap())
            } else {
                Some(parts.join(" & "))
            }
        }
        SearchToken::Or(children) => {
            let parts: Vec<String> = children.iter().filter_map(render_tsquery).collect();
            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.into_iter().next().unwrap())
            } else {
                Some(format!("({})", parts.join(" | ")))
            }
        }
        SearchToken::Term { word, prefix } => {
            if word.is_empty() {
                return None;
            }
            if *prefix {
                Some(format!("{}:*", word))
            } else {
                Some(word.clone())
            }
        }
        SearchToken::Phrase(words) => {
            let non_empty: Vec<&String> = words.iter().filter(|w| !w.is_empty()).collect();
            if non_empty.is_empty() {
                return None;
            }
            if non_empty.len() == 1 {
                return Some(format!("{}:*", non_empty[0]));
            }
            Some(non_empty.iter().map(|w| w.as_str()).collect::<Vec<_>>().join(" <-> "))
        }
        SearchToken::Not(inner) => {
            render_tsquery(inner).map(|s| {
                if s.contains(' ') {
                    format!("!({})", s)
                } else {
                    format!("!{}", s)
                }
            })
        }
    }
}

/// Sanitize a word for tsquery: keep only alphanumeric chars, lowercase.
/// Words are left UNQUOTED so `to_tsquery` normalizes them through the
/// text-search dictionary (lowercase + unaccent).
fn tsquery_sanitize(w: &str) -> String {
    w.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Parse a freesearch string and return the final tsquery expression,
/// or `None` if the input produces no usable terms.
pub fn freesearch_to_tsquery(input: &str) -> Option<String> {
    let token = parse_search_query(input.trim())?;
    render_tsquery(&token)
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
    pub async fn items_get_by_id_or_isbn(&self, id_or_isbn: &str) -> AppResult<Item> {

        let query = r#"
            SELECT id, media_type, isbn,
                   publication_date, lang, lang_orig, title,
                   subject, audience_type, page_extent, format,
                   table_of_contents, accompanying_material,
                   abstract as abstract_, notes, keywords, 
                   series_id, series_volume_number, edition_id,
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

        item.series = sqlx::query_as::<_, Serie>(
            "SELECT id, key, name, issn, created_at, updated_at FROM series WHERE id = $1",
        )
        .bind(item.series_id)
        .fetch_optional(&self.pool)
        .await?;

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
            SELECT a.id, a.lastname, a.firstname, a.bio, a.notes, ia.role as function
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
                function: r.get::<Option<String>, _>("function"),
            })
            .collect())
    }

    /// Get a short item representation by ID (includes author + specimens).
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
                           'function', ia.role
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

    /// Search items with full-text search (FTS via tsvector) and parameterized
    /// field filters. All ItemQuery fields are handled. Results are ranked by
    /// ts_rank when a freesearch query is present, otherwise ordered by title.
    pub async fn items_search(&self, query: &ItemQuery) -> AppResult<(Vec<ItemShort>, i64)> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(20).clamp(1, 200);
        let offset = (page - 1) * per_page;

        let tsquery_str = query
            .freesearch
            .as_deref()
            .and_then(freesearch_to_tsquery);

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
        if let Some(at) = query.audience_type {
            params.push(Param::I16(at));
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

        // freesearch → FTS on search_vector
        let fts_param_idx: Option<usize> = if let Some(ref tsq) = tsquery_str {
            params.push(Param::Text(tsq.clone()));
            let idx = params.len();
            where_parts.push(format!(
                "i.search_vector @@ to_tsquery('simple_unaccent', ${})",
                idx
            ));
            Some(idx)
        } else {
            None
        };

        let where_sql = if where_parts.is_empty() {
            "1=1".to_string()
        } else {
            where_parts.join(" AND ")
        };

        let order_sql = if let Some(fts_idx) = fts_param_idx {
            format!(
                "ts_rank(i.search_vector, to_tsquery('simple_unaccent', ${})) DESC, i.title ASC NULLS LAST",
                fts_idx
            )
        } else {
            "i.title ASC NULLS LAST".to_string()
        };

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
                           'function', ia.role
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
                           'function', ia.role
                       )
                       FROM item_authors ia
                       JOIN authors a ON a.id = ia.author_id
                       WHERE ia.item_id = i.id
                       ORDER BY ia.position LIMIT 1
                   ) as author
            FROM items i
            WHERE i.series_id = $1 AND i.archived_at IS NULL
            ORDER BY i.series_volume_number, i.title
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
    // CREATE
    // =========================================================================

    /// Create a new item
    pub async fn items_create<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let now = Utc::now();

        item.updated_at = Some(now);
        item.created_at = Some(now);
        item.series_id = self.process_serie(&item.series).await?;
        item.collection_id = self.process_collection(&item.collection).await?;
        item.edition_id = self.process_edition(&item.edition).await?;

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO items (
                media_type, isbn, publication_date,
                lang, lang_orig, title, subject,
                audience_type, page_extent, format, table_of_contents, accompanying_material,
                abstract, notes, keywords, is_valid,
                series_id, series_volume_number,
                collection_id, collection_sequence_number, collection_volume_number,
                edition_id, created_at, updated_at
            ) VALUES (
                $1, $2, $3,
                $4, $5, $6, $7,
                $8, $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18,
                $19, $20, $21,
                $22, $23, $24
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
        .bind(&item.series_id)
        .bind(&item.series_volume_number)
        .bind(&item.collection_id)
        .bind(&item.collection_sequence_number)
        .bind(&item.collection_volume_number)
        .bind(&item.edition_id)
        .bind(&item.created_at)
        .bind(&item.updated_at)
        .fetch_one(&self.pool)
        .await?;

        item.id = Some(id);
        self.sync_item_authors(id, &item.authors).await?;
        self.items_update_marc_record_for_item(item).await?;

        Ok(item)
    }

    // =========================================================================
    // UPDATE
    // =========================================================================

    /// Update an existing item
    pub async fn items_update<'a>(&self, id: i64, item: &'a mut Item) -> AppResult<&'a mut Item> {

        item.updated_at = Some(Utc::now());
        item.series_id = self.process_serie(&item.series).await?;
        item.collection_id = self.process_collection(&item.collection).await?;
        item.edition_id = self.process_edition(&item.edition).await?;
        item.id = Some(id);

        sqlx::query(
            r#"
            UPDATE items SET
                media_type = COALESCE($1::text, media_type),    
                isbn = COALESCE($2::text, isbn),
                title = COALESCE($3::text, title),
                series_id = $4,
                series_volume_number = $5,
                collection_id = $6,
                collection_sequence_number = $7,
                collection_volume_number = $8,
                edition_id = $9,
                updated_at = $10
            WHERE id = $11
            "#,
        )
        .bind(&item.media_type)
        .bind(&item.isbn.as_ref().map(|i| i.to_string()))
        .bind(&item.title)
        .bind(&item.series_id)
        .bind(&item.series_volume_number)
        .bind(&item.collection_id)
        .bind(&item.collection_sequence_number)
        .bind(&item.collection_volume_number)
        .bind(&item.edition_id)
        .bind(&item.updated_at)
        .bind(id)
        .execute(&self.pool)
        .await?;

        if !item.authors.is_empty() {
            self.sync_item_authors(id, &item.authors).await?;
        }

        self.items_update_marc_record_for_item(item).await?;

        Ok(item)
    }

 
    /// Update marc record for an item, then refresh the search_vector.
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
            "UPDATE items SET marc_record = $1, \
             search_vector = items_rebuild_search_vector($2) \
             WHERE id = $2",
        )
        .bind(serde_json::to_value(&item.marc_record).unwrap())
        .bind(item.id.unwrap_or(0))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Rebuild the search_vector for an item (called after specimen mutations).
    async fn items_rebuild_search_vector(&self, item_id: i64) -> AppResult<()> {
        sqlx::query(
            "UPDATE items SET search_vector = items_rebuild_search_vector($1) WHERE id = $1",
        )
        .bind(item_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // =========================================================================
    // DELETE (archive)
    // =========================================================================

    /// Delete an item (soft delete — sets archived_at)
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

    /// Replace all authors for an item: delete existing rows then insert new ones.
    async fn sync_item_authors(
        &self,
        item_id: i64,
        authors: &[Author],
    ) -> AppResult<()> {
        sqlx::query("DELETE FROM item_authors WHERE item_id = $1")
            .bind(item_id)
            .execute(&self.pool)
            .await?;

        for (idx, author) in authors.iter().enumerate() {
            let author_id = self.ensure_author(author).await?;
            let Some(author_id) = author_id else { continue };

            sqlx::query(
                r#"
                INSERT INTO item_authors (item_id, author_id, role, author_type, position)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (item_id, author_id, role) DO UPDATE SET position = $5
                "#,
            )
            .bind(item_id)
            .bind(author_id)
            .bind(&author.function)
            .bind(0i16) // personal by default
            .bind((idx + 1) as i16)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Insert author if new, or return existing id.
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
        self.items_rebuild_search_vector(item_id).await?;
        Ok(new_specimen)
    }

    /// Upsert a specimen
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

        if let Some(item_id) = specimen.item_id {
            self.items_rebuild_search_vector(item_id).await?;
        }

        Ok(specimen)
    }

    /// Delete a specimen (soft delete — sets archived_at)
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

        if let Some(iid) = item_id {
            self.items_rebuild_search_vector(iid).await?;
        }

        Ok(())
    }

    /// Check if specimen barcode already exists
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

#[cfg(test)]
mod search_parser_tests {
    use super::freesearch_to_tsquery;

    fn tsq(input: &str) -> String {
        freesearch_to_tsquery(input).unwrap_or_default()
    }

    #[test]
    fn empty_returns_none() {
        assert!(freesearch_to_tsquery("").is_none());
        assert!(freesearch_to_tsquery("   ").is_none());
    }

    #[test]
    fn single_word_becomes_prefix_term() {
        assert_eq!(tsq("harry"), "harry:*");
    }

    #[test]
    fn multiple_words_are_anded() {
        assert_eq!(tsq("harry potter"), "harry:* & potter:*");
    }

    #[test]
    fn multiple_words_lowercased() {
        assert_eq!(tsq("Stephen King"), "stephen:* & king:*");
    }

    #[test]
    fn quoted_phrase_uses_adjacency() {
        assert_eq!(tsq(r#""harry potter""#), "harry <-> potter");
    }

    #[test]
    fn single_word_phrase_is_prefix() {
        assert_eq!(tsq(r#""wizard""#), "wizard:*");
    }

    #[test]
    fn negation_prefix() {
        assert_eq!(tsq("-horror"), "!horror:*");
    }

    #[test]
    fn negation_phrase() {
        assert_eq!(tsq(r#"-"dark arts""#), "!(dark <-> arts)");
    }

    #[test]
    fn or_keyword() {
        assert_eq!(tsq("cats OR dogs"), "(cats:* | dogs:*)");
    }

    #[test]
    fn plus_is_same_as_bare_and() {
        assert_eq!(tsq("+harry +potter"), "harry:* & potter:*");
    }

    #[test]
    fn mixed_and_or() {
        assert_eq!(
            tsq("stephen king OR tolkien"),
            "(stephen:* & king:* | tolkien:*)"
        );
    }

    #[test]
    fn mixed_phrase_and_word() {
        assert_eq!(
            tsq(r#""harry potter" magic"#),
            "harry <-> potter & magic:*"
        );
    }

    #[test]
    fn special_chars_stripped_from_words() {
        assert_eq!(tsq("c++"), "c:*");
    }

    #[test]
    fn accented_chars_kept_for_dictionary() {
        // Accented chars are kept in the tsquery string; the PostgreSQL
        // dictionary (simple_unaccent) handles normalisation.
        assert_eq!(tsq("résumé"), "résumé:*");
    }

    #[test]
    fn apostrophe_stripped() {
        assert_eq!(tsq("l'amour"), "lamour:*");
    }
}

