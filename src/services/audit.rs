//! Audit log service for recording all mutations, auth events, and system events.
//!
//! Uses a fire-and-forget pattern so logging never blocks the calling handler.
//! Sensitive fields are stripped from payloads before insertion.

use std::net::SocketAddr;

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::{
    error::AppError,
    error::AppResult,
    repository::Repository,
};

const AUDIT_MESSAGE_MAX_BYTES: usize = 1000;

/// Outcome stored on each audit row (`audit_log.outcome`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditOutcome {
    Success,
    Failure,
}

impl AuditOutcome {
    fn as_db_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

/// HTTP / API error context for an audit row (failure rows populate error fields).
#[derive(Clone, Debug)]
pub struct AuditLogMeta {
    pub outcome: AuditOutcome,
    pub http_status: Option<i16>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl AuditLogMeta {
    pub fn success() -> Self {
        Self {
            outcome: AuditOutcome::Success,
            http_status: None,
            error_code: None,
            error_message: None,
        }
    }

    pub fn from_app_error(err: &AppError) -> Self {
        let (st, code, msg) = err.audit_http_fields();
        Self {
            outcome: AuditOutcome::Failure,
            http_status: Some(st as i16),
            error_code: Some(code.to_string()),
            error_message: Some(truncate_audit_message(msg)),
        }
    }

    /// Non-HTTP failures (background jobs, email transport) with a stable code.
    pub fn failure_background(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            outcome: AuditOutcome::Failure,
            http_status: None,
            error_code: Some(error_code.into()),
            error_message: Some(truncate_audit_message(message.into())),
        }
    }
}

fn truncate_audit_message(mut s: String) -> String {
    if s.len() > AUDIT_MESSAGE_MAX_BYTES {
        s.truncate(AUDIT_MESSAGE_MAX_BYTES);
        s.push('…');
    }
    s
}

/// Known audit event type constants (use these instead of raw strings)
pub mod event {
    // Users
    pub const USER_CREATED: &str = "user.created";
    pub const USER_UPDATED: &str = "user.updated";
    pub const USER_DELETED: &str = "user.deleted";
    pub const USER_ACCOUNT_TYPE_CHANGED: &str = "user.account_type_changed";
    pub const ACCOUNT_TYPE_UPDATED: &str = "account_type.updated";

    // Biblios
    pub const BIBLIO_CREATED: &str = "biblio.created";
    pub const BIBLIO_UPDATED: &str = "biblio.updated";
    pub const BIBLIO_DELETED: &str = "biblio.deleted";

    // Items
    pub const ITEM_CREATED: &str = "item.created";
    pub const ITEM_UPDATED: &str = "item.updated";
    pub const ITEM_DELETED: &str = "item.deleted";

    // Loans
    pub const LOAN_CREATED: &str = "loan.created";
    pub const LOAN_RETURNED: &str = "loan.returned";
    pub const LOAN_RENEWED: &str = "loan.renewed";

    // Sources
    pub const SOURCE_CREATED: &str = "source.created";
    pub const SOURCE_UPDATED: &str = "source.updated";
    pub const SOURCE_ARCHIVED: &str = "source.archived";
    pub const SOURCE_MERGED: &str = "source.merged";

    // Equipment
    pub const EQUIPMENT_CREATED: &str = "equipment.created";
    pub const EQUIPMENT_UPDATED: &str = "equipment.updated";
    pub const EQUIPMENT_DELETED: &str = "equipment.deleted";

    // Cultural events
    pub const EVENT_CREATED: &str = "event.created";
    pub const EVENT_UPDATED: &str = "event.updated";
    pub const EVENT_DELETED: &str = "event.deleted";
    pub const EVENT_ANNOUNCEMENT_SENT: &str = "event.announcement_sent";

    // Public types
    pub const PUBLIC_TYPE_CREATED: &str = "public_type.created";
    pub const PUBLIC_TYPE_UPDATED: &str = "public_type.updated";
    pub const PUBLIC_TYPE_DELETED: &str = "public_type.deleted";
    pub const PUBLIC_TYPE_LOAN_SETTINGS_UPDATED: &str = "public_type.loan_settings_updated";

    // Schedules
    pub const SCHEDULE_PERIOD_CREATED: &str = "schedule.period_created";
    pub const SCHEDULE_PERIOD_UPDATED: &str = "schedule.period_updated";
    pub const SCHEDULE_PERIOD_DELETED: &str = "schedule.period_deleted";
    pub const SCHEDULE_SLOT_CREATED: &str = "schedule.slot_created";
    pub const SCHEDULE_SLOT_DELETED: &str = "schedule.slot_deleted";
    pub const SCHEDULE_CLOSURE_CREATED: &str = "schedule.closure_created";
    pub const SCHEDULE_CLOSURE_DELETED: &str = "schedule.closure_deleted";

    // Visitor counts
    pub const VISITOR_COUNT_CREATED: &str = "visitor_count.created";
    pub const VISITOR_COUNT_DELETED: &str = "visitor_count.deleted";

    // Settings
    pub const SETTINGS_UPDATED: &str = "settings.updated";
    /// One-time initial library setup (first admin, library info, optional email override)
    pub const FIRST_SETUP_COMPLETED: &str = "system.first_setup_completed";

    // Library info
    pub const LIBRARY_INFO_UPDATED: &str = "library_info.updated";

    // Email
    pub const EMAIL_OVERDUE_REMINDER_SENT: &str = "email.overdue_reminder_sent";
    pub const EMAIL_2FA_CODE_SENT: &str = "email.2fa_code_sent";
    pub const EMAIL_RECOVERY_CODE_SENT: &str = "email.recovery_code_sent";
    pub const EMAIL_PASSWORD_RESET_SENT: &str = "email.password_reset_sent";
    pub const EMAIL_TEST_SENT: &str = "email.test_sent";
    pub const EMAIL_TEMPLATE_UPDATED: &str = "email_template.updated";

    // Auth
    pub const AUTH_LOGIN_SUCCESS: &str = "auth.login_success";
    pub const AUTH_LOGIN_FAILED: &str = "auth.login_failed";
    pub const AUTH_2FA_VERIFIED: &str = "auth.2fa_verified";
    pub const AUTH_2FA_FAILED: &str = "auth.2fa_failed";
    pub const AUTH_PASSWORD_RESET_REQUESTED: &str = "auth.password_reset_requested";
    pub const AUTH_PASSWORD_CHANGED: &str = "auth.password_changed";
    pub const AUTH_2FA_ENABLED: &str = "auth.2fa_enabled";
    pub const AUTH_2FA_DISABLED: &str = "auth.2fa_disabled";

    // Config
    pub const CONFIG_SECTION_UPDATED: &str = "config.section_updated";
    pub const CONFIG_SECTION_RESET: &str = "config.section_reset";

    // Import
    pub const IMPORT_MARC_BATCH: &str = "import.marc_batch";
    pub const IMPORT_Z3950_RECORD: &str = "import.z3950_record";

    // Holds
    pub const HOLD_CREATED: &str = "hold.created";
    pub const HOLD_CANCELLED: &str = "hold.cancelled";
    pub const HOLD_FULFILLED: &str = "hold.fulfilled";

    // Fines
    pub const FINE_CREATED: &str = "fine.created";
    pub const FINE_PAID: &str = "fine.paid";
    pub const FINE_WAIVED: &str = "fine.waived";

    // Inventory
    pub const INVENTORY_SESSION_CREATED: &str = "inventory.session_created";
    pub const INVENTORY_SESSION_CLOSED: &str = "inventory.session_closed";

    // Maintenance
    pub const MAINTENANCE_RUN: &str = "maintenance.run";
    pub const MAINTENANCE_DATABASE_DUMP: &str = "maintenance.database_dump";
    pub const MAINTENANCE_DATABASE_RESTORE: &str = "maintenance.database_restore";

    // Reader assistant
    pub const READER_ASSISTANT_SESSION_CREATED: &str = "reader_assistant.session_created";
    pub const READER_ASSISTANT_MESSAGE: &str = "reader_assistant.message";
    pub const READER_ASSISTANT_SESSION_DELETED: &str = "reader_assistant.session_deleted";

    // System
    pub const SYSTEM_STARTUP: &str = "system.startup";
    pub const SYSTEM_REMINDERS_BATCH_COMPLETED: &str = "system.reminders_batch_completed";
    pub const SYSTEM_AUDIT_CLEANUP: &str = "system.audit_cleanup";
}

pub use crate::models::audit::{AuditLogEntry, AuditLogPage, AuditQueryParams};

#[derive(Clone)]
pub struct AuditService {
    repository: Repository,
}

impl AuditService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    /// Fire-and-forget audit log insertion. Never blocks the caller.
    /// Payload is JSON-serialized on the caller stack, then sensitive fields are stripped before insert.
    pub fn log<P: Serialize>(
        &self,
        event_type: &'static str,
        user_id: Option<i64>,
        entity_type: Option<&'static str>,
        entity_id: Option<i64>,
        ip_address: Option<String>,
        payload: Option<P>,
        meta: AuditLogMeta,
    ) {
        let repository = self.repository.clone();
        let event_type = event_type.to_string();
        let entity_type: Option<String> = entity_type.map(|s| s.to_string());
        let payload: Option<Value> = payload.and_then(|p| match serde_json::to_value(p) {
            Ok(v) => Some(mask_sensitive_fields(v)),
            Err(e) => {
                tracing::warn!(
                    "audit log payload serialization failed for '{}': {}",
                    event_type,
                    e
                );
                None
            }
        });

        let outcome_db = meta.outcome.as_db_str().to_string();
        let http_st = meta.http_status;
        let err_code = meta.error_code.clone();
        let err_msg = meta.error_message.clone();

        tracing::info!(
            target: "audit",
            event_type = %event_type,
            outcome = %outcome_db,
            http_status = http_st,
            error_code = err_code.as_deref(),
            "audit event"
        );

        tokio::spawn(async move {
            let result = repository
                .audit_insert(
                    &event_type,
                    user_id,
                    entity_type.as_deref(),
                    entity_id,
                    ip_address.as_deref(),
                    payload,
                    &outcome_db,
                    http_st,
                    err_code.as_deref(),
                    err_msg.as_deref(),
                )
                .await;

            if let Err(e) = result {
                tracing::warn!("Failed to write audit log entry '{}': {}", event_type, e);
            }
        });
    }

    /// Query audit log entries with filters and pagination.
    #[tracing::instrument(skip(self), err)]
    pub async fn query(&self, params: AuditQueryParams) -> AppResult<AuditLogPage> {
        self.repository.audit_query_page(params).await
    }

    /// Export audit log entries for a date range (unbounded, for CSV/JSON export).
    #[tracing::instrument(skip(self), err)]
    pub async fn export(
        &self,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        event_type: Option<&str>,
    ) -> AppResult<Vec<AuditLogEntry>> {
        self.repository
            .audit_export(from_date, to_date, event_type)
            .await
    }

    /// Delete audit log entries older than `retention_days` days.
    /// Returns the number of deleted rows.
    #[tracing::instrument(skip(self), err)]
    pub async fn cleanup(&self, retention_days: u32) -> AppResult<u64> {
        self.repository.audit_cleanup(retention_days).await
    }
}

/// Strip sensitive field names from a JSONB payload before storing.
pub fn mask_sensitive_fields(mut value: Value) -> Value {
    const SENSITIVE: &[&str] = &[
        "password",
        "smtp_password",
        "dataBase64",
        "attachmentDataBase64",
        "token",
        "totp_secret",
        "recovery_codes",
        "recovery_codes_used",
        "jwt_secret",
        "new_password",
        "current_password",
    ];

    if let Value::Object(ref mut map) = value {
        for key in SENSITIVE {
            if map.contains_key(*key) {
                map.insert(key.to_string(), Value::String("[redacted]".to_string()));
            }
        }
        // Recurse into nested objects (e.g. old_value/new_value in config events)
        for v in map.values_mut() {
            if v.is_object() {
                *v = mask_sensitive_fields(v.clone());
            }
        }
    }
    value
}

/// X-Forwarded-For (first IP) → X-Real-IP → optional TCP peer address.
pub fn resolve_client_ip(headers: &HeaderMap, peer_addr: Option<SocketAddr>) -> Option<String> {
    extract_client_ip(headers).or_else(|| peer_addr.map(|a| a.ip().to_string()))
}

/// Extract client IP address from request headers, in priority order:
/// X-Forwarded-For (first IP) → X-Real-IP → None
pub fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(s) = xff.to_str() {
            if let Some(first) = s.split(',').next() {
                let ip = first.trim().to_string();
                if !ip.is_empty() {
                    return Some(ip);
                }
            }
        }
    }
    if let Some(xri) = headers.get("x-real-ip") {
        if let Ok(s) = xri.to_str() {
            let ip = s.trim().to_string();
            if !ip.is_empty() {
                return Some(ip);
            }
        }
    }
    None
}
