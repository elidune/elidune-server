//! Schedule API endpoints (periods, slots, closures)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde_json::json;

use crate::{
    error::AppResult,
    models::schedule::{
        CreateScheduleClosure, CreateSchedulePeriod, CreateScheduleSlot,
        ScheduleClosure, ScheduleClosureQuery, SchedulePeriod, ScheduleSlot,
        UpdateSchedulePeriod,
    },
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};


/// Build the schedules routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post, put};
    axum::Router::new()
        .route("/schedules/periods", get(list_periods).post(create_period))
        .route("/schedules/periods/:id", put(update_period).delete(delete_period))
        .route("/schedules/periods/:id/slots", get(list_slots).post(create_slot))
        .route("/schedules/slots/:id", delete(delete_slot))
        .route("/schedules/closures", get(list_closures).post(create_closure))
        .route("/schedules/closures/:id", delete(delete_closure))
}


// ---- Periods ----

/// List schedule periods
#[utoipa::path(
    get,
    path = "/schedules/periods",
    tag = "schedules",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Schedule periods", body = Vec<SchedulePeriod>),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_periods(
    State(state): State<crate::AppState>,
) -> AppResult<Json<Vec<SchedulePeriod>>> {
    let periods = state.services.schedules.list_periods().await?;
    Ok(Json(periods))
}

/// Create a schedule period
#[utoipa::path(
    post,
    path = "/schedules/periods",
    tag = "schedules",
    security(("bearer_auth" = [])),
    request_body = CreateSchedulePeriod,
    responses(
        (status = 201, description = "Period created", body = SchedulePeriod),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn create_period(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateSchedulePeriod>,
) -> AppResult<(StatusCode, Json<SchedulePeriod>)> {
    claims.require_write_settings()?;
    let period = state.services.schedules.create_period(&data).await?;
    state.services.audit.log(audit::event::SCHEDULE_PERIOD_CREATED, Some(claims.user_id), Some("schedule_period"), Some(period.id), ip, Some((&data, &period)), audit::AuditLogMeta::success());
    Ok((StatusCode::CREATED, Json(period)))
}

/// Update a schedule period
#[utoipa::path(
    put,
    path = "/schedules/periods/{id}",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Period ID")),
    request_body = UpdateSchedulePeriod,
    responses(
        (status = 200, description = "Period updated", body = SchedulePeriod),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn update_period(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<UpdateSchedulePeriod>,
) -> AppResult<Json<SchedulePeriod>> {
    claims.require_write_settings()?;
    let period = state.services.schedules.update_period(id, &data).await?;
    state.services.audit.log(audit::event::SCHEDULE_PERIOD_UPDATED, Some(claims.user_id), Some("schedule_period"), Some(id), ip, Some((id, &data, &period)), audit::AuditLogMeta::success());
    Ok(Json(period))
}

/// Delete a schedule period
#[utoipa::path(
    delete,
    path = "/schedules/periods/{id}",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Period ID")),
    responses(
        (status = 204, description = "Period deleted"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn delete_period(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state.services.schedules.delete_period(id).await?;
    state.services.audit.log(audit::event::SCHEDULE_PERIOD_DELETED, Some(claims.user_id), Some("schedule_period"), Some(id), ip, Some(json!({ "id": id })), audit::AuditLogMeta::success());
    Ok(StatusCode::NO_CONTENT)
}

// ---- Slots ----

/// List slots for a period
#[utoipa::path(
    get,
    path = "/schedules/periods/{id}/slots",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Period ID")),
    responses(
        (status = 200, description = "Period slots", body = Vec<ScheduleSlot>),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_slots(
    State(state): State<crate::AppState>,
    Path(period_id): Path<i64>,
) -> AppResult<Json<Vec<ScheduleSlot>>> {
    let slots = state.services.schedules.list_slots(period_id).await?;
    Ok(Json(slots))
}

/// Create a slot for a period
#[utoipa::path(
    post,
    path = "/schedules/periods/{id}/slots",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Period ID")),
    request_body = CreateScheduleSlot,
    responses(
        (status = 201, description = "Slot created", body = ScheduleSlot),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn create_slot(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(period_id): Path<i64>,
    Json(data): Json<CreateScheduleSlot>,
) -> AppResult<(StatusCode, Json<ScheduleSlot>)> {
    claims.require_write_settings()?;
    let slot = state.services.schedules.create_slot(period_id, &data).await?;
    state.services.audit.log(audit::event::SCHEDULE_SLOT_CREATED, Some(claims.user_id), Some("schedule_slot"), Some(slot.id), ip, Some((period_id, &data, &slot)), audit::AuditLogMeta::success());
    Ok((StatusCode::CREATED, Json(slot)))
}

/// Delete a slot
#[utoipa::path(
    delete,
    path = "/schedules/slots/{id}",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Slot ID")),
    responses(
        (status = 204, description = "Slot deleted"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn delete_slot(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state.services.schedules.delete_slot(id).await?;
    state.services.audit.log(audit::event::SCHEDULE_SLOT_DELETED, Some(claims.user_id), Some("schedule_slot"), Some(id), ip, Some(json!({ "id": id })), audit::AuditLogMeta::success());
    Ok(StatusCode::NO_CONTENT)
}

// ---- Closures ----

/// List schedule closures
#[utoipa::path(
    get,
    path = "/schedules/closures",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(ScheduleClosureQuery),
    responses(
        (status = 200, description = "Closures list", body = Vec<ScheduleClosure>),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_closures(
    State(state): State<crate::AppState>,
    Query(query): Query<ScheduleClosureQuery>,
) -> AppResult<Json<Vec<ScheduleClosure>>> {
    let start = query.start_date.as_ref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let end = query.end_date.as_ref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let closures = state.services.schedules.list_closures(start, end).await?;
    Ok(Json(closures))
}

/// Create a closure
#[utoipa::path(
    post,
    path = "/schedules/closures",
    tag = "schedules",
    security(("bearer_auth" = [])),
    request_body = CreateScheduleClosure,
    responses(
        (status = 201, description = "Closure created", body = ScheduleClosure),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn create_closure(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateScheduleClosure>,
) -> AppResult<(StatusCode, Json<ScheduleClosure>)> {
    claims.require_write_settings()?;
    let closure = state.services.schedules.create_closure(&data).await?;
    state.services.audit.log(audit::event::SCHEDULE_CLOSURE_CREATED, Some(claims.user_id), Some("schedule_closure"), Some(closure.id), ip, Some((&data, &closure)), audit::AuditLogMeta::success());
    Ok((StatusCode::CREATED, Json(closure)))
}

/// Delete a closure
#[utoipa::path(
    delete,
    path = "/schedules/closures/{id}",
    tag = "schedules",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Closure ID")),
    responses(
        (status = 204, description = "Closure deleted"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn delete_closure(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state.services.schedules.delete_closure(id).await?;
    state.services.audit.log(audit::event::SCHEDULE_CLOSURE_DELETED, Some(claims.user_id), Some("schedule_closure"), Some(id), ip, Some(json!({ "id": id })), audit::AuditLogMeta::success());
    Ok(StatusCode::NO_CONTENT)
}

