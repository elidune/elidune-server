//! Loans domain methods on Repository

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::{
        biblio::{BiblioShort, Isbn, MediaType},
        item::ItemShort,
        loan::{CreateLoan, Loan, LoanDetails, LoanSettings},
        user::{UserShort, UserShortRow},
    },
};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LoansRepository: Send + Sync {
    async fn loans_get_by_id(&self, id: i64) -> AppResult<Loan>;
    async fn loans_get_by_item_identification(&self, item_identification: &str) -> AppResult<Loan>;
    async fn loans_get_for_user(&self, user_id: i64) -> AppResult<Vec<LoanDetails>>;
    async fn loans_archives_get_for_user(&self, user_id: i64) -> AppResult<Vec<LoanDetails>>;
    async fn loans_create(&self, loan: &CreateLoan) -> AppResult<(i64, DateTime<Utc>)>;
    async fn loans_return(&self, loan_id: i64) -> AppResult<LoanDetails>;
    async fn loans_renew(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)>;
    async fn loans_get_settings(&self) -> AppResult<Vec<LoanSettings>>;
    async fn loans_count_active(&self) -> AppResult<i64>;
    async fn loans_count_overdue(&self) -> AppResult<i64>;
    async fn loans_count_active_for_item(&self, item_id: i64) -> AppResult<i64>;
    async fn loans_get_active_ids_for_item(&self, item_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_get_active_ids_for_biblio(&self, biblio_id: i64) -> AppResult<Vec<i64>>;
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
    async fn loans_get_for_user(&self, user_id: i64) -> crate::error::AppResult<Vec<LoanDetails>> {
        Repository::loans_get_for_user(self, user_id).await
    }
    async fn loans_archives_get_for_user(&self, user_id: i64) -> crate::error::AppResult<Vec<LoanDetails>> {
        Repository::loans_archives_get_for_user(self, user_id).await
    }
    async fn loans_create(&self, loan: &CreateLoan) -> crate::error::AppResult<(i64, chrono::DateTime<chrono::Utc>)> {
        Repository::loans_create(self, loan).await
    }
    async fn loans_return(&self, loan_id: i64) -> crate::error::AppResult<LoanDetails> {
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
}


impl Repository {
    /// Resolve loan settings: (duration_days, nb_max_media, nb_max_total, nb_renews).
    async fn resolve_loan_settings(
        &self,
        user_public_type: Option<i64>,
        media_type: Option<&str>,
    ) -> AppResult<(i16, i16, i16, i16)> {
        let default_duration = 21i16;
        let default_nb_max_media = 5i16;
        let default_nb_max_total = 5i16;
        let default_nb_renews = 2i16;

        let ptls = if let (Some(pt_id), Some(mt)) = (user_public_type, media_type) {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews FROM public_type_loan_settings WHERE public_type_id = $1 AND media_type = $2"
            )
            .bind(pt_id)
            .bind(mt)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let pt_row = if let Some(pt_id) = user_public_type {
            sqlx::query("SELECT loan_duration_days, max_loans FROM public_types WHERE id = $1")
                .bind(pt_id)
                .fetch_optional(&self.pool)
                .await?
        } else {
            None
        };

        let ls_row = if let Some(mt) = media_type {
            sqlx::query("SELECT duration, nb_max, nb_renews FROM loans_settings WHERE media_type = $1")
                .bind(mt)
                .fetch_optional(&self.pool)
                .await?
        } else {
            None
        };

        let duration = ptls
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("duration"))
            .or_else(|| pt_row.as_ref().and_then(|r| r.get::<Option<i16>, _>("loan_duration_days")))
            .or_else(|| ls_row.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .unwrap_or(default_duration);

        let nb_max_media = ptls
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_max"))
            .or_else(|| ls_row.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_max")))
            .unwrap_or(default_nb_max_media);

        let nb_max_total = pt_row
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("max_loans"))
            .unwrap_or(default_nb_max_total);

        let nb_renews = ptls
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_renews"))
            .or_else(|| ls_row.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .unwrap_or(default_nb_renews);

        Ok((duration, nb_max_media, nb_max_total, nb_renews))
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

    /// Get active loans for a user
    pub async fn loans_get_for_user(&self, user_id: i64) -> AppResult<Vec<LoanDetails>> {
        let author_subquery = r#"
            (SELECT jsonb_build_object(
                'id', a.id::text, 'lastname', a.lastname, 'firstname', a.firstname,
                'bio', a.bio, 'notes', a.notes, 'function', ba.function
            ) FROM biblio_authors ba JOIN authors a ON a.id = ba.author_id
            WHERE ba.biblio_id = b.id ORDER BY ba.position LIMIT 1) as author
        "#;

        let sql = format!(r#"
            SELECT l.id, l.date, l.renew_at, l.nb_renews, l.issue_at,
                   l.returned_at,
                   it.barcode as item_identification,
                   it.id as item_copy_id, it.barcode as item_barcode,
                   it.call_number as item_call_number, it.borrowable as item_borrowable,
                   so.name as item_source_name,
                   b.id as biblio_id, b.media_type, b.isbn as biblio_isbn,
                   b.title, b.publication_date,
                   {author_subquery}
            FROM loans l
            JOIN items it ON l.item_id = it.id
            LEFT JOIN sources so ON it.source_id = so.id
            JOIN biblios b ON it.biblio_id = b.id
            WHERE l.user_id = $1 AND l.returned_at IS NULL
            ORDER BY l.issue_at
        "#);

        let rows = sqlx::query(&sql)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(Self::map_loan_rows(rows))
    }

    /// Get archived (returned) loans for a user
    pub async fn loans_archives_get_for_user(&self, user_id: i64) -> AppResult<Vec<LoanDetails>> {
        let author_subquery = r#"
            (SELECT jsonb_build_object(
                'id', a.id::text, 'lastname', a.lastname, 'firstname', a.firstname,
                'bio', a.bio, 'notes', a.notes, 'function', ba.function
            ) FROM biblio_authors ba JOIN authors a ON a.id = ba.author_id
            WHERE ba.biblio_id = b.id ORDER BY ba.position LIMIT 1) as author
        "#;

        let sql = format!(r#"
            SELECT la.id, la.date, NULL::timestamptz as renew_at, la.nb_renews,
                   la.issue_at, la.returned_at,
                   it.barcode as item_identification,
                   it.id as item_copy_id, it.barcode as item_barcode,
                   it.call_number as item_call_number, it.borrowable as item_borrowable,
                   so.name as item_source_name,

                   b.id as biblio_id, b.media_type, b.isbn as biblio_isbn,
                   b.title, b.publication_date,
                   {author_subquery}
            FROM loans_archives la
            JOIN items it ON la.item_id = it.id
            LEFT JOIN sources so ON it.source_id = so.id
            JOIN biblios b ON it.biblio_id = b.id
            WHERE la.user_id = $1
            ORDER BY la.returned_at DESC
        "#);

        let rows = sqlx::query(&sql)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(Self::map_loan_rows(rows))
    }

    fn map_loan_rows(rows: Vec<sqlx::postgres::PgRow>) -> Vec<LoanDetails> {
        let now = Utc::now();
        rows.into_iter().map(|row| {
            let start_date: DateTime<Utc> = row.get("date");
            let issue_at: Option<DateTime<Utc>> = row.get("issue_at");
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
                start_date,
                issue_at: issue_at.unwrap_or(now),
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
                    is_valid: Some(1),
                    archived_at: None,
                    author: row.get::<Option<serde_json::Value>, _>("author")
                        .and_then(|v| serde_json::from_value(v).ok()),
                    items: vec![borrowed_item],
                },
                user: None,
                item_identification: row.get("item_identification"),
                is_overdue: returned_at.is_none() && issue_at.map(|d| d < now).unwrap_or(false),
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
        let already_borrowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM loans WHERE item_id = $1 AND returned_at IS NULL)"
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;

        if already_borrowed && !loan.force {
            return Err(AppError::BusinessRule("Item is already borrowed".to_string()));
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

        let (duration_days, nb_max_media, nb_max_total, _) = self
            .resolve_loan_settings(user_public_type, media_type.as_deref())
            .await?;

        let issue_at = now + Duration::days(duration_days as i64);

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

        let loan_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO loans (user_id, item_id, date, issue_at, nb_renews)
            VALUES ($1, $2, $3, $4, 0)
            RETURNING id
            "#
        )
        .bind(loan.user_id)
        .bind(item_id)
        .bind(now)
        .bind(issue_at)
        .fetch_one(&self.pool)
        .await?;

        Ok((loan_id, issue_at))
    }

    /// Return a loan (moves it to loans_archives).
    pub async fn loans_return(&self, loan_id: i64) -> AppResult<LoanDetails> {
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
                user_id, item_id, date, nb_renews, issue_at,
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
        .bind(loan.issue_at)
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

        let biblio_row = sqlx::query(
            r#"
            SELECT b.id as biblio_id, b.media_type, b.isbn, b.title, b.publication_date,
                   it.barcode as item_identification,
                   it.id as item_copy_id, it.barcode as item_barcode,
                   it.call_number as item_call_number, it.borrowable as item_borrowable,
                   so.name as item_source_name
            FROM biblios b
            JOIN items it ON it.biblio_id = b.id
            LEFT JOIN sources so ON it.source_id = so.id
            WHERE it.id = $1
            "#
        )
        .bind(loan.item_id)
        .fetch_one(&self.pool)
        .await?;

        let user_short_row = sqlx::query_as::<_, UserShortRow>(
            r#"
            SELECT u.id, u.firstname, u.lastname, u.account_type, u.public_type,
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

        Ok(LoanDetails {
            id: loan.id,
            start_date: loan.date,
            issue_at: loan.issue_at.unwrap_or(now),
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
                is_valid: Some(1),
                archived_at: None,
                author: None,
                items: vec![item_short],
            },
            user,
            item_identification: biblio_row.get("item_identification"),
            is_overdue: false,
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

        let (duration_days, _nb_max_media, _nb_max_total, max_renews) = self
            .resolve_loan_settings(user_public_type, media_type.as_deref())
            .await?;

        let current_renews = loan.nb_renews.unwrap_or(0);

        if current_renews >= max_renews {
            return Err(AppError::BusinessRule(format!(
                "Maximum renewals reached ({}/{})",
                current_renews, max_renews
            )));
        }

        let new_issue_date = now + Duration::days(duration_days as i64);
        let new_renews = current_renews + 1;

        sqlx::query(
            "UPDATE loans SET issue_at = $1, renew_at = $2, nb_renews = $3 WHERE id = $4"
        )
        .bind(new_issue_date)
        .bind(now)
        .bind(new_renews)
        .bind(loan_id)
        .execute(&self.pool)
        .await?;

        Ok((new_issue_date, new_renews))
    }

    /// Get loan settings
    pub async fn loans_get_settings(&self) -> AppResult<Vec<LoanSettings>> {
        sqlx::query_as::<_, LoanSettings>("SELECT * FROM loans_settings ORDER BY media_type")
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
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
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND issue_at < NOW()"
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
                l.issue_at,
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
              AND l.issue_at < NOW()
              AND (
                  l.last_reminder_sent_at IS NULL
                  OR l.last_reminder_sent_at < NOW() - ($1 || ' days')::INTERVAL
              )
              AND u.email IS NOT NULL
              AND u.email != ''
              AND u.receive_reminders = TRUE
            ORDER BY u.id, l.issue_at
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
                issue_at: row.get("issue_at"),
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
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND issue_at < NOW()"
        )
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query(
            r#"
            SELECT
                l.id as loan_id,
                l.user_id,
                l.date as loan_date,
                l.issue_at,
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
              AND l.issue_at < NOW()
            ORDER BY l.issue_at ASC
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
                issue_at: row.get("issue_at"),
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
    pub issue_at: Option<DateTime<Utc>>,
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

