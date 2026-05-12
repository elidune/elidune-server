//! Visitor counts API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde_json::json;

use crate::{
    error::AppResult,
    models::visitor_count::{CreateVisitorCount, VisitorCount, VisitorCountQuery},
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// List visitor counts
#[utoipa::path(
    get,
    path = "/visitor-counts",
    tag = "visitor_counts",
    security(("bearer_auth" = [])),
    params(VisitorCountQuery),
    responses(
        (status = 200, description = "Visitor counts list", body = Vec<VisitorCount>),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_visitor_counts(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<VisitorCountQuery>,
) -> AppResult<Json<Vec<VisitorCount>>> {
    claims.require_read_settings()?;

    let start = query.start_date.as_ref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let end = query.end_date.as_ref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let counts = state.services.visitor_counts.list(start, end).await?;
    Ok(Json(counts))
}

/// Create a visitor count record
#[utoipa::path(
    post,
    path = "/visitor-counts",
    tag = "visitor_counts",
    security(("bearer_auth" = [])),
    request_body = CreateVisitorCount,
    responses(
        (status = 201, description = "Visitor count created", body = VisitorCount),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn create_visitor_count(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateVisitorCount>,
) -> AppResult<(StatusCode, Json<VisitorCount>)> {
    claims.require_write_settings()?;
    let count = state.services.visitor_counts.create(&data).await?;
    state.services.audit.log(audit::event::VISITOR_COUNT_CREATED, Some(claims.user_id), Some("visitor_count"), Some(count.id), ip, Some((&data, &count)), audit::AuditLogMeta::success());
    Ok((StatusCode::CREATED, Json(count)))
}

/// Delete a visitor count record
#[utoipa::path(
    delete,
    path = "/visitor-counts/{id}",
    tag = "visitor_counts",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Visitor count ID")),
    responses(
        (status = 204, description = "Visitor count deleted"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn delete_visitor_count(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state.services.visitor_counts.delete(id).await?;
    state.services.audit.log(audit::event::VISITOR_COUNT_DELETED, Some(claims.user_id), Some("visitor_count"), Some(id), ip, Some(json!({ "id": id })), audit::AuditLogMeta::success());
    Ok(StatusCode::NO_CONTENT)
}

/// Build the visitor-counts routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        .route("/visitor-counts", get(list_visitor_counts).post(create_visitor_count))
        .route("/visitor-counts/:id", delete(delete_visitor_count))
}
