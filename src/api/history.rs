//! Patron borrowing history with GDPR controls

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::AppResult,
    models::loan::LoanDetails,
    services::audit,
};

use super::{biblios::PaginatedResponse, AuthenticatedUser, ClientIp};

/// GDPR history preference update request
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHistoryPreference {
    /// When false, loan records are anonymised on return (patron opts out of history)
    pub enabled: bool,
}

/// Patron history preference
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPreference {
    pub user_id: String,
    pub history_enabled: bool,
}

/// Query for paginated borrowing history
#[derive(Debug, Deserialize, Default, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    /// Page number (1-based, default 1)
    pub page: Option<i64>,
    /// Page size (default 20, max 200)
    pub per_page: Option<i64>,
}

/// Get a user's borrowing history (returned loans), paginated
#[utoipa::path(
    get,
    path = "/users/{id}/history",
    tag = "history",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "User ID"),
        HistoryQuery
    ),
    responses(
        (status = 200, description = "Borrowing history", body = PaginatedResponse<LoanDetails>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Access denied", body = crate::error::ErrorResponse),
        (status = 404, description = "User not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_history(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
    Query(query): Query<HistoryQuery>,
) -> AppResult<Json<PaginatedResponse<LoanDetails>>> {
    claims.require_self_or_staff(user_id)?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 200);
    let (items, total) = state
        .services
        .loans
        .get_user_archived_loans(user_id, page, per_page)
        .await?;
    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

/// Get a user's GDPR history preference
#[utoipa::path(
    get,
    path = "/users/{id}/history/preference",
    tag = "history",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "History preference", body = HistoryPreference),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_history_preference(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
) -> AppResult<Json<HistoryPreference>> {
    claims.require_self_or_staff(user_id)?;
    let enabled = state.services.users.get_history_preference(user_id).await?;
    Ok(Json(HistoryPreference {
        user_id: user_id.to_string(),
        history_enabled: enabled,
    }))
}

/// Update a user's GDPR history preference
#[utoipa::path(
    put,
    path = "/users/{id}/history/preference",
    tag = "history",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "User ID")),
    request_body = UpdateHistoryPreference,
    responses(
        (status = 200, description = "Preference updated", body = HistoryPreference),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Access denied", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_history_preference(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(user_id): Path<i64>,
    Json(req): Json<UpdateHistoryPreference>,
) -> AppResult<Json<HistoryPreference>> {
    // GDPR consent changes are restricted to self or admin (librarians cannot override patron consent)
    claims.require_self_or_admin(user_id)?;
    state
        .services
        .users
        .set_history_preference(user_id, req.enabled)
        .await?;

    state.services.audit.log(
        if req.enabled {
            audit::event::HISTORY_OPT_IN
        } else {
            audit::event::HISTORY_OPT_OUT
        },
        Some(claims.user_id),
        Some("user"),
        Some(user_id),
        ip,
        None::<()>,
    );

    Ok(Json(HistoryPreference {
        user_id: user_id.to_string(),
        history_enabled: req.enabled,
    }))
}

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, put};
    axum::Router::new()
        .route("/users/:id/history", get(get_history))
        .route(
            "/users/:id/history/preference",
            get(get_history_preference).put(update_history_preference),
        )
}
