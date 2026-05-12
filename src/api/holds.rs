//! Hold endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_with::serde_as;
use serde_with::DisplayFromStr;
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::{AppError, AppResult},
    models::{
        hold::{CreateHold, Hold, HoldDetails},
        user::Rights,
    },
    services::audit,
};

use super::{biblios::PaginatedResponse, AuthenticatedUser, ClientIp};

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get};
    axum::Router::new()
        .route("/holds", get(list_holds).post(create_hold))
        .route("/holds/:id", delete(cancel_hold))
        .route("/items/:id/holds", get(list_holds_for_item))
        .route("/users/:id/holds", get(list_holds_for_user))
}

/// Query parameters for `GET /holds` (global list).
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListHoldsQuery {
    /// Page number (1-based, default 1)
    pub page: Option<i64>,
    /// Page size (default 50, max 200)
    pub per_page: Option<i64>,
    /// If true, only `pending` and `ready` holds (ongoing).
    pub active_only: Option<bool>,
}

/// Paginated list of holds: staff (`holds_rights` read/write) sees all rows; `o` sees only their holds.
#[utoipa::path(
    get,
    path = "/holds",
    tag = "holds",
    security(("bearer_auth" = [])),
    params(ListHoldsQuery),
    responses(
        (status = 200, description = "All holds", body = PaginatedResponse<HoldDetails>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_holds(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<ListHoldsQuery>,
) -> AppResult<Json<PaginatedResponse<HoldDetails>>> {
    claims.require_list_holds()?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 200);
    let active_only = query.active_only.unwrap_or(false);

    let (items, total) = if claims.rights.holds_rights.rank() >= Rights::Read.rank() {
        state.services.holds.list_all(page, per_page, active_only).await?
    } else {
        state
            .services
            .holds
            .list_for_user_paginated(claims.user_id, page, per_page, active_only)
            .await?
    };
    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

#[serde_as]
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateHoldRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub item_id: i64,
    pub notes: Option<String>,
}

#[utoipa::path(
    post,
    path = "/holds",
    tag = "holds",
    security(("bearer_auth" = [])),
    request_body = CreateHoldRequest,
    responses(
        (status = 201, description = "Hold created", body = Hold),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 409, description = "User already has a hold for this item", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_hold(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(req): Json<CreateHoldRequest>,
) -> AppResult<(StatusCode, Json<Hold>)> {
    claims.require_create_hold()?;
    if claims.rights.holds_rights.rank() < Rights::Write.rank() && req.user_id != claims.user_id {
        return Err(AppError::Authorization(
            "Insufficient rights to place a hold for another user".into(),
        ));
    }
    let data = CreateHold {
        user_id: req.user_id,
        item_id: req.item_id,
        notes: req.notes,
    };
    let hold = state.services.holds.place_hold(data).await?;

    state.services.audit.log(
        audit::event::HOLD_CREATED,
        Some(claims.user_id),
        Some("hold"),
        Some(hold.id),
        ip,
        None::<()>,
     audit::AuditLogMeta::success());

    Ok((StatusCode::CREATED, Json(hold)))
}

#[utoipa::path(
    get,
    path = "/items/{id}/holds",
    tag = "holds",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Item ID")),
    responses(
        (status = 200, description = "Hold queue for this item", body = Vec<Hold>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Item not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_holds_for_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(item_id): Path<i64>,
) -> AppResult<Json<Vec<HoldDetails>>> {
    claims.require_read_holds_staff()?;
    let list = state.services.holds.get_for_item(item_id).await?;
    Ok(Json(list))
}

#[utoipa::path(
    get,
    path = "/users/{id}/holds",
    tag = "holds",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "User's holds", body = Vec<HoldDetails>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "User not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_holds_for_user(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
) -> AppResult<Json<Vec<HoldDetails>>> {
    claims.require_read_holds_staff()?;
    claims.require_read_users()?;
    let list = state.services.holds.get_for_user(user_id).await?;
    Ok(Json(list))
}

#[utoipa::path(
    delete,
    path = "/holds/{id}",
    tag = "holds",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Hold ID")),
    responses(
        (status = 200, description = "Hold cancelled", body = Hold),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Cannot cancel another user's hold", body = crate::error::ErrorResponse),
        (status = 404, description = "Hold not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn cancel_hold(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<Hold>> {
    claims.require_cancel_hold()?;
    let can_manage_others = claims.rights.holds_rights.rank() >= Rights::Write.rank();
    let hold = state
        .services
        .holds
        .cancel(id, claims.user_id, can_manage_others)
        .await?;

    state.services.audit.log(
        audit::event::HOLD_CANCELLED,
        Some(claims.user_id),
        Some("hold"),
        Some(id),
        ip,
        None::<()>,
     audit::AuditLogMeta::success());

    Ok(Json(hold))
}
