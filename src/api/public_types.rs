//! Public types API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    error::AppResult,
    models::public_type::{
        CreatePublicType, PublicType, PublicTypeLoanSettings, ReplacePublicTypeLoanSettingsRequest,
        UpdatePublicType,
    },
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// List all public types
#[utoipa::path(
    get,
    path = "/public-types",
    tag = "public_types",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of public types", body = Vec<PublicType>),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn list_public_types(
    State(state): State<crate::AppState>,
) -> AppResult<Json<Vec<PublicType>>> {
    let types = state.services.public_types.list().await?;
    Ok(Json(types))
}

/// Get public type by ID with loan settings
#[utoipa::path(
    get,
    path = "/public-types/{id}",
    tag = "public_types",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Public type ID")),
    responses(
        (status = 200, description = "Public type with loan settings"),
        (status = 404, description = "Not found")
    )
)]
pub async fn get_public_type(
    State(state): State<crate::AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<(PublicType, Vec<PublicTypeLoanSettings>)>> {
    let public_type = state.services.public_types.get_by_id(id).await?;
    let loan_settings = state.services.public_types.get_loan_settings(id).await?;
    Ok(Json((public_type, loan_settings)))
}

/// Create a new public type
#[utoipa::path(
    post,
    path = "/public-types",
    tag = "public_types",
    security(("bearer_auth" = [])),
    request_body = CreatePublicType,
    responses(
        (status = 201, description = "Public type created", body = PublicType),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn create_public_type(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreatePublicType>,
) -> AppResult<(StatusCode, Json<PublicType>)> {
    claims.require_write_settings()?;
    match state.services.public_types.create(&data).await {
        Ok(public_type) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_CREATED,
                Some(claims.user_id),
                Some("public_type"),
                Some(public_type.id),
                ip,
                Some((&data, &public_type)),
                audit::AuditLogMeta::success(),
            );
            Ok((StatusCode::CREATED, Json(public_type)))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_CREATED,
                Some(claims.user_id),
                Some("public_type"),
                None,
                ip.clone(),
                Some(&data),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Update a public type
#[utoipa::path(
    put,
    path = "/public-types/{id}",
    tag = "public_types",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Public type ID")),
    request_body = UpdatePublicType,
    responses(
        (status = 200, description = "Public type updated", body = PublicType),
        (status = 404, description = "Not found")
    )
)]
pub async fn update_public_type(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<UpdatePublicType>,
) -> AppResult<Json<PublicType>> {
    claims.require_write_settings()?;
    match state.services.public_types.update(id, &data).await {
        Ok(public_type) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_UPDATED,
                Some(claims.user_id),
                Some("public_type"),
                Some(id),
                ip,
                Some((id, &data, &public_type)),
                audit::AuditLogMeta::success(),
            );
            Ok(Json(public_type))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_UPDATED,
                Some(claims.user_id),
                Some("public_type"),
                Some(id),
                ip.clone(),
                Some((id, &data)),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Delete a public type
#[utoipa::path(
    delete,
    path = "/public-types/{id}",
    tag = "public_types",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Public type ID")),
    responses(
        (status = 204, description = "Public type deleted"),
        (status = 400, description = "Cannot delete: users still reference it"),
        (status = 404, description = "Not found")
    )
)]
pub async fn delete_public_type(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    match state.services.public_types.delete(id).await {
        Ok(()) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_DELETED,
                Some(claims.user_id),
                Some("public_type"),
                Some(id),
                ip,
                Some(serde_json::json!({ "id": id })),
                audit::AuditLogMeta::success(),
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_DELETED,
                Some(claims.user_id),
                Some("public_type"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({ "id": id })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Replace all loan settings for a public type (full list). Rows are deleted and re-inserted; response is the new list (same order as GET).
#[utoipa::path(
    put,
    path = "/public-types/{id}/loan-settings",
    tag = "public_types",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Public type ID")),
    request_body = ReplacePublicTypeLoanSettingsRequest,
    responses(
        (status = 200, description = "Loan settings replaced", body = Vec<PublicTypeLoanSettings>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 404, description = "Public type not found", body = ErrorResponse)
    )
)]
pub async fn update_loan_settings(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<ReplacePublicTypeLoanSettingsRequest>,
) -> AppResult<Json<Vec<PublicTypeLoanSettings>>> {
    claims.require_write_settings()?;
    match state.services.public_types.update_loan_settings(id, &data).await {
        Ok(settings) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_LOAN_SETTINGS_UPDATED,
                Some(claims.user_id),
                Some("public_type"),
                Some(id),
                ip,
                Some((id, &data, &settings)),
                audit::AuditLogMeta::success(),
            );
            Ok(Json(settings))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::PUBLIC_TYPE_LOAN_SETTINGS_UPDATED,
                Some(claims.user_id),
                Some("public_type"),
                Some(id),
                ip.clone(),
                Some((id, &data)),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Build the public-types routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post, put};
    axum::Router::new()
        .route("/public-types", get(list_public_types).post(create_public_type))
        .route("/public-types/:id", get(get_public_type).put(update_public_type).delete(delete_public_type))
        .route("/public-types/:id/loan-settings", put(update_loan_settings))
}
