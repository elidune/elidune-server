//! Biblios repository — meili operations.

use crate::{
    error::AppResult,
    models::biblio::MeiliBiblioDocument,
};

use super::super::Repository;

impl Repository {
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
                b.dewey,
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
                (b.archived_at IS NOT NULL) AS is_archived,
                EXISTS (
                    SELECT 1 FROM items it_act
                    WHERE it_act.biblio_id = b.id AND it_act.archived_at IS NULL
                ) AS has_active_items
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

    /// Fetch a page of Meilisearch documents using a keyset cursor.
    /// Returns biblios with `id > after_id`, up to `limit` rows, ordered by id.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_meili_documents_batch(
        &self,
        after_id: i64,
        limit: i64,
    ) -> AppResult<Vec<MeiliBiblioDocument>> {
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
                b.dewey,
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
                (b.archived_at IS NOT NULL) AS is_archived,
                EXISTS (
                    SELECT 1 FROM items it_act
                    WHERE it_act.biblio_id = b.id AND it_act.archived_at IS NULL
                ) AS has_active_items
            FROM biblios b
            LEFT JOIN biblio_authors ba ON ba.biblio_id = b.id
            LEFT JOIN authors a ON a.id = ba.author_id
            LEFT JOIN editions ed ON ed.id = b.edition_id
            LEFT JOIN biblio_series bsx ON bsx.biblio_id = b.id
            LEFT JOIN series se ON se.id = bsx.series_id
            LEFT JOIN biblio_collections bcx ON bcx.biblio_id = b.id
            LEFT JOIN collections co ON co.id = bcx.collection_id
            LEFT JOIN items it ON it.biblio_id = b.id AND it.archived_at IS NULL
            WHERE b.id > $1
            GROUP BY b.id, ed.publisher_name
            ORDER BY b.id
            LIMIT $2
            "#,
        )
        .bind(after_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(docs)
    }
}
