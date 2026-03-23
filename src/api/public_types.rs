//! Public types API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    error::AppResult,
    models::public_type::{
        CreatePublicType, PublicType, PublicTypeLoanSettings, UpdatePublicType,
    },
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// Request body for upserting a loan setting override
#[derive(Debug, serde::Deserialize, serde::Serialize, utoipa::ToSchema)]
pub struct UpsertLoanSettingRequest {
    pub media_type: String,
    pub duration: Option<i16>,
    pub nb_max: Option<i16>,
    pub nb_renews: Option<i16>,
}

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
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<Vec<PublicType>>> {
    claims.require_read_settings()?;
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
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<(PublicType, Vec<PublicTypeLoanSettings>)>> {
    claims.require_read_settings()?;
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
    let public_type = state.services.public_types.create(&data).await?;
    state.services.audit.log(audit::event::PUBLIC_TYPE_CREATED, Some(claims.user_id), Some("public_type"), Some(public_type.id), ip, Some((&data, &public_type)));
    Ok((StatusCode::CREATED, Json(public_type)))
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
    let public_type = state.services.public_types.update(id, &data).await?;
    state.services.audit.log(audit::event::PUBLIC_TYPE_UPDATED, Some(claims.user_id), Some("public_type"), Some(id), ip, Some((id, &data, &public_type)));
    Ok(Json(public_type))
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
    state.services.public_types.delete(id).await?;
    state.services.audit.log(audit::event::PUBLIC_TYPE_DELETED, Some(claims.user_id), Some("public_type"), Some(id), ip, Some(serde_json::json!({ "id": id })));
    Ok(StatusCode::NO_CONTENT)
}

/// Upsert loan setting override for a public type
#[utoipa::path(
    put,
    path = "/public-types/{id}/loan-settings",
    tag = "public_types",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Public type ID")),
    request_body = UpsertLoanSettingRequest,
    responses(
        (status = 200, description = "Loan setting updated", body = PublicTypeLoanSettings),
        (status = 404, description = "Public type not found")
    )
)]
pub async fn upsert_loan_setting(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<UpsertLoanSettingRequest>,
) -> AppResult<Json<PublicTypeLoanSettings>> {
    claims.require_write_settings()?;
    let setting = state
        .services
        .public_types
        .upsert_loan_setting(id, &data.media_type, data.duration, data.nb_max, data.nb_renews)
        .await?;
    state.services.audit.log(audit::event::PUBLIC_TYPE_LOAN_SETTING_UPDATED, Some(claims.user_id), Some("public_type"), Some(id), ip, Some((&data, &setting)));
    Ok(Json(setting))
}

/// Delete loan setting override for a public type
#[utoipa::path(
    delete,
    path = "/public-types/{id}/loan-settings/{media_type}",
    tag = "public_types",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Public type ID"),
        ("media_type" = String, Path, description = "Media type code (e.g. printedText)")
    ),
    responses(
        (status = 204, description = "Loan setting removed"),
        (status = 404, description = "Not found")
    )
)]
pub async fn delete_loan_setting(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path((id, media_type)): Path<(i64, String)>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;
    state
        .services
        .public_types
        .delete_loan_setting(id, &media_type)
        .await?;
    state.services.audit.log(audit::event::PUBLIC_TYPE_LOAN_SETTING_DELETED, Some(claims.user_id), Some("public_type"), Some(id), ip, Some(serde_json::json!({ "id": id, "media_type": media_type })));
    Ok(StatusCode::NO_CONTENT)
}

/// Build the public-types routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post, put};
    axum::Router::new()
        .route("/public-types", get(list_public_types).post(create_public_type))
        .route("/public-types/{id}", get(get_public_type).put(update_public_type).delete(delete_public_type))
        .route("/public-types/{id}/loan-settings", put(upsert_loan_setting))
        .route("/public-types/{id}/loan-settings/{media_type}", delete(delete_loan_setting))
}
