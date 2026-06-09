//! Loans repository — overdue reminder queries.

use chrono::{DateTime, Utc};
use sqlx::Row;

use super::super::Repository;
use crate::error::AppResult;

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

impl Repository {
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
              AND NOT EXISTS (
                  SELECT 1
                  FROM email_outbox_reminder_loans rl
                  JOIN email_outbox o ON o.id = rl.outbox_id
                  WHERE rl.loan_id = l.id
                    AND o.status = 'pending'
              )
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
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND expiry_at < NOW()",
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
