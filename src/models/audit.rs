//! Audit log API and query types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// A single audit log entry returned from queries.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub id: i64,
    pub event_type: String,
    /// `"success"` or `"failure"` — whether the audited action completed successfully.
    pub outcome: String,
    pub user_id: Option<i64>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub ip_address: Option<String>,
    pub payload: Option<Value>,
    /// HTTP status that would have been returned for API-originated actions (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<i16>,
    /// Machine-readable code (same family as [`crate::error::error_code`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for audit log pagination and filtering.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditQueryParams {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub user_id: Option<i64>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    /// Filter: `success` or `failure`.
    pub outcome: Option<String>,
    pub error_code: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Paginated audit log response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogPage {
    pub entries: Vec<AuditLogEntry>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
