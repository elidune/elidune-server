//! Background worker for the `email_outbox` table.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

use crate::{
    email::EmailService,
    error::{AppError, AppResult},
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
}

/// Claim and send up to `batch_size` pending outbox rows.
pub async fn process_outbox_batch(
    email: &EmailService,
    pool: &Pool<Postgres>,
    batch_size: Option<i64>,
) -> AppResult<EmailOutboxBatchReport> {
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
                mark_failed(pool, row.id, row.attempts, &format!("invalid body JSON: {e}")).await?;
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
            }
            Err(e) => {
                let next_attempts = row.attempts + 1;
                if next_attempts >= MAX_ATTEMPTS {
                    mark_failed(pool, row.id, row.attempts, &e.to_string()).await?;
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

async fn mark_failed(pool: &Pool<Postgres>, id: i64, attempts: i32, reason: &str) -> AppResult<()> {
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
    tracing::error!(outbox_id = id, reason, "email outbox row marked failed");
    Ok(())
}
