//! Inventory / stocktaking endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use utoipa::ToSchema;

use crate::{
    error::AppResult,
    models::inventory::{
        CreateInventorySession, InventoryReport, InventorySession, InventoryScan, ScanBarcode,
    },
    services::audit,
};

use super::StaffUser;

/// List all inventory sessions
#[utoipa::path(
    get,
    path = "/inventory/sessions",
    tag = "inventory",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of inventory sessions", body = Vec<InventorySession>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Staff access required", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_sessions(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
) -> AppResult<Json<Vec<InventorySession>>> {
    Ok(Json(state.services.inventory.list_sessions().await?))
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
            Some(claims.user_id),
        )
        .await?;
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
    StaffUser(_staff): StaffUser,
    Path(id): Path<i64>,
    Json(req): Json<ScanBarcode>,
) -> AppResult<(StatusCode, Json<InventoryScan>)> {
    // Verify session is open
    let session = state.services.inventory.get_session(id).await?;
    if session.status != crate::models::inventory::InventoryStatus::Open {
        return Err(crate::error::AppError::BadRequest(
            "Session is closed — cannot scan".to_string(),
        ));
    }
    let scan = state.services.inventory.scan_barcode(id, &req.barcode).await?;
    Ok((StatusCode::CREATED, Json(scan)))
}

/// Get all scans for a session
#[utoipa::path(
    get,
    path = "/inventory/sessions/{id}/scans",
    tag = "inventory",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Session ID")),
    responses(
        (status = 200, description = "List of scans", body = Vec<InventoryScan>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_scans(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<InventoryScan>>> {
    Ok(Json(state.services.inventory.list_scans(id).await?))
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

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/inventory/sessions", get(list_sessions).post(create_session))
        .route("/inventory/sessions/{id}", get(get_session))
        .route("/inventory/sessions/{id}/close", post(close_session))
        .route("/inventory/sessions/{id}/scan", post(scan_barcode))
        .route("/inventory/sessions/{id}/scans", get(list_scans))
        .route("/inventory/sessions/{id}/report", get(get_report))
}
