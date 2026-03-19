//! Loans domain methods on Repository

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::{
        item::{Isbn, ItemShort},
        loan::{CreateLoan, Loan, LoanDetails, LoanSettings},
        specimen::SpecimenShort,
        user::{UserShort, UserShortRow},
    },
};

impl Repository {
    /// Resolve loan settings: (duration_days, nb_max_media, nb_max_total, nb_renews).
    /// nb_max_media: max loans for this specific media type.
    /// nb_max_total: max total loans across all media types.
    /// Priority: public_type_loan_settings > public_types > loans_settings > defaults.
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

        // Per-media-type limit (for this document type)
        let nb_max_media = ptls
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_max"))
            .or_else(|| ls_row.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_max")))
            .unwrap_or(default_nb_max_media);

        // Total limit (across all media types)
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

    /// Get active loan by specimen identification
    pub async fn loans_get_by_specimen_identification(&self, specimen_identification: &str) -> AppResult<Loan> {
        sqlx::query_as::<_, Loan>(
            r#"
            SELECT l.* FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            WHERE s.barcode = $1 AND l.returned_at IS NULL
            ORDER BY l.id DESC LIMIT 1
            "#
        )
        .bind(specimen_identification)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("No active loan found for specimen {}", specimen_identification)))
    }

    /// Get loans for a user
    /// Get active loans for a user
    pub async fn loans_get_for_user(&self, user_id: i64) -> AppResult<Vec<LoanDetails>> {
        let author_subquery = r#"
            (SELECT jsonb_build_object(
                'id', a.id::text, 'lastname', a.lastname, 'firstname', a.firstname,
                'bio', a.bio, 'notes', a.notes, 'function', ia.role
            ) FROM item_authors ia JOIN authors a ON a.id = ia.author_id
            WHERE ia.item_id = i.id ORDER BY ia.position LIMIT 1) as author
        "#;

        let sql = format!(r#"
            SELECT l.id, l.date, l.renew_at, l.nb_renews, l.issue_at,
                   l.returned_at,
                   s.barcode as specimen_identification,
                   s.id as specimen_id, s.barcode as specimen_barcode,
                   s.call_number as specimen_call_number, s.borrowable as specimen_borrowable,
                   so.name as specimen_source_name,
                   i.id as item_id, i.media_type, i.isbn as item_isbn,
                   i.title, i.publication_date,
                   {author_subquery}
            FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            LEFT JOIN sources so ON s.source_id = so.id
            JOIN items i ON s.item_id = i.id
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
                'bio', a.bio, 'notes', a.notes, 'function', ia.role
            ) FROM item_authors ia JOIN authors a ON a.id = ia.author_id
            WHERE ia.item_id = i.id ORDER BY ia.position LIMIT 1) as author
        "#;

        let sql = format!(r#"
            SELECT la.id, la.date, NULL::timestamptz as renew_at, la.nb_renews,
                   la.issue_at, la.returned_at,
                   s.barcode as specimen_identification,
                   s.id as specimen_id, s.barcode as specimen_barcode,
                   s.call_number as specimen_call_number, s.borrowable as specimen_borrowable,
                   so.name as specimen_source_name,
                   i.id as item_id, i.media_type, i.isbn as item_isbn,
                   i.title, i.publication_date,
                   {author_subquery}
            FROM loans_archives la
            JOIN specimens s ON la.specimen_id = s.id
            LEFT JOIN sources so ON s.source_id = so.id
            JOIN items i ON s.item_id = i.id
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

            let borrowed_specimen = SpecimenShort {
                id: row.get("specimen_id"),
                barcode: row.get("specimen_barcode"),
                call_number: row.get("specimen_call_number"),
                borrowable: row.get("specimen_borrowable"),
                source_name: row.get("specimen_source_name"),
                availability: Some(0),
            };

            LoanDetails {
                id: row.get("id"),
                start_date,
                issue_at: issue_at.unwrap_or(now),
                renewal_date: renew_at,
                nb_renews: row.get::<Option<i16>, _>("nb_renews").unwrap_or(0),
                returned_at,
                item: ItemShort {
                    id: row.get("item_id"),
                    media_type: row.get("media_type"),
                    isbn: row
                        .get::<Option<String>, _>("item_isbn")
                        .map(Isbn::new)
                        .filter(|i| !i.is_empty()),
                    title: row.get("title"),
                    date: row.get("publication_date"),
                    status: 0,
                    is_valid: Some(1),
                    archived_at: None,
                    author: row.get::<Option<serde_json::Value>, _>("author")
                        .and_then(|v| serde_json::from_value(v).ok()),
                    specimens: vec![borrowed_specimen],
                },
                user: None,
                specimen_identification: row.get("specimen_identification"),
                is_overdue: returned_at.is_none() && issue_at.map(|d| d < now).unwrap_or(false),
            }
        }).collect()
    }

    /// Create a new loan
    pub async fn loans_create(&self, loan: &CreateLoan) -> AppResult<(i64, DateTime<Utc>)> {
        let now = Utc::now();

        // Get specimen ID
        let specimen_id = if let Some(id) = loan.specimen_id {
            id
        } else if let Some(ref identification) = loan.specimen_identification {
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM specimens WHERE barcode = $1"
            )
            .bind(identification)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Specimen not found".to_string()))?
        } else {
            return Err(AppError::BadRequest("specimen_id or specimen_identification required".to_string()));
        };

        // Check if specimen is already borrowed
        let already_borrowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM loans WHERE specimen_id = $1 AND returned_at IS NULL)"
        )
        .bind(specimen_id)
        .fetch_one(&self.pool)
        .await?;

        if already_borrowed && !loan.force {
            return Err(AppError::BusinessRule("Specimen is already borrowed".to_string()));
        }

        // Get specimen info and loan settings
        let specimen_row = sqlx::query(
            r#"
            SELECT s.borrowable, i.media_type
            FROM specimens s
            JOIN items i ON s.item_id = i.id
            WHERE s.id = $1
            "#
        )
        .bind(specimen_id)
        .fetch_one(&self.pool)
        .await?;

        let borrowable: bool = specimen_row.get("borrowable");
        let media_type: Option<String> = specimen_row.get("media_type");

        // Check if borrowable
        if !borrowable && !loan.force {
            return Err(AppError::BusinessRule("Specimen is not borrowable".to_string()));
        }

        // Get user's public_type for loan settings cascade
        let user_public_type: Option<i64> = sqlx::query_scalar(
            "SELECT public_type FROM users WHERE id = $1"
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?;

        // Resolve duration, nb_max_media, nb_max_total, nb_renews
        let (duration_days, nb_max_media, nb_max_total, _) = self
            .resolve_loan_settings(user_public_type, media_type.as_deref())
            .await?;

        let issue_at = now + Duration::days(duration_days as i64);

        // Check max loans: total AND per media type
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
                JOIN specimens s ON l.specimen_id = s.id
                JOIN items i ON s.item_id = i.id
                WHERE l.user_id = $1 AND l.returned_at IS NULL AND i.media_type = $2
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

        // Create the loan
        let loan_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO loans (user_id, specimen_id, date, issue_at, nb_renews)
            VALUES ($1, $2, $3, $4, 0)
            RETURNING id
            "#
        )
        .bind(loan.user_id)
        .bind(specimen_id)
        .bind(now)
        .bind(issue_at)
        .fetch_one(&self.pool)
        .await?;

        Ok((loan_id, issue_at))
    }

    /// Return a loan (moves it to loans_archives)
    pub async fn loans_return(&self, loan_id: i64) -> AppResult<LoanDetails> {
        let now = Utc::now();

        // Get loan details before returning
        let loan = self.loans_get_by_id(loan_id).await?;

        if loan.returned_at.is_some() {
            return Err(AppError::BusinessRule("Loan already returned".to_string()));
        }

        // Get user info for archiving
        let user_row = sqlx::query(
            "SELECT addr_city, account_type, public_type FROM users WHERE id = $1"
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?;

        let account_type: Option<String> = user_row.as_ref().and_then(|r| r.get("account_type"));

        // Archive the loan
        sqlx::query(
            r#"
            INSERT INTO loans_archives (
                user_id, specimen_id, date, nb_renews, issue_at, 
                returned_at, notes, borrower_public_type,
                addr_city, account_type
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(loan.user_id)
        .bind(loan.specimen_id)
        .bind(loan.date)
        .bind(loan.nb_renews)
        .bind(loan.issue_at)
        .bind(now)
        .bind(&loan.notes)
        .bind(user_row.as_ref().and_then(|r| r.get::<Option<i64>, _>("public_type")))
        .bind(user_row.as_ref().and_then(|r| r.get::<Option<String>, _>("addr_city")))
        .bind(account_type)
        .execute(&self.pool)
        .await?;

        // Delete from active loans
        sqlx::query("DELETE FROM loans WHERE id = $1")
            .bind(loan_id)
            .execute(&self.pool)
            .await?;

        // Get item details with the returned specimen
        let item_row = sqlx::query(
            r#"
            SELECT i.id, i.media_type, i.isbn, i.title, i.publication_date,
                   s.barcode as specimen_identification,
                   s.id as specimen_id, s.barcode as specimen_barcode,
                   s.call_number as specimen_call_number, s.borrowable as specimen_borrowable,
                   so.name as specimen_source_name
            FROM items i
            JOIN specimens s ON s.item_id = i.id
            LEFT JOIN sources so ON s.source_id = so.id
            WHERE s.id = $1
            "#
        )
        .bind(loan.specimen_id)
        .fetch_one(&self.pool)
        .await?;

        let user_row = sqlx::query_as::<_, UserShortRow>(
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

        let user: Option<UserShort> = user_row.map(|r| r.into());

        let specimen_short = SpecimenShort {
            id: item_row.get("specimen_id"),
            barcode: item_row.get("specimen_barcode"),
            call_number: item_row.get("specimen_call_number"),
            borrowable: item_row.get("specimen_borrowable"),
            source_name: item_row.get("specimen_source_name"),
            availability: Some(1), // returned = available
        };

        Ok(LoanDetails {
            id: loan.id,
            start_date: loan.date,
            issue_at: loan.issue_at.unwrap_or(now),
            renewal_date: loan.renew_at,
            nb_renews: loan.nb_renews.unwrap_or(0),
            returned_at: Some(now),
            item: ItemShort {
                id: item_row.get("id"),
                media_type: item_row.get("media_type"),
                isbn: item_row.get("isbn"),
                title: item_row.get("title"),
                date: item_row.get("publication_date"),
                status: 0,
                is_valid: Some(1),
                archived_at: None,
                author: None,
                specimens: vec![specimen_short],
            },
            user,
            specimen_identification: item_row.get("specimen_identification"),
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

        // Get specimen media_type and user public_type
        let specimen_row = sqlx::query(
            "SELECT i.media_type FROM specimens s JOIN items i ON s.item_id = i.id WHERE s.id = $1"
        )
        .bind(loan.specimen_id)
        .fetch_one(&self.pool)
        .await?;

        let media_type: Option<String> = specimen_row.get("media_type");

        let user_public_type: Option<i64> = sqlx::query_scalar(
            "SELECT public_type FROM users WHERE id = $1"
        )
        .bind(loan.user_id)
        .fetch_optional(&self.pool)
        .await?;

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
        let settings = sqlx::query_as::<_, LoanSettings>(
            "SELECT * FROM loans_settings ORDER BY media_type"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(settings)
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

    /// Count active (non-returned) loans for a specimen
    pub async fn loans_count_active_for_specimen(&self, specimen_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE specimen_id = $1 AND returned_at IS NULL"
        )
        .bind(specimen_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Get IDs of active loans for a specimen
    pub async fn loans_get_active_ids_for_specimen(&self, specimen_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM loans WHERE specimen_id = $1 AND returned_at IS NULL"
        )
        .bind(specimen_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Get IDs of active loans for an item
    pub async fn loans_get_active_ids_for_item(&self, item_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            r#"
            SELECT l.id FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            WHERE s.item_id = $1 AND l.returned_at IS NULL
            "#
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Count active loans for an item (via specimens)
    pub async fn loans_count_active_for_item(&self, item_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            WHERE s.item_id = $1 AND l.returned_at IS NULL
            "#
        )
        .bind(item_id)
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
    /// Filters: overdue + not reminded recently (frequency_days) + user has email + opted in.
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
                i.id as item_id,
                i.title,
                (
                    SELECT string_agg(a.lastname || ' ' || COALESCE(a.firstname, ''), ', ' ORDER BY ia.position)
                    FROM item_authors ia
                    JOIN authors a ON a.id = ia.author_id
                    WHERE ia.item_id = i.id
                ) as authors,
                s.barcode as specimen_barcode
            FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            JOIN items i ON s.item_id = i.id
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
                item_id: row.get("item_id"),
                title: row.get("title"),
                authors: row.get("authors"),
                specimen_barcode: row.get("specimen_barcode"),
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
                i.id as item_id,
                i.title,
                (
                    SELECT string_agg(a.lastname || ' ' || COALESCE(a.firstname, ''), ', ' ORDER BY ia.position)
                    FROM item_authors ia
                    JOIN authors a ON a.id = ia.author_id
                    WHERE ia.item_id = i.id
                ) as authors,
                s.barcode as specimen_barcode
            FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            JOIN items i ON s.item_id = i.id
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
                item_id: row.get("item_id"),
                title: row.get("title"),
                authors: row.get("authors"),
                specimen_barcode: row.get("specimen_barcode"),
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
    pub item_id: i64,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub specimen_barcode: Option<String>,
}

