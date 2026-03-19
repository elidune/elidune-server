//! Events API endpoints (cultural actions, school visits, animations)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    models::event::{CreateEvent, Event, EventQuery, UpdateEvent},
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// Paginated events response
#[derive(Serialize, ToSchema)]
pub struct EventsListResponse {
    pub events: Vec<Event>,
    pub total: i64,
}

/// List events with filters and pagination
#[utoipa::path(
    get,
    path = "/events",
    tag = "events",
    security(("bearer_auth" = [])),
    params(EventQuery),
    responses(
        (status = 200, description = "Events list", body = EventsListResponse)
    )
)]
pub async fn list_events(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<EventQuery>,
) -> AppResult<Json<EventsListResponse>> {
    claims.require_read_settings()?;
    let (events, total) = state.services.events.list(&query).await?;
    Ok(Json(EventsListResponse { events, total }))
}

/// Get event by ID
#[utoipa::path(
    get,
    path = "/events/{id}",
    tag = "events",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Event ID")),
    responses(
        (status = 200, description = "Event details", body = Event)
    )
)]
pub async fn get_event(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Event>> {
    claims.require_read_settings()?;
    let event = state.services.events.get_by_id(id).await?;
    Ok(Json(event))
}

/// Create an event
#[utoipa::path(
    post,
    path = "/events",
    tag = "events",
    security(("bearer_auth" = [])),
    request_body = CreateEvent,
    responses(
        (status = 201, description = "Event created", body = Event)
    )
)]
pub async fn create_event(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateEvent>,
) -> AppResult<(StatusCode, Json<Event>)> {
    claims.require_write_settings()?;
    let event = state.services.events.create(&data).await?;
    state.services.audit.log(audit::event::EVENT_CREATED, Some(claims.user_id), Some("event"), Some(event.id), ip, Some((&data, &event)));
    Ok((StatusCode::CREATED, Json(event)))
}

/// Update an event
#[utoipa::path(
    put,
    path = "/events/{id}",
    tag = "events",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Event ID")),
    request_body = UpdateEvent,
    responses(
        (status = 200, description = "Event updated", body = Event)
    )
)]
pub async fn update_event(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<UpdateEvent>,
) -> AppResult<Json<Event>> {
    claims.require_write_settings()?;
    let event = state.services.events.update(id, &data).await?;
    state.services.audit.log(audit::event::EVENT_UPDATED, Some(claims.user_id), Some("event"), Some(id), ip, Some((id, &data, &event)));
    Ok(Json(event))
}

/// Delete an event
#[utoipa::path(
    delete,
    path = "/events/{id}",
    tag = "events",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Event ID")),
    responses(
        (status = 204, description = "Event deleted")
    )
)]
pub async fn delete_event(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state.services.events.delete(id).await?;
    state.services.audit.log(audit::event::EVENT_DELETED, Some(claims.user_id), Some("event"), Some(id), ip, Some(serde_json::json!({ "id": id })));
    Ok(StatusCode::NO_CONTENT)
}
