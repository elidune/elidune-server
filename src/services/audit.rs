//! Audit log service for recording all mutations, auth events, and system events.
//!
//! Uses a fire-and-forget pattern so logging never blocks the calling handler.
//! Sensitive fields are stripped from payloads before insertion.

use std::net::SocketAddr;

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres, Row};
use utoipa::ToSchema;

use crate::error::AppResult;

/// Known audit event type constants (use these instead of raw strings)
pub mod event {
    // Users
    pub const USER_CREATED: &str = "user.created";
    pub const USER_UPDATED: &str = "user.updated";
    pub const USER_DELETED: &str = "user.deleted";
    pub const USER_ACCOUNT_TYPE_CHANGED: &str = "user.account_type_changed";

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
    pub const PUBLIC_TYPE_LOAN_SETTING_UPDATED: &str = "public_type.loan_setting_updated";
    pub const PUBLIC_TYPE_LOAN_SETTING_DELETED: &str = "public_type.loan_setting_deleted";

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

    // Library info
    pub const LIBRARY_INFO_UPDATED: &str = "library_info.updated";

    // Email
    pub const EMAIL_OVERDUE_REMINDER_SENT: &str = "email.overdue_reminder_sent";
    pub const EMAIL_2FA_CODE_SENT: &str = "email.2fa_code_sent";
    pub const EMAIL_RECOVERY_CODE_SENT: &str = "email.recovery_code_sent";
    pub const EMAIL_PASSWORD_RESET_SENT: &str = "email.password_reset_sent";
    pub const EMAIL_TEST_SENT: &str = "email.test_sent";

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

    // History / GDPR
    pub const HISTORY_OPT_IN: &str = "history.opt_in";
    pub const HISTORY_OPT_OUT: &str = "history.opt_out";

    // Reservations
    pub const RESERVATION_CREATED: &str = "reservation.created";
    pub const RESERVATION_CANCELLED: &str = "reservation.cancelled";
    pub const RESERVATION_FULFILLED: &str = "reservation.fulfilled";

    // Fines
    pub const FINE_CREATED: &str = "fine.created";
    pub const FINE_PAID: &str = "fine.paid";
    pub const FINE_WAIVED: &str = "fine.waived";

    // Inventory
    pub const INVENTORY_SESSION_CREATED: &str = "inventory.session_created";
    pub const INVENTORY_SESSION_CLOSED: &str = "inventory.session_closed";

    // Maintenance
    pub const MAINTENANCE_RUN: &str = "maintenance.run";

    // System
    pub const SYSTEM_STARTUP: &str = "system.startup";
    pub const SYSTEM_REMINDERS_BATCH_COMPLETED: &str = "system.reminders_batch_completed";
    pub const SYSTEM_AUDIT_CLEANUP: &str = "system.audit_cleanup";
}

/// A single audit log entry returned from queries
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub id: i64,
    pub event_type: String,
    pub user_id: Option<i64>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub ip_address: Option<String>,
    pub payload: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for audit log pagination and filtering
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditQueryParams {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub user_id: Option<i64>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Paginated audit log response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogPage {
    pub entries: Vec<AuditLogEntry>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Clone)]
pub struct AuditService {
    pool: Pool<Postgres>,
}

impl AuditService {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
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
    ) {
        let pool = self.pool.clone();
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

        tokio::spawn(async move {
            let result = sqlx::query(
                r#"
                INSERT INTO audit_log (event_type, user_id, entity_type, entity_id, ip_address, payload)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(&event_type)
            .bind(user_id)
            .bind(entity_type.as_deref())
            .bind(entity_id)
            .bind(ip_address.as_deref())
            .bind(payload)
            .execute(&pool)
            .await;

            if let Err(e) = result {
                tracing::warn!("Failed to write audit log entry '{}': {}", event_type, e);
            }
        });
    }

    /// Query audit log entries with filters and pagination.
    #[tracing::instrument(skip(self), err)]
    pub async fn query(&self, params: AuditQueryParams) -> AppResult<AuditLogPage> {
        let page = params.page.unwrap_or(1).max(1);
        let per_page = params.per_page.unwrap_or(50).clamp(1, 500);
        let offset = (page - 1) * per_page;

        let mut conditions: Vec<String> = Vec::new();
        let mut bind_idx = 1usize;

        if params.event_type.is_some() {
            conditions.push(format!("event_type = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.entity_type.is_some() {
            conditions.push(format!("entity_type = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.entity_id.is_some() {
            conditions.push(format!("entity_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.user_id.is_some() {
            conditions.push(format!("user_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.from_date.is_some() {
            conditions.push(format!("created_at >= ${}", bind_idx));
            bind_idx += 1;
        }
        if params.to_date.is_some() {
            conditions.push(format!("created_at <= ${}", bind_idx));
            bind_idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM audit_log {}", where_clause);
        let data_sql = format!(
            "SELECT id, event_type, user_id, entity_type, entity_id, ip_address, payload, created_at \
             FROM audit_log {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            where_clause, bind_idx, bind_idx + 1,
        );

        // Bind count query
        let mut cq = sqlx::query_scalar::<sqlx::Postgres, i64>(&count_sql);
        if let Some(ref v) = params.event_type { cq = cq.bind(v.clone()); }
        if let Some(ref v) = params.entity_type { cq = cq.bind(v.clone()); }
        if let Some(v) = params.entity_id { cq = cq.bind(v); }
        if let Some(v) = params.user_id { cq = cq.bind(v); }
        if let Some(v) = params.from_date { cq = cq.bind(v); }
        if let Some(v) = params.to_date { cq = cq.bind(v); }

        let total: i64 = cq.fetch_one(&self.pool).await?;

        // Bind data query
        let mut dq = sqlx::query(&data_sql);
        if let Some(ref v) = params.event_type { dq = dq.bind(v.clone()); }
        if let Some(ref v) = params.entity_type { dq = dq.bind(v.clone()); }
        if let Some(v) = params.entity_id { dq = dq.bind(v); }
        if let Some(v) = params.user_id { dq = dq.bind(v); }
        if let Some(v) = params.from_date { dq = dq.bind(v); }
        if let Some(v) = params.to_date { dq = dq.bind(v); }
        dq = dq.bind(per_page).bind(offset);

        let rows = dq.fetch_all(&self.pool).await?;

        let entries = rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                event_type: row.get("event_type"),
                user_id: row.get("user_id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                ip_address: row.get("ip_address"),
                payload: row.get("payload"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(AuditLogPage {
            entries,
            total,
            page,
            per_page,
        })
    }

    /// Export audit log entries for a date range (unbounded, for CSV/JSON export).
    #[tracing::instrument(skip(self), err)]
    pub async fn export(
        &self,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        event_type: Option<&str>,
    ) -> AppResult<Vec<AuditLogEntry>> {
        let mut conditions = Vec::new();
        if from_date.is_some() { conditions.push("created_at >= $1"); }
        if to_date.is_some() {
            conditions.push(if from_date.is_some() { "created_at <= $2" } else { "created_at <= $1" });
        }
        if event_type.is_some() {
            let idx = from_date.is_some() as usize + to_date.is_some() as usize + 1;
            conditions.push(Box::leak(format!("event_type = ${}", idx).into_boxed_str()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, event_type, user_id, entity_type, entity_id, ip_address, payload, created_at \
             FROM audit_log {} ORDER BY created_at DESC LIMIT 50000",
            where_clause
        );

        let mut q = sqlx::query(&sql);
        if let Some(v) = from_date { q = q.bind(v); }
        if let Some(v) = to_date { q = q.bind(v); }
        if let Some(v) = event_type { q = q.bind(v); }

        let rows = q.fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                event_type: row.get("event_type"),
                user_id: row.get("user_id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                ip_address: row.get("ip_address"),
                payload: row.get("payload"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// Delete audit log entries older than `retention_days` days.
    /// Returns the number of deleted rows.
    #[tracing::instrument(skip(self), err)]
    pub async fn cleanup(&self, retention_days: u32) -> AppResult<u64> {
        let deleted = sqlx::query_scalar::<_, i64>(
            "WITH deleted AS (DELETE FROM audit_log WHERE created_at < NOW() - ($1 || ' days')::INTERVAL RETURNING id) SELECT COUNT(*) FROM deleted"
        )
        .bind(retention_days as i64)
        .fetch_one(&self.pool)
        .await?;

        Ok(deleted as u64)
    }
}

/// Strip sensitive field names from a JSONB payload before storing.
pub fn mask_sensitive_fields(mut value: Value) -> Value {
    const SENSITIVE: &[&str] = &[
        "password",
        "smtp_password",
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
