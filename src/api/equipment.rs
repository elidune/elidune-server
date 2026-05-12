//! Equipment API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    error::AppResult,
    models::equipment::{CreateEquipment, Equipment, UpdateEquipment},
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// List all equipment
#[utoipa::path(
    get,
    path = "/equipment",
    tag = "equipment",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Equipment list", body = Vec<Equipment>),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<Vec<Equipment>>> {
    claims.require_read_settings()?;
    let equipment = state.services.equipment.list().await?;
    Ok(Json(equipment))
}

/// Get equipment by ID
#[utoipa::path(
    get,
    path = "/equipment/{id}",
    tag = "equipment",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Equipment ID")),
    responses(
        (status = 200, description = "Equipment details", body = Equipment),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn get_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Equipment>> {
    claims.require_read_settings()?;
    let equipment = state.services.equipment.get_by_id(id).await?;
    Ok(Json(equipment))
}

/// Create equipment
#[utoipa::path(
    post,
    path = "/equipment",
    tag = "equipment",
    security(("bearer_auth" = [])),
    request_body = CreateEquipment,
    responses(
        (status = 201, description = "Equipment created", body = Equipment),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn create_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateEquipment>,
) -> AppResult<(StatusCode, Json<Equipment>)> {
    claims.require_write_settings()?;
    match state.services.equipment.create(&data).await {
        Ok(equipment) => {
            state.services.audit.log(
                audit::event::EQUIPMENT_CREATED,
                Some(claims.user_id),
                Some("equipment"),
                Some(equipment.id),
                ip,
                Some(&equipment),
                audit::AuditLogMeta::success(),
            );
            Ok((StatusCode::CREATED, Json(equipment)))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::EQUIPMENT_CREATED,
                Some(claims.user_id),
                Some("equipment"),
                None,
                ip.clone(),
                None::<serde_json::Value>,
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Update equipment
#[utoipa::path(
    put,
    path = "/equipment/{id}",
    tag = "equipment",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Equipment ID")),
    request_body = UpdateEquipment,
    responses(
        (status = 200, description = "Equipment updated", body = Equipment),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn update_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<UpdateEquipment>,
) -> AppResult<Json<Equipment>> {
    claims.require_write_settings()?;
    match state.services.equipment.update(id, &data).await {
        Ok(equipment) => {
            state.services.audit.log(
                audit::event::EQUIPMENT_UPDATED,
                Some(claims.user_id),
                Some("equipment"),
                Some(id),
                ip,
                Some(&equipment),
                audit::AuditLogMeta::success(),
            );
            Ok(Json(equipment))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::EQUIPMENT_UPDATED,
                Some(claims.user_id),
                Some("equipment"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({ "id": id })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Delete equipment
#[utoipa::path(
    delete,
    path = "/equipment/{id}",
    tag = "equipment",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Equipment ID")),
    responses(
        (status = 204, description = "Equipment deleted"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn delete_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    match state.services.equipment.delete(id).await {
        Ok(()) => {
            state.services.audit.log(
                audit::event::EQUIPMENT_DELETED,
                Some(claims.user_id),
                Some("equipment"),
                Some(id),
                ip,
                Some(serde_json::json!({ "id": id })),
                audit::AuditLogMeta::success(),
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::EQUIPMENT_DELETED,
                Some(claims.user_id),
                Some("equipment"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({ "id": id })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Build the equipment routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post, put};
    axum::Router::new()
        .route("/equipment", get(list_equipment).post(create_equipment))
        .route("/equipment/:id", get(get_equipment).put(update_equipment).delete(delete_equipment))
}
