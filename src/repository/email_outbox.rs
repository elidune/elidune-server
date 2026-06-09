//! Email outbox reminder-loan reservation helpers.

use super::Repository;
use crate::error::AppResult;

#[derive(Debug, Clone, Copy)]
pub struct MetricsSnapshot {
    pub active_loans: i64,
    pub pending_holds: i64,
    pub outbox_pending_count: i64,
    pub outbox_oldest_pending_seconds: i64,
}

impl Repository {
    /// Loan IDs reserved by a pending overdue-reminder outbox row.
    pub async fn email_outbox_reminder_loan_ids(&self, outbox_id: i64) -> AppResult<Vec<i64>> {
        let ids = sqlx::query_scalar(
            r#"
            SELECT loan_id
            FROM email_outbox_reminder_loans
            WHERE outbox_id = $1
            ORDER BY loan_id
            "#,
        )
        .bind(outbox_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    /// Release loan reservations when delivery failed permanently or completed.
    pub async fn email_outbox_release_reminder_loans(&self, outbox_id: i64) -> AppResult<()> {
        sqlx::query(
            r#"
            DELETE FROM email_outbox_reminder_loans
            WHERE outbox_id = $1
            "#,
        )
        .bind(outbox_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Event ID reserved by an outbox row when this row is an event announcement.
    pub async fn email_outbox_event_id_for_outbox(&self, outbox_id: i64) -> AppResult<Option<i64>> {
        let event_id = sqlx::query_scalar(
            r#"
            SELECT event_id
            FROM email_outbox_event_announcements
            WHERE outbox_id = $1
            "#,
        )
        .bind(outbox_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(event_id)
    }

    /// Pending outbox rows still linked to a given event announcement.
    pub async fn email_outbox_pending_event_announcement_count(&self, event_id: i64) -> AppResult<i64> {
        let pending = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM email_outbox_event_announcements ea
            JOIN email_outbox o ON o.id = ea.outbox_id
            WHERE ea.event_id = $1
              AND o.status = 'pending'
            "#,
        )
        .bind(event_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(pending)
    }

    /// Release event-announcement reservation row.
    pub async fn email_outbox_release_event_announcement(&self, outbox_id: i64) -> AppResult<()> {
        sqlx::query(
            r#"
            DELETE FROM email_outbox_event_announcements
            WHERE outbox_id = $1
            "#,
        )
        .bind(outbox_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Operational metrics snapshot for Prometheus exposition.
    pub async fn metrics_snapshot(&self) -> AppResult<MetricsSnapshot> {
        let active_loans: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM loans
            WHERE returned_at IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let pending_holds: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM holds
            WHERE status = 'pending'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let outbox_pending_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM email_outbox
            WHERE status = 'pending'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let outbox_oldest_pending_seconds: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(EXTRACT(EPOCH FROM (NOW() - created_at)))::bigint, 0)
            FROM email_outbox
            WHERE status = 'pending'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(MetricsSnapshot {
            active_loans,
            pending_holds,
            outbox_pending_count,
            outbox_oldest_pending_seconds,
        })
    }
}
