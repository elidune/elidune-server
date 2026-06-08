//! Server-Sent Events — real-time notifications for connected clients
//!
//! Clients subscribe with a valid JWT token. The server pushes events
//! (loan created, item returned, hold ready) as they happen.
//!
//! Architecture: a tokio broadcast channel is held in AppState. All handlers
//! that create/modify loans or holds publish to the channel. SSE
//! subscribers receive a filtered stream.

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;



use super::AuthenticatedUser;

pub use crate::models::dto::sse::SsePayload;

/// Subscribe to real-time library events
///
/// Returns a Server-Sent Events stream. Auth via `Authorization: Bearer <token>` header.
///
/// **Event types published:**
/// - `loan.created` — a new loan was created
/// - `loan.returned` — a specimen was returned
/// - `loan.renewed` — a loan was renewed
/// - `hold.ready` — a hold is ready for pickup
#[utoipa::path(
    get,
    path = "/events/stream",
    tag = "sse",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "SSE stream (text/event-stream)"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn sse_stream(
    State(state): State<crate::AppState>,
    AuthenticatedUser(_claims): AuthenticatedUser,
) -> impl IntoResponse {
    let rx = state.event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        msg.ok().map(|payload: SsePayload| {
            let data = serde_json::to_string(&payload).unwrap_or_default();
            Ok::<_, std::convert::Infallible>(
                Event::default()
                    .event(payload.event.clone())
                    .data(data),
            )
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new().route("/events/stream", get(sse_stream))
}
