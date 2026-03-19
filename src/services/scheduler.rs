//! Background scheduler for overdue reminder emails and audit log cleanup.
//!
//! Spawned at startup via `tokio::spawn`. Two periodic tasks run concurrently:
//! - Reminder sending at the configured time of day
//! - Audit log cleanup at 03:00 daily

use std::sync::Arc;

use chrono::{Local, NaiveTime, Timelike};
use tokio::sync::Notify;
use tokio::time::Duration;

use crate::{
    dynamic_config::DynamicConfig,
    services::{audit, reminders::RemindersService, audit::AuditService},
};

/// Start the background scheduler. Returns a `Notify` handle that can be used
/// to wake up the reminder task early (e.g. after a config change).
pub fn spawn(
    dynamic_config: Arc<DynamicConfig>,
    reminders_service: RemindersService,
    audit_service: AuditService,
) -> Arc<Notify> {
    let notify = Arc::new(Notify::new());

    // Reminder sending task
    let notify_clone = notify.clone();
    let dc_rem = dynamic_config.clone();
    let rem_svc = reminders_service.clone();
    let audit_rem = audit_service.clone();

    tokio::spawn(async move {
        tracing::info!("Reminder scheduler started");
        loop {
            let cfg = dc_rem.read_reminders();

            if !cfg.enabled {
                tracing::debug!("Reminders disabled, sleeping 60s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                    _ = notify_clone.notified() => {
                        tracing::info!("Reminder scheduler woken by config change");
                    }
                }
                continue;
            }

            let sleep_dur = duration_until_next_send(&cfg.send_time);
            tracing::info!(
                "Next reminder run in {:.1} minutes (at {})",
                sleep_dur.as_secs_f64() / 60.0,
                cfg.send_time
            );

            tokio::select! {
                _ = tokio::time::sleep(sleep_dur) => {}
                _ = notify_clone.notified() => {
                    tracing::info!("Reminder scheduler woken early by config change, re-evaluating schedule");
                    continue;
                }
            }

            tracing::info!("Running scheduled overdue reminder batch");
            match rem_svc.send_overdue_reminders(false, None, None).await {
                Ok(report) => {
                    tracing::info!(
                        "Reminder batch completed: {} emails sent, {} loans reminded, {} errors",
                        report.emails_sent,
                        report.loans_reminded,
                        report.errors.len()
                    );
                    audit_rem.log(
                        audit::event::SYSTEM_REMINDERS_BATCH_COMPLETED,
                        None,
                        None,
                        None,
                        None,
                        Some(serde_json::json!({
                            "emails_sent": report.emails_sent,
                            "loans_reminded": report.loans_reminded,
                            "errors": report.errors.len(),
                        })),
                    );
                }
                Err(e) => {
                    tracing::error!("Reminder batch failed: {}", e);
                }
            }
        }
    });

    // Audit log cleanup task (runs daily at 03:00)
    let dc_audit = dynamic_config.clone();
    let audit_cleanup = audit_service.clone();

    tokio::spawn(async move {
        tracing::info!("Audit cleanup scheduler started");
        loop {
            let sleep_dur = duration_until_next_send("03:00");
            tokio::time::sleep(sleep_dur).await;

            let cfg = dc_audit.read_audit();
            tracing::info!("Running audit log cleanup (retention: {} days)", cfg.retention_days);

            match audit_cleanup.cleanup(cfg.retention_days).await {
                Ok(deleted) => {
                    tracing::info!("Audit cleanup: {} entries deleted", deleted);
                    audit_cleanup.log(
                        audit::event::SYSTEM_AUDIT_CLEANUP,
                        None,
                        None,
                        None,
                        None,
                        Some(serde_json::json!({
                            "deleted_count": deleted,
                            "retention_days": cfg.retention_days,
                        })),
                    );
                }
                Err(e) => {
                    tracing::error!("Audit cleanup failed: {}", e);
                }
            }
        }
    });

    notify
}

/// Calculate duration until next occurrence of `send_time` (HH:MM, local time).
/// If the time has already passed today, schedules for tomorrow.
fn duration_until_next_send(send_time: &str) -> Duration {
    let parts: Vec<&str> = send_time.split(':').collect();
    let hour: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(9);
    let minute: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    let now = Local::now();
    let target = NaiveTime::from_hms_opt(hour, minute, 0)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).unwrap());
    let now_time = now.time();

    let secs_until = if now_time < target {
        let diff = target - now_time;
        diff.num_seconds() as u64
    } else {
        // Already passed today — schedule for tomorrow
        let seconds_remaining_today =
            86400 - (now_time.num_seconds_from_midnight() as u64);
        let secs_from_midnight = (target.num_seconds_from_midnight()) as u64;
        seconds_remaining_today + secs_from_midnight
    };

    // Minimum 1 second to avoid spin loop
    Duration::from_secs(secs_until.max(1))
}
