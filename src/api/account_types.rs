//! Library `account_types` API (roles and fine-grained rights).

use axum::{
    extract::{Path, State},
    Json,
};

#[allow(unused_imports)]
use crate::error::ErrorResponse;
use crate::{
    error::AppResult,
    models::account_type::{AccountTypeDefinition, UpdateAccountTypeDefinition},
    services::audit,
};

use super::{AuthenticatedUser, ClientIp};

/// List all account type definitions (`account_types` table).
#[utoipa::path(
    get,
    path = "/account-types",
    tag = "account_types",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Account types with rights", body = Vec<AccountTypeDefinition>),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
    )
)]
pub async fn list_account_types(
    State(state): State<crate::AppState>,
    AuthenticatedUser(_claims): AuthenticatedUser,
) -> AppResult<Json<Vec<AccountTypeDefinition>>> {
    let rows = state.services.account_types_catalog.list().await?;
    Ok(Json(rows))
}

/// Get one account type by code (e.g. `librarian`, `admin`).
#[utoipa::path(
    get,
    path = "/account-types/{code}",
    tag = "account_types",
    security(("bearer_auth" = [])),
    params(("code" = String, Path, description = "Account type code")),
    responses(
        (status = 200, description = "Account type", body = AccountTypeDefinition),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "Unknown code", body = ErrorResponse),
    )
)]
pub async fn get_account_type(
    State(state): State<crate::AppState>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    Path(code): Path<String>,
) -> AppResult<Json<AccountTypeDefinition>> {
    let row = state.services.account_types_catalog.get_by_code(&code).await?;
    Ok(Json(row))
}

/// Update display name and/or rights for an account type (admin only). `code` is immutable.
#[utoipa::path(
    put,
    path = "/account-types/{code}",
    tag = "account_types",
    security(("bearer_auth" = [])),
    params(("code" = String, Path, description = "Account type code")),
    request_body = UpdateAccountTypeDefinition,
    responses(
        (status = 200, description = "Updated account type", body = AccountTypeDefinition),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Administrator required", body = ErrorResponse),
        (status = 404, description = "Unknown code", body = ErrorResponse),
    )
)]
pub async fn update_account_type(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(code): Path<String>,
    Json(mut body): Json<UpdateAccountTypeDefinition>,
) -> AppResult<Json<AccountTypeDefinition>> {
    claims.require_admin()?;
    let before = state.services.account_types_catalog.get_by_code(&code).await?;
    let updated = state
        .services
        .account_types_catalog
        .update(&code, &mut body)
        .await?;

    state.services.audit.log(
        audit::event::ACCOUNT_TYPE_UPDATED,
        Some(claims.user_id),
        Some("account_type"),
        None,
        ip,
        Some(serde_json::json!({
            "code": code,
            "before": before,
            "after": &updated,
        })),
     audit::AuditLogMeta::success());

    Ok(Json(updated))
}

/// Routes under `/api/v1`.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/account-types", get(list_account_types))
        .route("/account-types/:code", get(get_account_type).put(update_account_type))
}
