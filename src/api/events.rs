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
    services::{
        audit,
        events::{AnnouncementReport, SendAnnouncementRequest},
    },
};

use super::{AuthenticatedUser, ClientIp};


/// Build the events routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/events", get(list_events).post(create_event))
        .route("/events/:id", get(get_event).put(update_event).delete(delete_event))
        .route("/events/:id/send-announcement", post(send_event_announcement))
}

/// Paginated events response
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
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
        (status = 200, description = "Events list", body = EventsListResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_events(
    State(state): State<crate::AppState>,
    Query(query): Query<EventQuery>,
) -> AppResult<Json<EventsListResponse>> {
    let (events, total) = state.services.events.list(&query).await?;
    Ok(Json(EventsListResponse { events, total }))
}

/// Get event by ID (includes `attachmentDataBase64` when an attachment exists)
#[utoipa::path(
    get,
    path = "/events/{id}",
    tag = "events",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Event ID")),
    responses(
        (status = 200, description = "Event details", body = Event),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn get_event(
    State(state): State<crate::AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Event>> {
    let event = state.services.events.get_by_id_with_attachment(id).await?;
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
        (status = 201, description = "Event created", body = Event),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
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

/// Update an event (optional `attachment` / `removeAttachment` same as create semantics)
#[utoipa::path(
    put,
    path = "/events/{id}",
    tag = "events",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Event ID")),
    request_body = UpdateEvent,
    responses(
        (status = 200, description = "Event updated", body = Event),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
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
        (status = 204, description = "Event deleted"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
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

/// Send an announcement email for an event to all users whose public_type matches
/// the event's target_public (all users if target_public is NULL).
///
/// The default `event_announcement` template is used unless `subject`/`body_plain`
/// (and optionally `body_html`) are supplied in the request body, in which case the
/// supplied text overrides the template entirely.
#[utoipa::path(
    post,
    path = "/events/{id}/send-announcement",
    tag = "events",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Event ID")),
    request_body = SendAnnouncementRequest,
    responses(
        (status = 200, description = "Announcement report", body = AnnouncementReport),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn send_event_announcement(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(payload): Json<SendAnnouncementRequest>,
) -> AppResult<Json<AnnouncementReport>> {
    claims.require_write_settings()?;
    let report = state
        .services
        .events
        .send_announcement(id, &payload, Some(claims.user_id), ip)
        .await?;
    Ok(Json(report))
}
