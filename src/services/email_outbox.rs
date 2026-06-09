//! Background worker for the `email_outbox` table.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::{
    email::EmailService,
    error::{AppError, AppResult},
    repository::Repository,
    services::audit::{self, AuditLogMeta, AuditService},
};

/// Maximum send attempts before marking a row as permanently failed.
const MAX_ATTEMPTS: i32 = 5;

/// Default batch size when draining the outbox.
const DEFAULT_BATCH_SIZE: i64 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OutboxBody {
    plain: String,
    html: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OutboxRow {
    id: i64,
    to_addr: String,
    subject: String,
    body: String,
    attempts: i32,
}

/// Summary returned after one outbox drain cycle.
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailOutboxBatchReport {
    pub processed: u32,
    pub sent: u32,
    pub failed: u32,
    pub deferred: u32,
    pub reminders_confirmed: u32,
}

/// Claim and send up to `batch_size` pending outbox rows.
pub async fn process_outbox_batch(
    email: &EmailService,
    repository: &Repository,
    audit: &AuditService,
    batch_size: Option<i64>,
) -> AppResult<EmailOutboxBatchReport> {
    let pool = repository.pool();
    let limit = batch_size.unwrap_or(DEFAULT_BATCH_SIZE).clamp(1, 100);
    let mut report = EmailOutboxBatchReport::default();

    let rows: Vec<OutboxRow> = sqlx::query_as(
        r#"
        SELECT id, to_addr, subject, body, attempts
        FROM email_outbox
        WHERE status = 'pending'
        ORDER BY created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)?;

    for row in rows {
        report.processed += 1;

        let body = match serde_json::from_str::<OutboxBody>(&row.body) {
            Ok(b) => b,
            Err(e) => {
                mark_failed(repository, audit, row.id, row.to_addr.as_str(), row.attempts, &format!("invalid body JSON: {e}"))
                    .await?;
                report.failed += 1;
                continue;
            }
        };

        match email
            .send_email_with_html(&row.to_addr, &row.subject, &body.plain, &body.html)
            .await
        {
            Ok(()) => {
                sqlx::query(
                    r#"
                    UPDATE email_outbox
                    SET status = 'sent', sent_at = $2, attempts = attempts + 1
                    WHERE id = $1
                    "#,
                )
                .bind(row.id)
                .bind(Utc::now())
                .execute(pool)
                .await
                .map_err(AppError::from)?;
                report.sent += 1;

                if let Some(loan_ids) = apply_reminder_delivery(repository, audit, row.id, &row.to_addr)
                    .await?
                {
                    report.reminders_confirmed += 1;
                    tracing::info!(
                        outbox_id = row.id,
                        loan_count = loan_ids.len(),
                        "overdue reminder delivered; loan tracking updated"
                    );
                }

                apply_event_announcement_delivery(repository, row.id).await?;
            }
            Err(e) => {
                let next_attempts = row.attempts + 1;
                if next_attempts >= MAX_ATTEMPTS {
                    mark_failed(repository, audit, row.id, row.to_addr.as_str(), row.attempts, &e.to_string())
                        .await?;
                    report.failed += 1;
                } else {
                    sqlx::query(
                        r#"
                        UPDATE email_outbox
                        SET attempts = attempts + 1
                        WHERE id = $1
                        "#,
                    )
                    .bind(row.id)
                    .execute(pool)
                    .await
                    .map_err(AppError::from)?;
                    report.deferred += 1;
                    tracing::warn!(
                        outbox_id = row.id,
                        attempts = next_attempts,
                        error = %e,
                        "email outbox send deferred"
                    );
                }
            }
        }
    }

    Ok(report)
}

/// After SMTP success: update loan reminder columns and release reservations.
async fn apply_reminder_delivery(
    repository: &Repository,
    audit: &AuditService,
    outbox_id: i64,
    to_addr: &str,
) -> AppResult<Option<Vec<i64>>> {
    let loan_ids = repository.email_outbox_reminder_loan_ids(outbox_id).await?;
    if loan_ids.is_empty() {
        return Ok(None);
    }

    repository.loans_update_reminder_sent(&loan_ids).await?;
    repository.email_outbox_release_reminder_loans(outbox_id).await?;

    audit.log(
        audit::event::EMAIL_OVERDUE_REMINDER_SENT,
        None,
        None,
        None,
        None,
        Some(serde_json::json!({
            "email": to_addr,
            "loan_ids": loan_ids,
            "loan_count": loan_ids.len(),
            "outbox_id": outbox_id,
            "delivery": "sent",
        })),
        AuditLogMeta::success(),
    );

    Ok(Some(loan_ids))
}

/// After SMTP success: if this row belongs to an event announcement, mark the event sent
/// once no pending outbox rows remain for that event.
async fn apply_event_announcement_delivery(repository: &Repository, outbox_id: i64) -> AppResult<()> {
    let Some(event_id) = repository
        .email_outbox_event_id_for_outbox(outbox_id)
        .await?
    else {
        return Ok(());
    };

    let pending = repository
        .email_outbox_pending_event_announcement_count(event_id)
        .await?;
    if pending == 0 {
        repository.events_set_announcement_sent_at(event_id).await?;
    }

    Ok(())
}

async fn mark_failed(
    repository: &Repository,
    audit: &AuditService,
    id: i64,
    to_addr: &str,
    attempts: i32,
    reason: &str,
) -> AppResult<()> {
    let pool = repository.pool();
    sqlx::query(
        r#"
        UPDATE email_outbox
        SET status = 'failed', attempts = $2
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(attempts + 1)
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    let loan_ids = repository.email_outbox_reminder_loan_ids(id).await?;
    if !loan_ids.is_empty() {
        repository.email_outbox_release_reminder_loans(id).await?;
        audit.log(
            audit::event::EMAIL_OVERDUE_REMINDER_SENT,
            None,
            None,
            None,
            None,
            Some(serde_json::json!({
                "email": to_addr,
                "loan_ids": loan_ids,
                "loan_count": loan_ids.len(),
                "outbox_id": id,
                "delivery": "failed",
                "reason": reason,
            })),
            AuditLogMeta::failure_background("email_delivery_failed", reason.to_string()),
        );
    }

    repository.email_outbox_release_event_announcement(id).await?;

    tracing::error!(outbox_id = id, reason, "email outbox row marked failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::OutboxBody;

    #[test]
    fn outbox_body_deserializes_plain_and_html_only() {
        let raw = r#"{"plain":"hello","html":"<p>hello</p>"}"#;
        let body: OutboxBody = serde_json::from_str(raw).expect("parse");
        assert_eq!(body.plain, "hello");
        assert_eq!(body.html, "<p>hello</p>");
    }
}
