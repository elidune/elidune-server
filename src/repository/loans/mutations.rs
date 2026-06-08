//! Loans repository — create, return, and renew mutations.

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;

use super::super::Repository;
use super::LOAN_DETAILS_FIRST_AUTHOR_SQL;
use crate::{
    error::{AppError, AppResult},
    models::{
        biblio::BiblioShort,
        item::ItemShort,
        loan::{CreateLoan, LoanCreateOutcome, LoanDetails, LoanReturnOutcome, LoanSettingsRenewAt},
        user::{UserShort, UserShortRow},
    },
};

impl Repository {
    /// Create a new loan
    pub async fn loans_create(&self, loan: &CreateLoan) -> AppResult<LoanCreateOutcome> {
        let now = Utc::now();

        // Get item (physical copy) ID
        let item_id = if let Some(id) = loan.item_id {
            id
        } else if let Some(ref identification) = loan.item_identification {
            sqlx::query_scalar::<_, i64>("SELECT id FROM items WHERE barcode = $1")
                .bind(identification)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| AppError::NotFound("Item not found".to_string()))?
        } else {
            return Err(AppError::BadRequest(
                "item_id or item_identification required".to_string(),
            ));
        };

        // Check if item is already borrowed
        let loan_id: Option<i64> = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM loans WHERE item_id = $1 AND returned_at IS NULL",
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
            "#,
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
            "SELECT public_type FROM users WHERE id = $1",
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
            "SELECT COUNT(*) FROM loans WHERE user_id = $1 AND returned_at IS NULL",
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
                "#,
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
            "#,
        )
        .bind(loan.user_id)
        .bind(item_id)
        .bind(now)
        .bind(expiry_at)
        .fetch_one(&mut *tx)
        .await?;

        let fulfilled_hold_id = if loan.force {
            self.holds_cancel_active_for_item_tx(&mut tx, item_id)
                .await?;
            None
        } else {
            self.holds_fulfill_active_for_user_item_tx(&mut tx, loan.user_id, item_id)
                .await?
        };

        tx.commit().await?;

        Ok(LoanCreateOutcome {
            loan_id,
            expiry_at,
            fulfilled_hold_id,
        })
    }

    /// Return a loan (moves it to loans_archives).
    pub async fn loans_return(&self, loan_id: i64) -> AppResult<LoanReturnOutcome> {
        let now = Utc::now();

        let loan = self.loans_get_by_id(loan_id).await?;

        if loan.returned_at.is_some() {
            return Err(AppError::BusinessRule("Loan already returned".to_string()));
        }

        let user_row = sqlx::query(
            "SELECT addr_city, account_type, public_type FROM users WHERE id = $1",
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
            "#,
        )
        .bind(loan.user_id)
        .bind(loan.item_id)
        .bind(loan.date)
        .bind(loan.nb_renews)
        .bind(loan.expiry_at)
        .bind(now)
        .bind(&loan.notes)
        .bind(
            user_row
                .as_ref()
                .and_then(|r| r.get::<Option<i64>, _>("public_type")),
        )
        .bind(
            user_row
                .as_ref()
                .and_then(|r| r.get::<Option<String>, _>("addr_city")),
        )
        .bind(account_type)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM loans WHERE id = $1")
            .bind(loan_id)
            .execute(&mut *tx)
            .await?;

        let readied_hold = self
            .holds_notify_next_tx(&mut tx, loan.item_id, self.hold_ready_expiry_days())
            .await?;

        if let Some(ref h) = readied_hold {
            tracing::debug!(
                target: "loans",
                hold_id = h.id,
                item_id = loan.item_id,
                "Marked next pending hold as ready after loan return"
            );
        }

        tx.commit().await?;

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
            "#,
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

        Ok(LoanReturnOutcome {
            details,
            readied_hold,
            hold_ready_email: None,
        })
    }

    /// Renew a loan
    pub async fn loans_renew(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)> {
        let now = Utc::now();

        let loan = self.loans_get_by_id(loan_id).await?;

        if loan.returned_at.is_some() {
            return Err(AppError::BusinessRule(
                "Cannot renew a returned loan".to_string(),
            ));
        }

        let item_row = sqlx::query(
            "SELECT b.media_type FROM items it JOIN biblios b ON it.biblio_id = b.id WHERE it.id = $1",
        )
        .bind(loan.item_id)
        .fetch_one(&self.pool)
        .await?;

        let media_type: Option<String> = item_row.get("media_type");

        let user_public_type: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT public_type FROM users WHERE id = $1",
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
            "UPDATE loans SET expiry_at = $1, renew_at = $2, nb_renews = $3 WHERE id = $4",
        )
        .bind(new_expiry_date)
        .bind(now)
        .bind(new_renews)
        .bind(loan_id)
        .execute(&self.pool)
        .await?;

        Ok((new_expiry_date, new_renews))
    }
}
