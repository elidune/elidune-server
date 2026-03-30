//! Background task status API.
//!
//! Allows the frontend to list and poll long-running server operations
//! (MARC batch imports, maintenance runs, …) for a given authenticated user.
//!
//! ## Lifecycle
//! 1. A long-running endpoint returns `202 Accepted` with `{ "taskId": "<id>" }`.
//! 2. The client polls `GET /tasks/:id` until `status` is `completed` or `failed`.
//! 3. On reconnect the client calls `GET /tasks` to recover its pending/completed tasks.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::{
    error::{AppError, AppResult},
    models::task::BackgroundTask,
    AppState,
};

use super::AuthenticatedUser;


// ── Router ─────────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/tasks", get(list_tasks))
        .route("/tasks/:id", get(get_task))
}


// ── Response types ─────────────────────────────────────────────────────────────

/// Returned by endpoints that kick off a background task (`202 Accepted`).
#[serde_as]
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskAcceptedResponse {
    /// Opaque task identifier — use with `GET /tasks/:id` to poll for progress.
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub task_id: i64,
}

// ── Handlers ───────────────────────────────────────────────────────────────────

/// List background tasks for the current user.
///
/// Returns the authenticated user's tasks (active + completed/failed within the
/// last 24 hours).  Admin users additionally see all other users' **active**
/// tasks.
#[utoipa::path(
    get,
    path = "/tasks",
    tag = "tasks",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Task list", body = Vec<BackgroundTask>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<Vec<BackgroundTask>>> {
    let tasks = state
        .services
        .tasks
        .list_tasks(claims.user_id, claims.is_admin())
        .await;
    Ok(Json(tasks))
}

/// Get the current state of a background task.
///
/// Poll this endpoint until `status` is `completed` or `failed`.  The `result`
/// field is populated on completion; `error` is populated on failure.
///
/// Completed task data is retained for 24 hours after the operation finishes.
#[utoipa::path(
    get,
    path = "/tasks/{id}",
    tag = "tasks",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Task ID returned by the initiating endpoint")
    ),
    responses(
        (status = 200, description = "Task state", body = BackgroundTask),
        (status = 404, description = "Task not found or expired"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Task belongs to another user")
    )
)]
pub async fn get_task(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<BackgroundTask>> {
    let task = state
        .services
        .tasks
        .get_task(id)
        .await
        .ok_or_else(|| AppError::NotFound("Task not found or expired".into()))?;

    if !claims.is_admin() && task.user_id != claims.user_id {
        return Err(AppError::Authorization(
            "Task belongs to another user".into(),
        ));
    }

    Ok(Json(task))
}

