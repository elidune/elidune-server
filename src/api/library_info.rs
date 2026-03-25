//! Library information endpoints

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppResult;
use crate::services::audit;

use super::{AuthenticatedUser, ClientIp};

/// Library global information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    /// Library name
    pub name: Option<String>,
    /// Street address (number + street)
    pub addr_line1: Option<String>,
    /// Address complement (building, apt, etc.)
    pub addr_line2: Option<String>,
    /// Postal code
    pub addr_postcode: Option<String>,
    /// City
    pub addr_city: Option<String>,
    /// Country
    pub addr_country: Option<String>,
    /// Phone numbers
    pub phones: Vec<String>,
    /// Contact email
    pub email: Option<String>,
    /// Last update timestamp
    pub updated_at: Option<DateTime<Utc>>,
}

/// Update library information request
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLibraryInfoRequest {
    /// Library name
    pub name: Option<String>,
    /// Street address (number + street)
    pub addr_line1: Option<String>,
    /// Address complement (building, apt, etc.)
    pub addr_line2: Option<String>,
    /// Postal code
    pub addr_postcode: Option<String>,
    /// City
    pub addr_city: Option<String>,
    /// Country
    pub addr_country: Option<String>,
    /// Phone numbers (replaces existing list)
    pub phones: Option<Vec<String>>,
    /// Contact email
    pub email: Option<String>,
}

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
    );

    Ok(Json(info))
}

/// Build the library-info routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, put};
    axum::Router::new()
        .route("/library-info", get(get_library_info).put(update_library_info))
}
