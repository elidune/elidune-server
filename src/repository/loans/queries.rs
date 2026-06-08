//! Loans repository — read, list, export, and count queries.

use chrono::{DateTime, Utc};
use sqlx::Row;

use super::super::Repository;
use super::LOAN_DETAILS_FIRST_AUTHOR_SQL;
use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        author::Author,
        biblio::{Biblio, BiblioShort, Collection, Edition, Isbn, Serie},
        item::{Item, ItemShort},
        loan::{Loan, LoanDetails, LoanMarcExportRow},
    },
};

impl Repository {
    /// Get loan by ID
    pub async fn loans_get_by_id(&self, id: i64) -> AppResult<Loan> {
        sqlx::query_as::<_, Loan>("SELECT * FROM loans WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Loan with id {} not found", id)))
    }

    /// Get active loan by item identification (barcode)
    pub async fn loans_get_by_item_identification(
        &self,
        item_identification: &str,
    ) -> AppResult<Loan> {
        sqlx::query_as::<_, Loan>(
            r#"
            SELECT l.* FROM loans l
            JOIN items it ON l.item_id = it.id
            WHERE it.barcode = $1 AND l.returned_at IS NULL
            ORDER BY l.id DESC LIMIT 1
            "#,
        )
        .bind(item_identification)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "No active loan found for item {}",
                item_identification
            ))
        })
    }

    /// Get active loans for a user (paginated).
    pub async fn loans_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)> {
        let offset = (page - 1) * per_page;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM loans l WHERE l.user_id = $1 AND l.returned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let sql = format!(
            r#"
            SELECT l.id, l.date, l.renew_at, l.nb_renews, l.expiry_at,
                   l.returned_at,
                   it.barcode as item_identification,
                   it.id as item_copy_id, it.barcode as item_barcode,
                   it.call_number as item_call_number, it.borrowable as item_borrowable,
                   so.name as item_source_name,
                   b.id as biblio_id, b.media_type, b.isbn as biblio_isbn,
                   b.title, b.publication_date,
                   {}
            FROM loans l
            JOIN items it ON l.item_id = it.id
            LEFT JOIN sources so ON it.source_id = so.id
            JOIN biblios b ON it.biblio_id = b.id
            WHERE l.user_id = $1 AND l.returned_at IS NULL
            ORDER BY l.expiry_at
            LIMIT $2 OFFSET $3
        "#,
            LOAN_DETAILS_FIRST_AUTHOR_SQL
        );

        let rows = sqlx::query(&sql)
            .bind(user_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok((Self::map_loan_rows(rows), total))
    }

    /// Get archived (returned) loans for a user (paginated).
    pub async fn loans_archives_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)> {
        let offset = (page - 1) * per_page;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM loans_archives la WHERE la.user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let sql = format!(
            r#"
            SELECT la.id, la.date, NULL::timestamptz as renew_at, la.nb_renews,
                   la.expiry_at, la.returned_at,
                   it.barcode as item_identification,
                   it.id as item_copy_id, it.barcode as item_barcode,
                   it.call_number as item_call_number, it.borrowable as item_borrowable,
                   so.name as item_source_name,

                   b.id as biblio_id, b.media_type, b.isbn as biblio_isbn,
                   b.title, b.publication_date,
                   {}
            FROM loans_archives la
            JOIN items it ON la.item_id = it.id
            LEFT JOIN sources so ON it.source_id = so.id
            JOIN biblios b ON it.biblio_id = b.id
            WHERE la.user_id = $1
            ORDER BY la.returned_at DESC
            LIMIT $2 OFFSET $3
        "#,
            LOAN_DETAILS_FIRST_AUTHOR_SQL
        );

        let rows = sqlx::query(&sql)
            .bind(user_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok((Self::map_loan_rows(rows), total))
    }

    /// All loans for one user for MARC file export (no pagination): one round-trip with full [`Biblio`] per row.
    pub async fn loans_get_for_marc_export(
        &self,
        user_id: i64,
        archived: bool,
    ) -> AppResult<Vec<LoanMarcExportRow>> {
        const BIBLIO_MARC_EXPORT_SELECT: &str = r#"
            b.id AS biblio_id,
            b.media_type,
            b.isbn,
            b.publication_date,
            b.lang,
            b.lang_orig,
            b.title,
            b.subject,
            b.dewey,
            b.audience_type,
            b.page_extent,
            b.format,
            b.table_of_contents,
            b.accompanying_material,
            b.abstract AS "abstract_",
            b.notes,
            b.keywords,
            b.edition_id,
            b.is_valid,
            b.created_at AS biblio_created_at,
            b.updated_at AS biblio_updated_at,
            b.archived_at AS biblio_archived_at,
            b.marc_record,
            (SELECT COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', a.id::text,
                        'key', a.key,
                        'lastname', a.lastname,
                        'firstname', a.firstname,
                        'bio', a.bio,
                        'notes', a.notes,
                        'function', ba.function
                    ) ORDER BY ba.position
                ),
                '[]'::jsonb
            )
            FROM biblio_authors ba
            JOIN authors a ON a.id = ba.author_id
            WHERE ba.biblio_id = b.id) AS authors_json,
            (SELECT COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', s.id::text,
                        'key', s.key,
                        'name', s.name,
                        'issn', s.issn,
                        'createdAt', to_jsonb(s.created_at),
                        'updatedAt', to_jsonb(s.updated_at),
                        'volumeNumber', to_jsonb(bsx.volume_number)
                    ) ORDER BY bsx.position
                ),
                '[]'::jsonb
            )
            FROM biblio_series bsx
            INNER JOIN series s ON s.id = bsx.series_id
            WHERE bsx.biblio_id = b.id) AS series_json,
            (SELECT COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', c.id::text,
                        'key', c.key,
                        'name', c.name,
                        'secondaryTitle', c.secondary_title,
                        'tertiaryTitle', c.tertiary_title,
                        'issn', c.issn,
                        'createdAt', to_jsonb(c.created_at),
                        'updatedAt', to_jsonb(c.updated_at),
                        'volumeNumber', to_jsonb(bcx.volume_number)
                    ) ORDER BY bcx.position
                ),
                '[]'::jsonb
            )
            FROM biblio_collections bcx
            INNER JOIN collections c ON c.id = bcx.collection_id
            WHERE bcx.biblio_id = b.id) AS collections_json,
            (SELECT jsonb_build_object(
                'id', e.id::text,
                'publisherName', e.publisher_name,
                'placeOfPublication', e.place_of_publication,
                'date', e.date,
                'createdAt', to_jsonb(e.created_at),
                'updatedAt', to_jsonb(e.updated_at)
            )
            FROM editions e WHERE e.id = b.edition_id) AS edition_json,
            it.id AS item_id,
            it.biblio_id AS item_biblio_id,
            it.source_id AS item_source_id,
            it.barcode AS item_barcode,
            it.call_number AS item_call_number,
            it.volume_designation AS item_volume_designation,
            it.place AS item_place,
            it.borrowable AS item_borrowable,
            it.circulation_status AS item_circulation_status,
            it.notes AS item_notes,
            it.price AS item_price,
            it.created_at AS item_created_at,
            it.updated_at AS item_updated_at,
            it.archived_at AS item_archived_at,
            so.name AS item_source_name,
            EXISTS(
                SELECT 1 FROM loans ln WHERE ln.item_id = it.id AND ln.returned_at IS NULL
            ) AS item_borrowed
        "#;

        let sql = if archived {
            format!(
                r#"
                SELECT
                    la.date AS start_date,
                    la.expiry_at,
                    la.returned_at,
                    {BIBLIO_MARC_EXPORT_SELECT}
                FROM loans_archives la
                JOIN items it ON la.item_id = it.id
                LEFT JOIN sources so ON it.source_id = so.id
                JOIN biblios b ON it.biblio_id = b.id
                WHERE la.user_id = $1
                ORDER BY la.returned_at DESC NULLS LAST
                "#
            )
        } else {
            format!(
                r#"
                SELECT
                    l.date AS start_date,
                    l.expiry_at,
                    l.returned_at,
                    {BIBLIO_MARC_EXPORT_SELECT}
                FROM loans l
                JOIN items it ON l.item_id = it.id
                LEFT JOIN sources so ON it.source_id = so.id
                JOIN biblios b ON it.biblio_id = b.id
                WHERE l.user_id = $1
                  AND l.returned_at IS NULL
                  AND b.archived_at IS NULL
                  AND it.archived_at IS NULL
                ORDER BY l.expiry_at ASC NULLS LAST
                "#
            )
        };

        let rows = sqlx::query(&sql).bind(user_id).fetch_all(&self.pool).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(Self::loan_marc_export_row_from_pg(row)?);
        }
        Ok(out)
    }

    fn loan_marc_export_row_from_pg(row: sqlx::postgres::PgRow) -> AppResult<LoanMarcExportRow> {
        let start_date: DateTime<Utc> = row
            .try_get("start_date")
            .map_err(|e| AppError::Internal(format!("marc export row start_date: {}", e)))?;
        let expiry_at: Option<DateTime<Utc>> = row
            .try_get("expiry_at")
            .map_err(|e| AppError::Internal(format!("marc export row expiry_at: {}", e)))?;
        let returned_at: Option<DateTime<Utc>> = row
            .try_get("returned_at")
            .map_err(|e| AppError::Internal(format!("marc export row returned_at: {}", e)))?;

        let authors: Vec<Author> = {
            let v: serde_json::Value = row
                .try_get("authors_json")
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::from_value(v)
                .map_err(|e| AppError::Internal(format!("marc export authors_json: {}", e)))?
        };
        let series: Vec<Serie> = {
            let v: serde_json::Value = row
                .try_get("series_json")
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::from_value(v)
                .map_err(|e| AppError::Internal(format!("marc export series_json: {}", e)))?
        };
        let collections: Vec<Collection> = {
            let v: serde_json::Value = row
                .try_get("collections_json")
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::from_value(v)
                .map_err(|e| AppError::Internal(format!("marc export collections_json: {}", e)))?
        };

        let edition: Option<Edition> = {
            let v: Option<serde_json::Value> = row.try_get("edition_json").ok().flatten();
            match v {
                None => None,
                Some(ref x) if x.is_null() => None,
                Some(v) => Some(serde_json::from_value(v).map_err(|e| {
                    AppError::Internal(format!("marc export edition_json: {}", e))
                })?),
            }
        };

        let isbn_raw: Option<String> = row.try_get("isbn").ok().flatten();
        let isbn = isbn_raw.and_then(|s| {
            let i = Isbn::new(s);
            if i.is_empty() {
                None
            } else {
                Some(i)
            }
        });

        let marc_record: Option<MarcRecord> = {
            let v: Option<serde_json::Value> = row.try_get("marc_record").ok().flatten();
            match v {
                None => None,
                Some(ref x) if x.is_null() => None,
                Some(v) => Some(serde_json::from_value(v).map_err(|e| {
                    AppError::Internal(format!("marc export marc_record: {}", e))
                })?),
            }
        };

        let item = Item {
            id: row.try_get("item_id").ok(),
            biblio_id: row.try_get("item_biblio_id").ok(),
            source_id: row.try_get("item_source_id").ok(),
            barcode: row.try_get("item_barcode").ok().flatten(),
            call_number: row.try_get("item_call_number").ok().flatten(),
            volume_designation: row.try_get("item_volume_designation").ok().flatten(),
            place: row.try_get("item_place").ok().flatten(),
            borrowable: row.try_get("item_borrowable").unwrap_or(true),
            circulation_status: row.try_get("item_circulation_status").ok().flatten(),
            notes: row.try_get("item_notes").ok().flatten(),
            price: row.try_get("item_price").ok().flatten(),
            created_at: row.try_get("item_created_at").ok().flatten(),
            updated_at: row.try_get("item_updated_at").ok().flatten(),
            archived_at: row.try_get("item_archived_at").ok().flatten(),
            source_name: row.try_get("item_source_name").ok().flatten(),
            borrowed: row.try_get("item_borrowed").unwrap_or(false),
        };

        let series_ids: Vec<i64> = series.iter().filter_map(|s| s.id).collect();
        let series_volume_numbers: Vec<Option<i16>> =
            series.iter().map(|s| s.volume_number).collect();
        let collection_ids: Vec<i64> = collections.iter().filter_map(|c| c.id).collect();
        let collection_volume_numbers: Vec<Option<i16>> =
            collections.iter().map(|c| c.volume_number).collect();

        let biblio = Biblio {
            id: row.try_get("biblio_id").ok(),
            media_type: row
                .try_get("media_type")
                .map_err(|e| AppError::Internal(format!("marc export media_type: {}", e)))?,
            isbn,
            title: row.try_get("title").ok().flatten(),
            subject: row.try_get("subject").ok().flatten(),
            dewey: row.try_get("dewey").ok().flatten(),
            audience_type: row.try_get("audience_type").ok().flatten(),
            lang: row.try_get("lang").ok().flatten(),
            lang_orig: row.try_get("lang_orig").ok().flatten(),
            publication_date: row.try_get("publication_date").ok().flatten(),
            page_extent: row.try_get("page_extent").ok().flatten(),
            format: row.try_get("format").ok().flatten(),
            table_of_contents: row.try_get("table_of_contents").ok().flatten(),
            accompanying_material: row.try_get("accompanying_material").ok().flatten(),
            abstract_: row.try_get("abstract_").ok().flatten(),
            notes: row.try_get("notes").ok().flatten(),
            keywords: row.try_get("keywords").ok().flatten(),
            is_valid: row.try_get("is_valid").ok().flatten(),
            series_ids,
            series_volume_numbers,
            edition_id: row.try_get("edition_id").ok().flatten(),
            collection_ids,
            collection_volume_numbers,
            created_at: row.try_get("biblio_created_at").ok().flatten(),
            updated_at: row.try_get("biblio_updated_at").ok().flatten(),
            archived_at: row.try_get("biblio_archived_at").ok().flatten(),
            authors,
            series,
            collections,
            edition,
            items: vec![item],
            marc_record,
        };

        Ok(LoanMarcExportRow {
            start_date,
            expiry_at: expiry_at.unwrap_or(start_date),
            returned_at,
            biblio,
        })
    }

    fn map_loan_rows(rows: Vec<sqlx::postgres::PgRow>) -> Vec<LoanDetails> {
        let now = Utc::now();
        rows.into_iter()
            .map(|row| {
                let start_date: DateTime<Utc> = row.get("date");
                let expiry_at: Option<DateTime<Utc>> = row.get("expiry_at");
                let renew_at: Option<DateTime<Utc>> = row.get("renew_at");
                let returned_at: Option<DateTime<Utc>> = row.get("returned_at");

                let borrowed_item = ItemShort {
                    id: row.get("item_copy_id"),
                    barcode: row.get("item_barcode"),
                    call_number: row.get("item_call_number"),
                    borrowable: row.get("item_borrowable"),
                    source_name: row.get("item_source_name"),
                    borrowed: true,
                };

                LoanDetails {
                    id: row.get("id"),
                    item_id: row.get("item_copy_id"),
                    start_date,
                    expiry_at: expiry_at.unwrap_or(now),
                    renewal_date: renew_at,
                    nb_renews: row.get::<Option<i16>, _>("nb_renews").unwrap_or(0),
                    returned_at,
                    biblio: BiblioShort {
                        id: row.get("biblio_id"),
                        media_type: row.get("media_type"),
                        isbn: row
                            .get::<Option<String>, _>("biblio_isbn")
                            .map(Isbn::new)
                            .filter(|i| !i.is_empty()),
                        title: row.get("title"),
                        date: row.get("publication_date"),
                        status: 0,
                        is_valid: Some(true),
                        archived_at: None,
                        author: row
                            .get::<Option<serde_json::Value>, _>("author")
                            .and_then(|v| serde_json::from_value(v).ok()),
                        items: vec![borrowed_item],
                    },
                    user: None,
                    item_identification: row.get("item_identification"),
                    is_overdue: returned_at.is_none()
                        && expiry_at.map(|d| d < now).unwrap_or(false),
                }
            })
            .collect()
    }

    /// Count active loans
    pub async fn loans_count_active(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loans WHERE returned_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Count overdue loans
    pub async fn loans_count_overdue(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND expiry_at < NOW()",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Count active loans for a physical item (items table)
    pub async fn loans_count_active_for_item(&self, item_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE item_id = $1 AND returned_at IS NULL",
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Get IDs of active loans for a physical item
    pub async fn loans_get_active_ids_for_item(&self, item_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM loans WHERE item_id = $1 AND returned_at IS NULL",
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Get IDs of active loans for a biblio (via its physical items)
    pub async fn loans_get_active_ids_for_biblio(&self, biblio_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            r#"
            SELECT l.id FROM loans l
            JOIN items it ON l.item_id = it.id
            WHERE it.biblio_id = $1 AND l.returned_at IS NULL
            "#,
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Get IDs of active loans for a user
    pub async fn loans_get_active_ids_for_user(&self, user_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM loans WHERE user_id = $1 AND returned_at IS NULL",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Count active loans for a biblio (via its physical items)
    pub async fn loans_count_active_for_biblio(&self, biblio_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM loans l
            JOIN items it ON l.item_id = it.id
            WHERE it.biblio_id = $1 AND l.returned_at IS NULL
            "#,
        )
        .bind(biblio_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Count active loans for a user
    pub async fn loans_count_active_for_user(&self, user_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE user_id = $1 AND returned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }
}
