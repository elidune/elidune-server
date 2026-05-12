//! Loans domain methods on Repository

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        author::Author,
        biblio::{Biblio, BiblioShort, Collection, Edition, Isbn, Serie},
        item::{Item, ItemShort},
        loan::{
            CreateLoan, Loan, LoanDetails, LoanMarcExportRow, LoanReturnOutcome, LoanSettings,
            LoanSettingsRenewAt,
        },
        user::{UserShort, UserShortRow},
    },
};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LoansRepository: Send + Sync {
    async fn loans_get_by_id(&self, id: i64) -> AppResult<Loan>;
    async fn loans_get_by_item_identification(&self, item_identification: &str) -> AppResult<Loan>;
    async fn loans_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)>;
    async fn loans_archives_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)>;
    /// All loans for MARC export (no pagination). Active or archived only.
    async fn loans_get_for_marc_export(
        &self,
        user_id: i64,
        archived: bool,
    ) -> AppResult<Vec<LoanMarcExportRow>>;
    async fn loans_create(&self, loan: &CreateLoan) -> AppResult<(i64, DateTime<Utc>)>;
    async fn loans_return(&self, loan_id: i64) -> AppResult<LoanReturnOutcome>;
    async fn loans_renew(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)>;
    async fn loans_get_settings(&self) -> AppResult<Vec<LoanSettings>>;
    async fn loans_count_active(&self) -> AppResult<i64>;
    async fn loans_count_overdue(&self) -> AppResult<i64>;
    async fn loans_count_active_for_item(&self, item_id: i64) -> AppResult<i64>;
    async fn loans_get_active_ids_for_item(&self, item_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_get_active_ids_for_biblio(&self, biblio_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_get_active_ids_for_user(&self, user_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_count_active_for_biblio(&self, biblio_id: i64) -> AppResult<i64>;
    async fn loans_count_active_for_user(&self, user_id: i64) -> AppResult<i64>;
    async fn loans_get_overdue_for_reminders(
        &self,
        frequency_days: u32,
    ) -> AppResult<Vec<OverdueLoanRow>>;
    async fn loans_get_overdue(
        &self,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<OverdueLoanRow>, i64)>;
    async fn loans_update_reminder_sent(&self, loan_ids: &[i64]) -> AppResult<()>;
    /// Upsert global loan rules (`loans_settings`). `media_type == None` updates the default row (`media_type` IS NULL).
    async fn loans_settings_upsert_row(
        &self,
        media_type: Option<String>,
        nb_max: i16,
        nb_renews: i16,
        duration: i16,
        renew_at: LoanSettingsRenewAt,
    ) -> AppResult<()>;
    async fn loans_settings_delete_rows(&self) -> AppResult<()>;
}



/// Combined repository trait used by [`crate::services::loans::LoansService`].
///
/// Implemented by the concrete [`Repository`] via blanket impl below.
pub trait LoansServiceRepository: LoansRepository + crate::repository::UsersRepository + Send + Sync {}

impl<T: LoansRepository + crate::repository::UsersRepository + Send + Sync> LoansServiceRepository for T {}

// ---------------------------------------------------------------------------
// Trait implementation — forwards to inherent methods above.
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl LoansRepository for Repository {
    async fn loans_get_by_id(&self, id: i64) -> crate::error::AppResult<Loan> {
        Repository::loans_get_by_id(self, id).await
    }
    async fn loans_get_by_item_identification(&self, identification: &str) -> crate::error::AppResult<Loan> {
        Repository::loans_get_by_item_identification(self, identification).await
    }
    async fn loans_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> crate::error::AppResult<(Vec<LoanDetails>, i64)> {
        Repository::loans_get_for_user(self, user_id, page, per_page).await
    }
    async fn loans_archives_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> crate::error::AppResult<(Vec<LoanDetails>, i64)> {
        Repository::loans_archives_get_for_user(self, user_id, page, per_page).await
    }
    async fn loans_get_for_marc_export(
        &self,
        user_id: i64,
        archived: bool,
    ) -> crate::error::AppResult<Vec<LoanMarcExportRow>> {
        Repository::loans_get_for_marc_export(self, user_id, archived).await
    }
    async fn loans_create(&self, loan: &CreateLoan) -> crate::error::AppResult<(i64, chrono::DateTime<chrono::Utc>)> {
        Repository::loans_create(self, loan).await
    }
    async fn loans_return(&self, loan_id: i64) -> crate::error::AppResult<LoanReturnOutcome> {
        Repository::loans_return(self, loan_id).await
    }
    async fn loans_renew(&self, loan_id: i64) -> crate::error::AppResult<(chrono::DateTime<chrono::Utc>, i16)> {
        Repository::loans_renew(self, loan_id).await
    }
    async fn loans_get_settings(&self) -> crate::error::AppResult<Vec<crate::models::loan::LoanSettings>> {
        Repository::loans_get_settings(self).await
    }
    async fn loans_count_active(&self) -> crate::error::AppResult<i64> {
        Repository::loans_count_active(self).await
    }
    async fn loans_count_overdue(&self) -> crate::error::AppResult<i64> {
        Repository::loans_count_overdue(self).await
    }
    async fn loans_count_active_for_item(&self, item_id: i64) -> crate::error::AppResult<i64> {
        Repository::loans_count_active_for_item(self, item_id).await
    }
    async fn loans_get_active_ids_for_item(&self, item_id: i64) -> crate::error::AppResult<Vec<i64>> {
        Repository::loans_get_active_ids_for_item(self, item_id).await
    }
    async fn loans_get_active_ids_for_biblio(&self, biblio_id: i64) -> crate::error::AppResult<Vec<i64>> {
        Repository::loans_get_active_ids_for_biblio(self, biblio_id).await
    }
    async fn loans_get_active_ids_for_user(&self, user_id: i64) -> crate::error::AppResult<Vec<i64>> {
        Repository::loans_get_active_ids_for_user(self, user_id).await
    }
    async fn loans_count_active_for_biblio(&self, biblio_id: i64) -> crate::error::AppResult<i64> {
        Repository::loans_count_active_for_biblio(self, biblio_id).await
    }
    async fn loans_count_active_for_user(&self, user_id: i64) -> crate::error::AppResult<i64> {
        Repository::loans_count_active_for_user(self, user_id).await
    }
    async fn loans_get_overdue_for_reminders(&self, frequency_days: u32) -> crate::error::AppResult<Vec<OverdueLoanRow>> {
        Repository::loans_get_overdue_for_reminders(self, frequency_days).await
    }
    async fn loans_get_overdue(&self, page: i64, per_page: i64) -> crate::error::AppResult<(Vec<OverdueLoanRow>, i64)> {
        Repository::loans_get_overdue(self, page, per_page).await
    }
    async fn loans_update_reminder_sent(&self, loan_ids: &[i64]) -> crate::error::AppResult<()> {
        Repository::loans_update_reminder_sent(self, loan_ids).await
    }
    async fn loans_settings_upsert_row(
        &self,
        media_type: Option<String>,
        nb_max: i16,
        nb_renews: i16,
        duration: i16,
        renew_at: LoanSettingsRenewAt,
    ) -> crate::error::AppResult<()> {
        Repository::loans_settings_upsert_row(
            self,
            media_type,
            nb_max,
            nb_renews,
            duration,
            renew_at,
        )
        .await
    }
    async fn loans_settings_delete_rows(&self) -> crate::error::AppResult<()> {
        Repository::loans_settings_delete_rows(self).await
    }
}

/// Scalar subquery (column alias `author`): first author on biblio `b` as JSON for [`BiblioShort`].
const LOAN_DETAILS_FIRST_AUTHOR_SQL: &str = r#"(SELECT jsonb_build_object(
                'id', a.id::text, 'lastname', a.lastname, 'firstname', a.firstname,
                'bio', a.bio, 'notes', a.notes, 'function', ba.function
            ) FROM biblio_authors ba JOIN authors a ON a.id = ba.author_id
            WHERE ba.biblio_id = b.id ORDER BY ba.position LIMIT 1) as author"#;

impl Repository {
    /// Resolve loan settings: (duration_days, nb_max_media, nb_max_total_all_media, nb_renews, renew_at_policy).
    ///
    /// `nb_max_total_all_media` comes from `nb_max` on the default rows (audience / global), not per-media rows.
    /// `nb_max_media` applies only to the current `media_type`; it does not use default rows' `nb_max`.
    async fn resolve_loan_settings(
        &self,
        user_public_type: Option<i64>,
        media_type: Option<&str>,
    ) -> AppResult<(i16, i16, i16, i16, LoanSettingsRenewAt)> {
        let default_duration = 21i16;
        let default_nb_max_media = 5i16;
        let default_nb_max_total = 5i16;
        let default_nb_renews = 2i16;

        let pick_renew =
            |row: Option<&sqlx::postgres::PgRow>| -> Option<LoanSettingsRenewAt> {
                row.and_then(|r| r.get::<Option<String>, _>("renew_at"))
                    .map(|s| LoanSettingsRenewAt::from(s.as_str()))
            };

        let ptls_spec = if let (Some(pt_id), Some(mt)) = (user_public_type, media_type) {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews, renew_at FROM public_type_loan_settings WHERE public_type_id = $1 AND media_type = $2",
            )
            .bind(pt_id)
            .bind(mt)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let ptls_default = if let Some(pt_id) = user_public_type {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews, renew_at FROM public_type_loan_settings WHERE public_type_id = $1 AND media_type IS NULL",
            )
            .bind(pt_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let ls_spec = if let Some(mt) = media_type {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews, renew_at FROM loans_settings WHERE media_type = $1",
            )
            .bind(mt)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let ls_default = sqlx::query(
            "SELECT duration, nb_max, nb_renews, renew_at FROM loans_settings WHERE media_type IS NULL",
        )
        .fetch_optional(&self.pool)
        .await?;

        let duration = ptls_spec
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("duration"))
            .or_else(|| ptls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .or_else(|| ls_spec.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .or_else(|| ls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .unwrap_or(default_duration);

        let nb_max_media = ptls_spec
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_max"))
            .or_else(|| ls_spec.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_max")))
            .unwrap_or(default_nb_max_media);

        let nb_max_total = ptls_default
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_max"))
            .or_else(|| ls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_max")))
            .unwrap_or(default_nb_max_total);

        let nb_renews = ptls_spec
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_renews"))
            .or_else(|| ptls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .or_else(|| ls_spec.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .or_else(|| ls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .unwrap_or(default_nb_renews);

        let renew_at_policy = pick_renew(ptls_spec.as_ref())
            .or_else(|| pick_renew(ptls_default.as_ref()))
            .or_else(|| pick_renew(ls_spec.as_ref()))
            .or_else(|| pick_renew(ls_default.as_ref()))
            .unwrap_or(LoanSettingsRenewAt::Now);

        Ok((duration, nb_max_media, nb_max_total, nb_renews, renew_at_policy))
    }

    /// Get loan by ID
    pub async fn loans_get_by_id(&self, id: i64) -> AppResult<Loan> {
        sqlx::query_as::<_, Loan>("SELECT * FROM loans WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Loan with id {} not found", id)))
    }

    /// Get active loan by item identification (barcode)
    pub async fn loans_get_by_item_identification(&self, item_identification: &str) -> AppResult<Loan> {
        sqlx::query_as::<_, Loan>(
            r#"
            SELECT l.* FROM loans l
            JOIN items it ON l.item_id = it.id
            WHERE it.barcode = $1 AND l.returned_at IS NULL
            ORDER BY l.id DESC LIMIT 1
            "#
        )
        .bind(item_identification)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("No active loan found for item {}", item_identification)))
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
        let start_date: DateTime<Utc> = row.try_get("start_date").map_err(|e| {
            AppError::Internal(format!("marc export row start_date: {}", e))
        })?;
        let expiry_at: Option<DateTime<Utc>> = row.try_get("expiry_at").map_err(|e| {
            AppError::Internal(format!("marc export row expiry_at: {}", e))
        })?;
        let returned_at: Option<DateTime<Utc>> = row.try_get("returned_at").map_err(|e| {
            AppError::Internal(format!("marc export row returned_at: {}", e))
        })?;

        let authors: Vec<Author> = {
            let v: serde_json::Value = row
                .try_get("authors_json")
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::from_value(v).map_err(|e| {
                AppError::Internal(format!("marc export authors_json: {}", e))
            })?
        };
        let series: Vec<Serie> = {
            let v: serde_json::Value = row
                .try_get("series_json")
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::from_value(v).map_err(|e| {
                AppError::Internal(format!("marc export series_json: {}", e))
            })?
        };
        let collections: Vec<Collection> = {
            let v: serde_json::Value = row
                .try_get("collections_json")
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::from_value(v).map_err(|e| {
                AppError::Internal(format!("marc export collections_json: {}", e))
            })?
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
            media_type: row.try_get("media_type").map_err(|e| {
                AppError::Internal(format!("marc export media_type: {}", e))
            })?,
            isbn,
            title: row.try_get("title").ok().flatten(),
            subject: row.try_get("subject").ok().flatten(),
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
        rows.into_iter().map(|row| {
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
                    author: row.get::<Option<serde_json::Value>, _>("author")
                        .and_then(|v| serde_json::from_value(v).ok()),
                    items: vec![borrowed_item],
                },
                user: None,
                item_identification: row.get("item_identification"),
                is_overdue: returned_at.is_none() && expiry_at.map(|d| d < now).unwrap_or(false),
            }
        }).collect()
    }

    /// Create a new loan
    pub async fn loans_create(&self, loan: &CreateLoan) -> AppResult<(i64, DateTime<Utc>)> {
        let now = Utc::now();

        // Get item (physical copy) ID
        let item_id = if let Some(id) = loan.item_id {
            id
        } else if let Some(ref identification) = loan.item_identification {
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM items WHERE barcode = $1"
            )
            .bind(identification)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
        } else {
            return Err(AppError::BadRequest("item_id or item_identification required".to_string()));
        };

        // Check if item is already borrowed
        let loan_id: Option<i64> = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM loans WHERE item_id = $1 AND returned_at IS NULL"
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(loan_id) = loan_id {
            if !loan.force {
                return Err(AppError::BusinessRule("Item is already borrowed".to_string()));
            } else {
                // return the loan
                self.loans_return(loan_id).await?;
            }
        }

        // Get item info and loan settings
        let item_row = sqlx::query(
            r#"
            SELECT it.borrowable, b.media_type
            FROM items it
            JOIN biblios b ON it.biblio_id = b.id
            WHERE it.id = $1
            "#
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;

        let borrowable: bool = item_row.get("borrowable");
        let media_type: Option<String> = item_row.get("media_type");

        if !borrowable && !loan.force {
            return Err(AppError::BusinessRule("Item is not borrowable".to_string()));
        }

        let user_public_type: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT public_type FROM users WHERE id = $1"
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        let (duration_days, nb_max_media, nb_max_total, _, _) = self
            .resolve_loan_settings(user_public_type, media_type.as_deref())
            .await?;

        let expiry_at = now + Duration::days(duration_days as i64);

        let current_loans_total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE user_id = $1 AND returned_at IS NULL"
        )
        .bind(loan.user_id)
        .fetch_one(&self.pool)
        .await?;
        let current_loans_media: i64 = if let Some(ref mt) = media_type {
            sqlx::query_scalar(
                r#"
                SELECT COUNT(*) FROM loans l
                JOIN items it ON l.item_id = it.id
                JOIN biblios b ON it.biblio_id = b.id
                WHERE l.user_id = $1 AND l.returned_at IS NULL AND b.media_type = $2
                "#
            )
            .bind(loan.user_id)
            .bind(mt)
            .fetch_one(&self.pool)
            .await?
        } else {
            0
        };

        let total_limit_reached = current_loans_total >= nb_max_total as i64;
        let media_limit_reached = current_loans_media >= nb_max_media as i64;

        if (total_limit_reached || media_limit_reached) && !loan.force {
            let msg = match (total_limit_reached, media_limit_reached) {
                (true, true) => format!(
                    "Maximum loans reached: total ({}/{}), this media type ({}/{})",
                    current_loans_total, nb_max_total, current_loans_media, nb_max_media
                ),
                (true, false) => format!(
                    "Maximum total loans reached ({}/{})",
                    current_loans_total, nb_max_total
                ),
                (false, true) => format!(
                    "Maximum loans for this document type reached ({}/{})",
                    current_loans_media, nb_max_media
                ),
                (false, false) => unreachable!(),
            };
            return Err(AppError::BusinessRule(msg));
        }

        // Hold queue: only the patron whose turn it is (`ready`, else first `pending`) may borrow,
        // unless staff uses `force=true` (clears active holds on this copy).
        if !loan.force {
            if let Some(eligible) = self.holds_eligible_borrower_for_item(item_id).await? {
                if eligible != loan.user_id {
                    return Err(AppError::BusinessRule(
                        "This copy has an active hold for another patron — only the queued patron may borrow it, or use force=true to override".to_string(),
                    ));
                }
            }
        }

        let mut tx = self.pool.begin().await?;

        let loan_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO loans (user_id, item_id, date, expiry_at, nb_renews)
            VALUES ($1, $2, $3, $4, 0)
            RETURNING id
            "#
        )
        .bind(loan.user_id)
        .bind(item_id)
        .bind(now)
        .bind(expiry_at)
        .fetch_one(&mut *tx)
        .await?;

        if loan.force {
            self.holds_cancel_active_for_item_tx(&mut tx, item_id).await?;
        } else {
            self.holds_fulfill_active_for_user_item_tx(&mut tx, loan.user_id, item_id)
                .await?;
        }

        tx.commit().await?;

        Ok((loan_id, expiry_at))
    }

    /// Return a loan (moves it to loans_archives).
    pub async fn loans_return(&self, loan_id: i64) -> AppResult<LoanReturnOutcome> {
        let now = Utc::now();

        let loan = self.loans_get_by_id(loan_id).await?;

        if loan.returned_at.is_some() {
            return Err(AppError::BusinessRule("Loan already returned".to_string()));
        }

        let user_row = sqlx::query(
            "SELECT addr_city, account_type, public_type FROM users WHERE id = $1"
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?;

        let account_type: Option<String> = user_row.as_ref().and_then(|r| r.get("account_type"));

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO loans_archives (
                user_id, item_id, date, nb_renews, expiry_at,
                returned_at, notes, borrower_public_type,
                addr_city, account_type
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(loan.user_id)
        .bind(loan.item_id)
        .bind(loan.date)
        .bind(loan.nb_renews)
        .bind(loan.expiry_at)
        .bind(now)
        .bind(&loan.notes)
        .bind(user_row.as_ref().and_then(|r| r.get::<Option<i64>, _>("public_type")))
        .bind(user_row.as_ref().and_then(|r| r.get::<Option<String>, _>("addr_city")))
        .bind(account_type)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM loans WHERE id = $1")
            .bind(loan_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        let readied_hold = match self
            .holds_notify_next(loan.item_id, self.hold_ready_expiry_days())
            .await
        {
            Ok(Some(res)) => {
                tracing::debug!(
                    target: "loans",
                    hold_id = res.id,
                    item_id = loan.item_id,
                    "Marked next pending hold as ready after loan return"
                );
                Some(res)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    target: "loans",
                    error = %e,
                    item_id = loan.item_id,
                    "Failed to advance hold queue after loan return"
                );
                None
            }
        };


        let biblio_row = sqlx::query(&format!(
            r#"
            SELECT b.id as biblio_id, b.media_type, b.isbn, b.title, b.publication_date,
                   it.barcode as item_identification,
                   it.id as item_copy_id, it.barcode as item_barcode,
                   it.call_number as item_call_number, it.borrowable as item_borrowable,
                   so.name as item_source_name,
                   {}
            FROM biblios b
            JOIN items it ON it.biblio_id = b.id
            LEFT JOIN sources so ON it.source_id = so.id
            WHERE it.id = $1
            "#,
            LOAN_DETAILS_FIRST_AUTHOR_SQL
        ))
        .bind(loan.item_id)
        .fetch_one(&self.pool)
        .await?;

        let user_short_row = sqlx::query_as::<_, UserShortRow>(
            r#"
            SELECT u.id, u.firstname, u.lastname, u.account_type, u.public_type,
                   u.status, u.created_at, u.expiry_at,
                   0::bigint as nb_loans, 0::bigint as nb_late_loans
            FROM users u
            WHERE u.id = $1
            "#
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?;

        let user: Option<UserShort> = user_short_row.map(|r| r.into());

        let item_short = ItemShort {
            id: biblio_row.get("item_copy_id"),
            barcode: biblio_row.get("item_barcode"),
            call_number: biblio_row.get("item_call_number"),
            borrowable: biblio_row.get("item_borrowable"),
            source_name: biblio_row.get("item_source_name"),
            borrowed: true,
        };

        let details = LoanDetails {
            id: loan.id,
            item_id: loan.item_id,
            start_date: loan.date,
            expiry_at: loan.expiry_at.unwrap_or(now),
            renewal_date: loan.renew_at,
            nb_renews: loan.nb_renews.unwrap_or(0),
            returned_at: Some(now),
            biblio: BiblioShort {
                id: biblio_row.get("biblio_id"),
                media_type: biblio_row.get("media_type"),
                isbn: biblio_row.get("isbn"),
                title: biblio_row.get("title"),
                date: biblio_row.get("publication_date"),
                status: 0,
                is_valid: Some(true),
                archived_at: None,
                author: biblio_row
                    .get::<Option<serde_json::Value>, _>("author")
                    .and_then(|v| serde_json::from_value(v).ok()),
                items: vec![item_short],
            },
            user,
            item_identification: biblio_row.get("item_identification"),
            is_overdue: false,
        };

        if let (Some(ref h), Some(ref email_svc)) = (&readied_hold, &self.email_service) {
            let contact = self
                .users_hold_ready_contact(h.user_id)
                .await
                .ok()
                .flatten();
            if let Err(e) =
                crate::hold_email::send_hold_ready(email_svc, contact, h, &details).await
            {
                tracing::warn!(
                    target: "loans",
                    error = %e,
                    hold_id = h.id,
                    "Failed to send hold ready email"
                );
            }
        }

        Ok(LoanReturnOutcome {
            details,
            readied_hold,
        })
    }

    /// Renew a loan
    pub async fn loans_renew(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)> {
        let now = Utc::now();

        let loan = self.loans_get_by_id(loan_id).await?;

        if loan.returned_at.is_some() {
            return Err(AppError::BusinessRule("Cannot renew a returned loan".to_string()));
        }

        let item_row = sqlx::query(
            "SELECT b.media_type FROM items it JOIN biblios b ON it.biblio_id = b.id WHERE it.id = $1"
        )
        .bind(loan.item_id)
        .fetch_one(&self.pool)
        .await?;

        let media_type: Option<String> = item_row.get("media_type");

        let user_public_type: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT public_type FROM users WHERE id = $1"
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        let (duration_days, _nb_max_media, _nb_max_total, max_renews, renew_at_policy) = self
            .resolve_loan_settings(user_public_type, media_type.as_deref())
            .await?;

        let current_renews = loan.nb_renews.unwrap_or(0);

        if current_renews >= max_renews {
            return Err(AppError::BusinessRule(format!(
                "Maximum renewals reached ({}/{})",
                current_renews, max_renews
            )));
        }

        let anchor = match renew_at_policy {
            LoanSettingsRenewAt::Now => now,
            LoanSettingsRenewAt::AtDueDate => loan.expiry_at.unwrap_or(now),
        };
        let new_expiry_date = anchor + Duration::days(duration_days as i64);
        let new_renews = current_renews + 1;

        sqlx::query(
            "UPDATE loans SET expiry_at = $1, renew_at = $2, nb_renews = $3 WHERE id = $4"
        )
        .bind(new_expiry_date)
        .bind(now)
        .bind(new_renews)
        .bind(loan_id)
        .execute(&self.pool)
        .await?;

        Ok((new_expiry_date, new_renews))
    }

    /// Get loan settings
    pub async fn loans_get_settings(&self) -> AppResult<Vec<LoanSettings>> {
        sqlx::query_as::<_, LoanSettings>(
            r#"SELECT * FROM loans_settings ORDER BY (media_type IS NOT NULL), media_type"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Delete all rows in `loans_settings`.
    pub async fn loans_settings_delete_rows(&self) -> AppResult<()> {
        sqlx::query("DELETE FROM loans_settings").execute(&self.pool).await?;
        Ok(())
    }

    /// Upsert one row in `loans_settings`. `media_type == None` is the global default row (`media_type` IS NULL).
    pub async fn loans_settings_upsert_row(
        &self,
        media_type: Option<String>,
        nb_max: i16,
        nb_renews: i16,
        duration: i16,
        renew_at: LoanSettingsRenewAt,
    ) -> AppResult<()> {
        let rows_affected = if let Some(ref mt) = media_type {
            sqlx::query(
                r#"
                UPDATE loans_settings
                SET nb_max = $2, nb_renews = $3, duration = $4, renew_at = $5
                WHERE media_type = $1
                "#,
            )
            .bind(mt.as_str())
            .bind(nb_max)
            .bind(nb_renews)
            .bind(duration)
            .bind(renew_at)
            .execute(&self.pool)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                r#"
                UPDATE loans_settings
                SET nb_max = $1, nb_renews = $2, duration = $3, renew_at = $4
                WHERE media_type IS NULL
                "#,
            )
            .bind(nb_max)
            .bind(nb_renews)
            .bind(duration)
            .bind(renew_at)
            .execute(&self.pool)
            .await?
            .rows_affected()
        };

        if rows_affected == 0 {
            if let Some(mt) = media_type {
                sqlx::query(
                    r#"
                    INSERT INTO loans_settings (media_type, nb_max, nb_renews, duration, renew_at)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                )
                .bind(mt)
                .bind(nb_max)
                .bind(nb_renews)
                .bind(duration)
                .bind(renew_at)
                .execute(&self.pool)
                .await?;
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO loans_settings (media_type, nb_max, nb_renews, duration, renew_at)
                    VALUES (NULL, $1, $2, $3, $4)
                    "#,
                )
                .bind(nb_max)
                .bind(nb_renews)
                .bind(duration)
                .bind(renew_at)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
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
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND expiry_at < NOW()"
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Count active loans for a physical item (items table)
    pub async fn loans_count_active_for_item(&self, item_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE item_id = $1 AND returned_at IS NULL"
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Get IDs of active loans for a physical item
    pub async fn loans_get_active_ids_for_item(&self, item_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM loans WHERE item_id = $1 AND returned_at IS NULL"
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
            "#
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Get IDs of active loans for a user
    pub async fn loans_get_active_ids_for_user(&self, user_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM loans WHERE user_id = $1 AND returned_at IS NULL"
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
            "#
        )
        .bind(biblio_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Count active loans for a user
    pub async fn loans_count_active_for_user(&self, user_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE user_id = $1 AND returned_at IS NULL"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Get overdue loans eligible for reminder emails.
    pub async fn loans_get_overdue_for_reminders(
        &self,
        frequency_days: u32,
    ) -> AppResult<Vec<OverdueLoanRow>> {
        let rows = sqlx::query(
            r#"
            SELECT
                l.id as loan_id,
                l.user_id,
                l.date as loan_date,
                l.expiry_at,
                l.last_reminder_sent_at,
                l.reminder_count,
                u.firstname,
                u.lastname,
                u.email as user_email,
                u.language as user_language,
                b.id as biblio_id,
                b.title,
                (
                    SELECT string_agg(a.lastname || ' ' || COALESCE(a.firstname, ''), ', ' ORDER BY ba.position)
                    FROM biblio_authors ba
                    JOIN authors a ON a.id = ba.author_id
                    WHERE ba.biblio_id = b.id
                ) as authors,
                it.barcode as item_barcode
            FROM loans l
            JOIN items it ON l.item_id = it.id
            JOIN biblios b ON it.biblio_id = b.id
            JOIN users u ON l.user_id = u.id
            WHERE l.returned_at IS NULL
              AND l.expiry_at < NOW()
              AND (
                  l.last_reminder_sent_at IS NULL
                  OR l.last_reminder_sent_at < NOW() - ($1 || ' days')::INTERVAL
              )
              AND u.email IS NOT NULL
              AND u.email != ''
              AND u.receive_reminders = TRUE
            ORDER BY u.id, l.expiry_at
            "#,
        )
        .bind(frequency_days as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| OverdueLoanRow {
                loan_id: row.get("loan_id"),
                user_id: row.get("user_id"),
                loan_date: row.get("loan_date"),
                expiry_at: row.get("expiry_at"),
                last_reminder_sent_at: row.get("last_reminder_sent_at"),
                reminder_count: row.get::<Option<i32>, _>("reminder_count").unwrap_or(0),
                firstname: row.get("firstname"),
                lastname: row.get("lastname"),
                user_email: row.get("user_email"),
                user_language: row.get::<Option<String>, _>("user_language"),
                biblio_id: row.get("biblio_id"),
                title: row.get("title"),
                authors: row.get("authors"),
                item_barcode: row.get("item_barcode"),
            })
            .collect())
    }

    /// Get all overdue loans for the admin dashboard (paginated).
    pub async fn loans_get_overdue(
        &self,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<OverdueLoanRow>, i64)> {
        let offset = (page - 1) * per_page;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND expiry_at < NOW()"
        )
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query(
            r#"
            SELECT
                l.id as loan_id,
                l.user_id,
                l.date as loan_date,
                l.expiry_at,
                l.last_reminder_sent_at,
                l.reminder_count,
                u.firstname,
                u.lastname,
                u.email as user_email,
                u.language as user_language,
                b.id as biblio_id,
                b.title,
                (
                    SELECT string_agg(a.lastname || ' ' || COALESCE(a.firstname, ''), ', ' ORDER BY ba.position)
                    FROM biblio_authors ba
                    JOIN authors a ON a.id = ba.author_id
                    WHERE ba.biblio_id = b.id
                ) as authors,
                it.barcode as item_barcode
            FROM loans l
            JOIN items it ON l.item_id = it.id
            JOIN biblios b ON it.biblio_id = b.id
            JOIN users u ON l.user_id = u.id
            WHERE l.returned_at IS NULL
              AND l.expiry_at < NOW()
            ORDER BY l.expiry_at ASC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let loans = rows
            .into_iter()
            .map(|row| OverdueLoanRow {
                loan_id: row.get("loan_id"),
                user_id: row.get("user_id"),
                loan_date: row.get("loan_date"),
                expiry_at: row.get("expiry_at"),
                last_reminder_sent_at: row.get("last_reminder_sent_at"),
                reminder_count: row.get::<Option<i32>, _>("reminder_count").unwrap_or(0),
                firstname: row.get("firstname"),
                lastname: row.get("lastname"),
                user_email: row.get("user_email"),
                user_language: row.get::<Option<String>, _>("user_language"),
                biblio_id: row.get("biblio_id"),
                title: row.get("title"),
                authors: row.get("authors"),
                item_barcode: row.get("item_barcode"),
            })
            .collect();

        Ok((loans, total))
    }

    /// Mark loans as reminded: update last_reminder_sent_at and increment reminder_count.
    pub async fn loans_update_reminder_sent(&self, loan_ids: &[i64]) -> AppResult<()> {
        if loan_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"
            UPDATE loans
            SET last_reminder_sent_at = NOW(),
                reminder_count = COALESCE(reminder_count, 0) + 1
            WHERE id = ANY($1)
            "#,
        )
        .bind(loan_ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// A flat row from overdue loan queries, used by the reminders service and API
#[derive(Debug, Clone)]
pub struct OverdueLoanRow {
    pub loan_id: i64,
    pub user_id: i64,
    pub loan_date: DateTime<Utc>,
    pub expiry_at: Option<DateTime<Utc>>,
    pub last_reminder_sent_at: Option<DateTime<Utc>>,
    pub reminder_count: i32,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub user_email: Option<String>,
    pub user_language: Option<String>,
    pub biblio_id: i64,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub item_barcode: Option<String>,
}

