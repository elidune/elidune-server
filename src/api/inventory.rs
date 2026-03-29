//! Inventory / stocktaking endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::AppResult,
    models::inventory::{
        BatchScanBarcodes, CreateInventorySession, InventoryMissingRow, InventoryReport,
        InventoryScan, InventorySession, InventoryStatus, ScanBarcode,
    },
    services::audit,
};

use super::{biblios::PaginatedResponse, StaffUser};

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/inventory/sessions", get(list_sessions).post(create_session))
        .route("/inventory/sessions/:id", get(get_session))
        .route("/inventory/sessions/:id/close", post(close_session))
        .route("/inventory/sessions/:id/scan", post(scan_barcode))
        .route("/inventory/sessions/:id/scans/batch", post(batch_scan))
        .route("/inventory/sessions/:id/scans", get(list_scans))
        .route("/inventory/sessions/:id/missing", get(list_missing))
        .route("/inventory/sessions/:id/report", get(get_report))
}

/// Query for `GET /inventory/sessions`.
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListInventorySessionsQuery {
    /// Page number (1-based, default 1)
    pub page: Option<i64>,
    /// Page size (default 50, max 200)
    pub per_page: Option<i64>,
    /// Filter by `open` or `closed`
    pub status: Option<InventoryStatus>,
}

/// Query for paginated scan / missing lists.
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListInventoryPageQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// List inventory sessions (paginated).
#[utoipa::path(
    get,
    path = "/inventory/sessions",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(ListInventorySessionsQuery),
    responses(
        (status = 200, description = "Paginated sessions", body = PaginatedResponse<InventorySession>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Staff access required", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_sessions(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Query(query): Query<ListInventorySessionsQuery>,
) -> AppResult<Json<PaginatedResponse<InventorySession>>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 200);
    let (items, total) = state
        .services
        .inventory
        .list_sessions_page(page, per_page, query.status)
        .await?;
    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

/// Create a new inventory session
#[utoipa::path(
    post,
    path = "/inventory/sessions",
    tag = "inventory",
    security(("bearer_auth" = [])),
    request_body = CreateInventorySession,
    responses(
        (status = 201, description = "Session created", body = InventorySession),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Staff access required", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_session(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Json(req): Json<CreateInventorySession>,
) -> AppResult<(StatusCode, Json<InventorySession>)> {
    let session = state
        .services
        .inventory
        .create_session(
            &req.name,
            req.location_filter.as_deref(),
            req.notes.as_deref(),
            req.scope_place,
            Some(claims.user_id),
        )
        .await?;
    state.services.audit.log(
        audit::event::INVENTORY_SESSION_CREATED,
        Some(claims.user_id),
        Some("inventory_session"),
        Some(session.id),
        None,
        None::<()>,
    );
    Ok((StatusCode::CREATED, Json(session)))
}

/// Get session details
#[utoipa::path(
    get,
    path = "/inventory/sessions/{id}",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    responses(
        (status = 200, description = "Session details", body = InventorySession),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_session(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<Json<InventorySession>> {
    Ok(Json(state.services.inventory.get_session(id).await?))
}

/// Close a session (no more scans accepted)
#[utoipa::path(
    post,
    path = "/inventory/sessions/{id}/close",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    responses(
        (status = 200, description = "Session closed", body = InventorySession),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Staff access required", body = crate::error::ErrorResponse),
        (status = 404, description = "Open session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn close_session(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<Json<InventorySession>> {
    let session = state.services.inventory.close_session(id).await?;
    state.services.audit.log(
        audit::event::INVENTORY_SESSION_CLOSED,
        Some(claims.user_id),
        Some("inventory_session"),
        Some(id),
        None,
        None::<()>,
    );
    Ok(Json(session))
}

/// Scan a barcode in an open session
#[utoipa::path(
    post,
    path = "/inventory/sessions/{id}/scan",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    request_body = ScanBarcode,
    responses(
        (status = 201, description = "Barcode recorded", body = InventoryScan),
        (status = 400, description = "Session is closed", body = crate::error::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn scan_barcode(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
    Json(req): Json<ScanBarcode>,
) -> AppResult<(StatusCode, Json<InventoryScan>)> {
    let session = state.services.inventory.get_session(id).await?;
    if session.status != InventoryStatus::Open {
        return Err(crate::error::AppError::BadRequest(
            "Session is closed — cannot scan".to_string(),
        ));
    }
    let scan = state
        .services
        .inventory
        .scan_barcode(id, &req.barcode, Some(claims.user_id))
        .await?;
    Ok((StatusCode::CREATED, Json(scan)))
}

/// Batch scan barcodes (open session only, max `INVENTORY_BATCH_MAX_BARCODES`).
#[utoipa::path(
    post,
    path = "/inventory/sessions/{id}/scans/batch",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    request_body = BatchScanBarcodes,
    responses(
        (status = 201, description = "Scans recorded", body = Vec<InventoryScan>),
        (status = 400, description = "Session closed or batch too large", body = crate::error::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn batch_scan(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
    Json(req): Json<BatchScanBarcodes>,
) -> AppResult<(StatusCode, Json<Vec<InventoryScan>>)> {
    let session = state.services.inventory.get_session(id).await?;
    if session.status != InventoryStatus::Open {
        return Err(crate::error::AppError::BadRequest(
            "Session is closed — cannot scan".to_string(),
        ));
    }
    let scans = state
        .services
        .inventory
        .scan_barcodes_batch(id, &req.barcodes, Some(claims.user_id))
        .await?;
    Ok((StatusCode::CREATED, Json(scans)))
}

/// Get scans for a session (paginated, oldest first).
#[utoipa::path(
    get,
    path = "/inventory/sessions/{id}/scans",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Session ID"),
        ListInventoryPageQuery
    ),
    responses(
        (status = 200, description = "Paginated scans", body = PaginatedResponse<InventoryScan>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_scans(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Path(id): Path<i64>,
    Query(query): Query<ListInventoryPageQuery>,
) -> AppResult<Json<PaginatedResponse<InventoryScan>>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 200);
    let (items, total) = state
        .services
        .inventory
        .list_scans_page(id, page, per_page)
        .await?;
    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

/// Missing items in session scope (never appeared as `itemId` on a scan).
#[utoipa::path(
    get,
    path = "/inventory/sessions/{id}/missing",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Session ID"),
        ListInventoryPageQuery
    ),
    responses(
        (status = 200, description = "Paginated missing copies", body = PaginatedResponse<InventoryMissingRow>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_missing(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Path(id): Path<i64>,
    Query(query): Query<ListInventoryPageQuery>,
) -> AppResult<Json<PaginatedResponse<InventoryMissingRow>>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 200);
    let (items, total) = state
        .services
        .inventory
        .list_missing_page(id, page, per_page)
        .await?;
    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

/// Get discrepancy report for a session
#[utoipa::path(
    get,
    path = "/inventory/sessions/{id}/report",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    responses(
        (status = 200, description = "Discrepancy report", body = InventoryReport),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_report(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<Json<InventoryReport>> {
    Ok(Json(state.services.inventory.report(id).await?))
}
