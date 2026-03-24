//! Reservation (hold) endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_with::serde_as;
use utoipa::ToSchema;
use serde::Deserialize;
use serde_with::DisplayFromStr;

use crate::{
    error::AppResult,
    models::reservation::{CreateReservation, Reservation},
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// Create reservation request (for use in API — mirrors model)
#[serde_as]
#[derive(Deserialize, ToSchema)]
pub struct CreateReservationRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub item_id: i64,
    pub notes: Option<String>,
}

/// Place a hold on an item
#[utoipa::path(
    post,
    path = "/reservations",
    tag = "reservations",
    security(("bearer_auth" = [])),
    request_body = CreateReservationRequest,
    responses(
        (status = 201, description = "Reservation created", body = Reservation),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 409, description = "User already has a reservation for this item", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_reservation(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(req): Json<CreateReservationRequest>,
) -> AppResult<(StatusCode, Json<Reservation>)> {
    claims.require_write_borrows()?;
    let data = CreateReservation {
        user_id: req.user_id,
        item_id: req.item_id,
        notes: req.notes,
    };
    let reservation = state.services.reservations.place_hold(data).await?;

    state.services.audit.log(
        audit::event::RESERVATION_CREATED,
        Some(claims.user_id),
        Some("reservation"),
        Some(reservation.id),
        ip,
        None::<()>,
    );

    Ok((StatusCode::CREATED, Json(reservation)))
}

/// List reservations for an item (the hold queue)
#[utoipa::path(
    get,
    path = "/items/{id}/reservations",
    tag = "reservations",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Item ID")),
    responses(
        (status = 200, description = "Hold queue for this item", body = Vec<Reservation>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Item not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_reservations_for_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(item_id): Path<i64>,
) -> AppResult<Json<Vec<Reservation>>> {
    claims.require_read_borrows()?;
    let list = state.services.reservations.get_for_item(item_id).await?;
    Ok(Json(list))
}

/// List reservations for a user
#[utoipa::path(
    get,
    path = "/users/{id}/reservations",
    tag = "reservations",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "User's hold list", body = Vec<Reservation>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "User not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_reservations_for_user(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
) -> AppResult<Json<Vec<Reservation>>> {
    claims.require_read_users()?;
    let list = state.services.reservations.get_for_user(user_id).await?;
    Ok(Json(list))
}

/// Cancel a reservation
#[utoipa::path(
    delete,
    path = "/reservations/{id}",
    tag = "reservations",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Reservation ID")),
    responses(
        (status = 200, description = "Reservation cancelled", body = Reservation),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Cannot cancel another user's reservation", body = crate::error::ErrorResponse),
        (status = 404, description = "Reservation not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn cancel_reservation(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<Reservation>> {
    claims.require_write_borrows()?;
    let is_staff = claims.is_admin() || claims.is_librarian();
    let reservation = state
        .services
        .reservations
        .cancel(id, claims.user_id, is_staff)
        .await?;

    state.services.audit.log(
        audit::event::RESERVATION_CANCELLED,
        Some(claims.user_id),
        Some("reservation"),
        Some(id),
        ip,
        None::<()>,
    );

    Ok(Json(reservation))
}

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        .route("/reservations", post(create_reservation))
        .route("/reservations/:id", delete(cancel_reservation))
        .route("/items/:id/reservations", get(list_reservations_for_item))
        .route("/users/:id/reservations", get(list_reservations_for_user))
}
