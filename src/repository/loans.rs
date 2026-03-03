//! Loans domain methods on Repository

use chrono::{DateTime, Duration, Utc};
use sqlx::{Pool, Postgres, Row};

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::{
        item::ItemShort,
        loan::{CreateLoan, Loan, LoanDetails, LoanSettings},
        specimen::SpecimenShort,
        user::{UserShort, UserShortRow},
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

    /// Get active loan by specimen identification
    pub async fn loans_get_by_specimen_identification(&self, specimen_identification: &str) -> AppResult<Loan> {
        sqlx::query_as::<_, Loan>(
            r#"
            SELECT l.* FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            WHERE s.barcode = $1 AND l.returned_date IS NULL
            ORDER BY l.id DESC LIMIT 1
            "#
        )
        .bind(specimen_identification)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("No active loan found for specimen {}", specimen_identification)))
    }

    /// Get loans for a user
    pub async fn loans_get_for_user(&self, user_id: i64) -> AppResult<Vec<LoanDetails>> {
        let loans = sqlx::query(
            r#"
            SELECT l.*, s.barcode as specimen_identification,
                   s.id as specimen_id, s.barcode as specimen_barcode,
                   s.call_number as specimen_call_number, s.borrow_status as specimen_borrow_status,
                   so.name as specimen_source_name,
                   i.id as item_id, i.media_type, i.isbn as item_isbn,
                   i.title, i.publication_date
            FROM loans l
            JOIN specimens s ON l.specimen_id = s.id
            LEFT JOIN sources so ON s.source_id = so.id
            JOIN items i ON s.item_id = i.id
            WHERE l.user_id = $1 AND l.returned_date IS NULL
            ORDER BY l.issue_date
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let now = Utc::now();

        let mut result = Vec::new();
        for row in loans {
            let start_date: DateTime<Utc> = row.get("date");
            let issue_date: Option<DateTime<Utc>> = row.get("issue_date");
            let renew_date: Option<DateTime<Utc>> = row.get("renew_date");

            let borrowed_specimen = SpecimenShort {
                id: row.get("specimen_id"),
                barcode: row.get("specimen_barcode"),
                call_number: row.get("specimen_call_number"),
                borrow_status: row.get("specimen_borrow_status"),
                source_name: row.get("specimen_source_name"),
                availability: Some(0), // borrowed = not available
            };

            result.push(LoanDetails {
                id: row.get("id"),
                start_date,
                issue_date: issue_date.unwrap_or(now),
                renewal_date: renew_date,
                nb_renews: row.get::<Option<i16>, _>("nb_renews").unwrap_or(0),
                item: ItemShort {
                    id: row.get("item_id"),
                    media_type: row.get("media_type"),
                    isbn: row.get("item_isbn"),
                    title: row.get("title"),
                    date: row.get("publication_date"),
                    status: 0,
                    is_valid: Some(1),
                    archived_at: None,
                    author: None,
                    specimens: vec![borrowed_specimen],
                },
                user: None,
                specimen_identification: row.get("specimen_identification"),
                is_overdue: issue_date.map(|d| d < now).unwrap_or(false),
            });
        }

        Ok(result)
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
            "SELECT EXISTS(SELECT 1 FROM loans WHERE specimen_id = $1 AND returned_date IS NULL)"
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
            SELECT s.borrow_status, i.media_type
            FROM specimens s
            JOIN items i ON s.item_id = i.id
            WHERE s.id = $1
            "#
        )
        .bind(specimen_id)
        .fetch_one(&self.pool)
        .await?;

        let status: Option<i16> = specimen_row.get("borrow_status");
        let media_type: Option<String> = specimen_row.get("media_type");

        // Check if borrowable
        if status != Some(98) && !loan.force {
            return Err(AppError::BusinessRule("Specimen is not borrowable".to_string()));
        }

        // Get loan duration from settings
        let duration_days: i16 = sqlx::query_scalar(
            "SELECT duration FROM loans_settings WHERE media_type = $1"
        )
        .bind(&media_type)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(21); // Default 21 days

        let issue_date = now + Duration::days(duration_days as i64);

        // Check max loans
        let current_loans: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE user_id = $1 AND returned_date IS NULL"
        )
        .bind(loan.user_id)
        .fetch_one(&self.pool)
        .await?;

        let max_loans: i16 = sqlx::query_scalar(
            "SELECT nb_max FROM loans_settings WHERE media_type = $1"
        )
        .bind(&media_type)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(5); // Default 5 loans

        if current_loans >= max_loans as i64 && !loan.force {
            return Err(AppError::BusinessRule(format!(
                "Maximum loans reached ({}/{})",
                current_loans, max_loans
            )));
        }

        // Create the loan
        let loan_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO loans (user_id, specimen_id, date, issue_date, nb_renews)
            VALUES ($1, $2, $3, $4, 0)
            RETURNING id
            "#
        )
        .bind(loan.user_id)
        .bind(specimen_id)
        .bind(now)
        .bind(issue_date)
        .fetch_one(&self.pool)
        .await?;

        Ok((loan_id, issue_date))
    }

    /// Return a loan (moves it to loans_archives)
    pub async fn loans_return(&self, loan_id: i64) -> AppResult<LoanDetails> {
        let now = Utc::now();

        // Get loan details before returning
        let loan = self.loans_get_by_id(loan_id).await?;

        if loan.returned_date.is_some() {
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
                user_id, specimen_id, date, nb_renews, issue_date, 
                returned_date, notes, borrower_public_type,
                addr_city, account_type
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(loan.user_id)
        .bind(loan.specimen_id)
        .bind(loan.date)
        .bind(loan.nb_renews)
        .bind(loan.issue_date)
        .bind(now)
        .bind(&loan.notes)
        .bind(user_row.as_ref().and_then(|r| r.get::<Option<i32>, _>("public_type")))
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
                   s.call_number as specimen_call_number, s.borrow_status as specimen_borrow_status,
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
            borrow_status: item_row.get("specimen_borrow_status"),
            source_name: item_row.get("specimen_source_name"),
            availability: Some(1), // returned = available
        };

        Ok(LoanDetails {
            id: loan.id,
            start_date: loan.date,
            issue_date: loan.issue_date.unwrap_or(now),
            renewal_date: loan.renew_date,
            nb_renews: loan.nb_renews.unwrap_or(0),
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

        if loan.returned_date.is_some() {
            return Err(AppError::BusinessRule("Cannot renew a returned loan".to_string()));
        }

        // Get max renewals
        let specimen_row = sqlx::query(
            "SELECT i.media_type FROM specimens s JOIN items i ON s.item_id = i.id WHERE s.id = $1"
        )
        .bind(loan.specimen_id)
        .fetch_one(&self.pool)
        .await?;

        let media_type: Option<String> = specimen_row.get("media_type");

        let max_renews: i16 = sqlx::query_scalar(
            "SELECT nb_renews FROM loans_settings WHERE media_type = $1"
        )
        .bind(&media_type)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(2);

        let current_renews = loan.nb_renews.unwrap_or(0);

        if current_renews >= max_renews {
            return Err(AppError::BusinessRule(format!(
                "Maximum renewals reached ({}/{})",
                current_renews, max_renews
            )));
        }

        // Get loan duration
        let duration_days: i16 = sqlx::query_scalar(
            "SELECT duration FROM loans_settings WHERE media_type = $1"
        )
        .bind(&media_type)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(21);

        let new_issue_date = now + Duration::days(duration_days as i64);
        let new_renews = current_renews + 1;

        sqlx::query(
            "UPDATE loans SET issue_date = $1, renew_date = $2, nb_renews = $3 WHERE id = $4"
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
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loans WHERE returned_date IS NULL")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Count overdue loans
    pub async fn loans_count_overdue(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE returned_date IS NULL AND issue_date < NOW()"
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Count active (non-returned) loans for a specimen
    pub async fn loans_count_active_for_specimen(&self, specimen_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE specimen_id = $1 AND returned_date IS NULL"
        )
        .bind(specimen_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Get IDs of active loans for a specimen
    pub async fn loans_get_active_ids_for_specimen(&self, specimen_id: i64) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM loans WHERE specimen_id = $1 AND returned_date IS NULL"
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
            WHERE s.item_id = $1 AND l.returned_date IS NULL
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
            WHERE s.item_id = $1 AND l.returned_date IS NULL
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
            "SELECT COUNT(*) FROM loans WHERE user_id = $1 AND returned_date IS NULL"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }
}


