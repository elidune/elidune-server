//! User management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    error::AppResult,
    models::user::{UpdateAccountType, UpdateProfile, User, UserPayload, UserQuery, UserShort},
    services::audit,
};

use super::{biblios::PaginatedResponse, AuthenticatedUser, ClientIp, ValidatedJson};


/// Build the users routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, put};
    axum::Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/:id", get(get_user).put(update_user).delete(delete_user))
        .route("/users/:id/account-type", put(update_account_type))
        .route("/users/:id/force-password-change", put(force_password_change))
        .route("/users/:id/loans", get(super::loans::get_user_loans))
        .route(
            "/users/:id/loans/export",
            get(super::loans::export_user_loans_marc),
        )
}


/// List users with search and pagination
#[utoipa::path(
    get,
    path = "/users",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("name" = Option<String>, Query, description = "Search by name"),
        ("barcode" = Option<String>, Query, description = "Search by barcode"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("per_page" = Option<i64>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "List of users", body = PaginatedResponse<UserShort>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_users(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<UserQuery>,
) -> AppResult<Json<PaginatedResponse<UserShort>>> {
    claims.require_read_users()?;

    let (users, total) = state.services.users.search_users(&query).await?;
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);

    Ok(Json(PaginatedResponse::new(users, total, page, per_page)))
}

/// Get user details by ID
#[utoipa::path(
    get,
    path = "/users/{id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User details", body = User),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<User>> {
    claims.require_read_users()?;

    let user = state.services.users.get_by_id(id).await?;
    Ok(Json(user))
}

/// Create a new user
#[utoipa::path(
    post,
    path = "/users",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = UserPayload,
    responses(
        (status = 201, description = "User created", body = User),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "Login already exists")
    )
)]
pub async fn create_user(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    ValidatedJson(user): ValidatedJson<UserPayload>,
) -> AppResult<(StatusCode, Json<User>)> {
    claims.require_write_users()?;
    let for_audit = user.clone();
    match state.services.users.create_user(user).await {
        Ok(created) => {
            state.services.audit.log(
                audit::event::USER_CREATED,
                Some(claims.user_id),
                Some("user"),
                Some(created.id),
                ip,
                Some(&created),
                audit::AuditLogMeta::success(),
            );
            Ok((StatusCode::CREATED, Json(created)))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::USER_CREATED,
                Some(claims.user_id),
                Some("user"),
                None,
                ip.clone(),
                Some(&for_audit),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Update an existing user
#[utoipa::path(
    put,
    path = "/users/{id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "User ID")
    ),
    request_body = UserPayload,
    responses(
        (status = 200, description = "User updated", body = User),
        (status = 404, description = "User not found")
    )
)]
pub async fn update_user(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    ValidatedJson(user): ValidatedJson<UserPayload>,
) -> AppResult<Json<User>> {
    claims.require_write_users()?;
    let audit_payload = user.clone();
    match state.services.users.update_user(id, user).await {
        Ok(updated) => {
            state.services.audit.log(
                audit::event::USER_UPDATED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip,
                Some((id, audit_payload)),
                audit::AuditLogMeta::success(),
            );
            Ok(Json(updated))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::USER_UPDATED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({ "id": id })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Delete a user
#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "User ID"),
        ("force" = Option<bool>, Query, description = "Force delete even with active loans")
    ),
    responses(
        (status = 204, description = "User deleted"),
        (status = 404, description = "User not found"),
        (status = 409, description = "User has active loans")
    )
)]
pub async fn delete_user(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Query(params): Query<DeleteUserParams>,
) -> AppResult<StatusCode> {
    claims.require_write_users()?;
    let force = params.force.unwrap_or(false);
    match state.services.users.delete_user(id, force).await {
        Ok(()) => {
            state.services.audit.log(
                audit::event::USER_DELETED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip,
                Some(serde_json::json!({ "id": id, "force": force })),
                audit::AuditLogMeta::success(),
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::USER_DELETED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({ "id": id, "force": force })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

#[derive(Deserialize)]
pub struct DeleteUserParams {
    pub force: Option<bool>,
}

/// Update own profile (name, password)
#[utoipa::path(
    put,
    path = "/auth/profile",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = UpdateProfile,
    responses(
        (status = 200, description = "Profile updated", body = User),
        (status = 400, description = "Invalid input"),
        (status = 401, description = "Not authenticated or wrong current password")
    )
)]
pub async fn update_my_profile(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ValidatedJson(profile): ValidatedJson<UpdateProfile>,
) -> AppResult<Json<User>> {
    let updated = state.services.users.update_profile(claims.user_id, profile).await?;
    Ok(Json(updated))
}

/// Update user's account type (admin only)
#[utoipa::path(
    put,
    path = "/users/{id}/account-type",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "User ID")
    ),
    request_body = UpdateAccountType,
    responses(
        (status = 200, description = "Account type updated", body = User),
        (status = 403, description = "Admin privileges required"),
        (status = 404, description = "User not found")
    )
)]
pub async fn update_account_type(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(request): Json<UpdateAccountType>,
) -> AppResult<Json<User>> {
    claims.require_admin()?;
    match state
        .services
        .users
        .update_account_type(id, &request.account_type)
        .await
    {
        Ok(updated) => {
            state.services.audit.log(
                audit::event::USER_ACCOUNT_TYPE_CHANGED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip,
                Some(serde_json::json!({ "new_account_type": request.account_type })),
                audit::AuditLogMeta::success(),
            );
            Ok(Json(updated))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::USER_ACCOUNT_TYPE_CHANGED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({ "new_account_type": request.account_type })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

/// Force the user to change their password on next login (admin only).
#[utoipa::path(
    put,
    path = "/users/{id}/force-password-change",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "Flag updated"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin privileges required"),
        (status = 404, description = "User not found")
    )
)]
pub async fn force_password_change(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    claims.require_admin()?;

    match state.services.users.set_must_change_password(id, true).await {
        Ok(()) => {
            state.services.audit.log(
                audit::event::USER_UPDATED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip,
                Some(serde_json::json!({ "must_change_password": true })),
                audit::AuditLogMeta::success(),
            );
            Ok(Json(serde_json::json!({ "message": "User must change password on next login" })))
        }
        Err(e) => {
            state.services.audit.log(
                audit::event::USER_UPDATED,
                Some(claims.user_id),
                Some("user"),
                Some(id),
                ip.clone(),
                Some(serde_json::json!({
                    "must_change_password": true,
                    "action": "force_password_change"
                })),
                audit::AuditLogMeta::from_app_error(&e),
            );
            Err(e)
        }
    }
}

