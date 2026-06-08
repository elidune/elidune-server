//! Biblios repository — search operations.

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
use super::{BiblioShortRow, like_escape};

impl Repository {
    /// are handled here. When `include_without_active_items` is false/absent, only biblios
    /// with at least one non-archived linked item are returned.
    /// When freesearch is present, the CatalogService routes through
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

        if !query.include_without_active_items.unwrap_or(false) {
            where_parts.push(
                "EXISTS (SELECT 1 FROM items i WHERE i.biblio_id = b.id AND i.archived_at IS NULL)"
                    .to_string(),
            );
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
            is_valid: Option<bool>,
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
}
