//! Inventory / stocktaking endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::{AppError, AppResult},
    models::{
        inventory::{
            BatchScanBarcodes, CreateInventorySession, InventoryMissingRow, InventoryReport,
            InventoryScan, InventorySession, InventoryStatus, ScanBarcode,
        },
        task::TaskKind,
    },
    services::{audit, inventory::INVENTORY_BATCH_MAX_BARCODES},
};

use super::{biblios::PaginatedResponse, tasks::TaskAcceptedResponse, StaffUser};

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
     audit::AuditLogMeta::success());
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
     audit::AuditLogMeta::success());
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
///
/// Returns `202 Accepted` with a `taskId`. Poll `GET /tasks/:id` until
/// `status` is `completed` or `failed`. On success, `result` is `InventoryScan[]` in request order.
#[utoipa::path(
    post,
    path = "/inventory/sessions/{id}/scans/batch",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    request_body = BatchScanBarcodes,
    responses(
        (status = 202, description = "Batch accepted; poll GET /tasks/:id", body = TaskAcceptedResponse),
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
) -> AppResult<(StatusCode, Json<TaskAcceptedResponse>)> {
    let session = state.services.inventory.get_session(id).await?;
    if session.status != InventoryStatus::Open {
        return Err(AppError::BadRequest(
            "Session is closed — cannot scan".to_string(),
        ));
    }
    if req.barcodes.len() > INVENTORY_BATCH_MAX_BARCODES {
        return Err(AppError::Validation(format!(
            "At most {} barcodes per batch",
            INVENTORY_BATCH_MAX_BARCODES
        )));
    }

    let inventory = state.services.inventory.clone();
    let tasks = state.services.tasks.clone();
    let session_id = id;
    let barcodes = req.barcodes;
    let scanned_by = Some(claims.user_id);
    let user_id = claims.user_id;

    let task_id = tasks.spawn_task(TaskKind::InventoryBatchScan, user_id, move |handle| async move {
        let total = barcodes.len();
        let mut scans: Vec<InventoryScan> = Vec::with_capacity(total);
        for (i, b) in barcodes.iter().enumerate() {
            match inventory
                .scan_barcode(session_id, b, scanned_by)
                .await
            {
                Ok(scan) => {
                    scans.push(scan);
                    handle.set_progress(i + 1, total, None).await;
                }
                Err(e) => {
                    handle.fail(e.to_string()).await;
                    return;
                }
            }
        }
        let result = serde_json::to_value(&scans).unwrap_or_default();
        handle.complete(result).await;
    });

    Ok((StatusCode::ACCEPTED, Json(TaskAcceptedResponse { task_id })))
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
