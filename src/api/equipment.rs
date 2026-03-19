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
        (status = 200, description = "Equipment list", body = Vec<Equipment>)
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
        (status = 200, description = "Equipment details", body = Equipment)
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
        (status = 201, description = "Equipment created", body = Equipment)
    )
)]
pub async fn create_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateEquipment>,
) -> AppResult<(StatusCode, Json<Equipment>)> {
    claims.require_write_settings()?;
    let equipment = state.services.equipment.create(&data).await?;
    state.services.audit.log(audit::event::EQUIPMENT_CREATED, Some(claims.user_id), Some("equipment"), Some(equipment.id), ip, Some(&equipment));
    Ok((StatusCode::CREATED, Json(equipment)))
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
        (status = 200, description = "Equipment updated", body = Equipment)
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
    let equipment = state.services.equipment.update(id, &data).await?;
    state.services.audit.log(audit::event::EQUIPMENT_UPDATED, Some(claims.user_id), Some("equipment"), Some(id), ip, Some(&equipment));
    Ok(Json(equipment))
}

/// Delete equipment
#[utoipa::path(
    delete,
    path = "/equipment/{id}",
    tag = "equipment",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Equipment ID")),
    responses(
        (status = 204, description = "Equipment deleted")
    )
)]
pub async fn delete_equipment(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state.services.equipment.delete(id).await?;
    state.services.audit.log(audit::event::EQUIPMENT_DELETED, Some(claims.user_id), Some("equipment"), Some(id), ip, Some(serde_json::json!({ "id": id })));
    Ok(StatusCode::NO_CONTENT)
}
