//! Audit log API endpoints

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::AppResult,
    services::audit::{AuditLogPage, AuditQueryParams},
    AppState,
};

use super::AuthenticatedUser;

/// Query parameters for audit log
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct AuditQueryRequest {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub user_id: Option<i64>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Query parameters for audit log export
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct AuditExportRequest {
    pub format: Option<String>,
    pub event_type: Option<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
}

/// Get paginated audit log entries (admin only)
#[utoipa::path(
    get,
    path = "/audit",
    tag = "audit",
    security(("bearer_auth" = [])),
    params(AuditQueryRequest),
    responses(
        (status = 200, description = "Paginated audit log entries", body = AuditLogPage),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn get_audit_log(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<AuditQueryRequest>,
) -> AppResult<Json<AuditLogPage>> {
    claims.require_admin()?;

    let params = AuditQueryParams {
        event_type: query.event_type,
        entity_type: query.entity_type,
        entity_id: query.entity_id,
        user_id: query.user_id,
        from_date: query.from_date,
        to_date: query.to_date,
        page: query.page,
        per_page: query.per_page,
    };

    let page = state.services.audit.query(params).await?;
    Ok(Json(page))
}

/// Export audit log entries as JSON or CSV (admin only)
#[utoipa::path(
    get,
    path = "/audit/export",
    tag = "audit",
    security(("bearer_auth" = [])),
    params(AuditExportRequest),
    responses(
        (status = 200, description = "Audit log export (JSON array or CSV)"),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn export_audit_log(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<AuditExportRequest>,
) -> AppResult<Response> {
    claims.require_admin()?;

    let entries = state.services.audit.export(
        query.from_date,
        query.to_date,
        query.event_type.as_deref(),
    ).await?;

    let format = query.format.as_deref().unwrap_or("json");

    if format == "csv" {
        let mut csv = String::from("id,event_type,user_id,entity_type,entity_id,ip_address,payload,created_at\n");
        for e in &entries {
            let payload_str = e.payload.as_ref()
                .map(|v| v.to_string().replace('"', "\"\""))
                .unwrap_or_default();
            csv.push_str(&format!(
                "{},{},{},{},{},{},\"{}\",{}\n",
                e.id,
                e.event_type,
                e.user_id.map(|v| v.to_string()).unwrap_or_default(),
                e.entity_type.as_deref().unwrap_or(""),
                e.entity_id.map(|v| v.to_string()).unwrap_or_default(),
                e.ip_address.as_deref().unwrap_or(""),
                payload_str,
                e.created_at.to_rfc3339(),
            ));
        }
        Ok((
            [
                ("content-type", "text/csv"),
                ("content-disposition", "attachment; filename=\"audit_log.csv\""),
            ],
            csv,
        )
            .into_response())
    } else {
        Ok(Json(entries).into_response())
    }
}
