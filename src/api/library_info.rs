//! Library information endpoints

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppResult;
use crate::services::audit;

use super::{AuthenticatedUser, ClientIp};


/// Public GET only — merged under the public API rate limiter in `main.rs`.
pub fn router_public() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new().route("/library-info", get(get_library_info))
}

/// Staff PUT — not subject to the public anonymous rate limiter.
pub fn router_staff() -> axum::Router<crate::AppState> {
    use axum::routing::put;
    axum::Router::new()
        .route("/library-info", put(update_library_info))
}

pub use crate::models::dto::library_info::{LibraryInfo, UpdateLibraryInfoRequest};

/// Get library information (public)
#[utoipa::path(
    get,
    path = "/library-info",
    tag = "library_info",
    responses(
        (status = 200, description = "Library information", body = LibraryInfo),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn get_library_info(
    State(state): State<crate::AppState>,
) -> AppResult<Json<LibraryInfo>> {
    let info = state.services.library_info.get().await?;
    Ok(Json(info))
}

/// Update library information (requires write settings permission)
#[utoipa::path(
    put,
    path = "/library-info",
    tag = "library_info",
    security(("bearer_auth" = [])),
    request_body = UpdateLibraryInfoRequest,
    responses(
        (status = 200, description = "Library information updated", body = LibraryInfo),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn update_library_info(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(request): Json<UpdateLibraryInfoRequest>,
) -> AppResult<Json<LibraryInfo>> {
    claims.require_write_settings()?;

    let info = state.services.library_info.update(request).await?;

    state.services.audit.log(
        audit::event::LIBRARY_INFO_UPDATED,
        Some(claims.user_id),
        None,
        None,
        ip,
        Some(&info),
     audit::AuditLogMeta::success());

    Ok(Json(info))
}

