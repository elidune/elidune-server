//! API handlers for Elidune REST endpoints

pub mod admin_config;
pub mod audit;
pub mod auth;
pub mod batch;
pub mod biblios;
pub mod collections;
pub mod covers;
pub mod equipment;
pub mod events;
pub mod fines;
pub mod health;
pub mod history;
pub mod inventory;
pub mod library_info;
pub mod loans;
pub mod maintenance;
pub mod openapi;
pub mod opac;
pub mod public_types;
pub mod reservations;
pub mod schedules;
pub mod series;
pub mod settings;
pub mod sources;
pub mod sse;
pub mod stats;
pub mod users;
pub mod visitor_counts;
pub mod z3950;

use std::net::SocketAddr;

use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequest, FromRequestParts, Request},
    http::{header::AUTHORIZATION, request::Parts},
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::{error::AppError, models::user::{UserClaims, SCOPE_CHANGE_PASSWORD}, AppState};

/// Resolved client IP for audit: proxy headers first, then `ConnectInfo` peer address.
pub struct ClientIp(pub Option<String>);

#[async_trait]
impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0);
        Ok(ClientIp(crate::services::audit::resolve_client_ip(
            &parts.headers,
            peer,
        )))
    }
}

// ============================================================================
// ValidatedJson extractor
// ============================================================================

/// Axum extractor that parses a JSON body **and** runs `validator::Validate`
/// on the resulting value, returning `400 Validation` on failure.
pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        let value: T = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::Validation(format!("Invalid JSON body: {e}")))?;

        value
            .validate()
            .map_err(|e| AppError::Validation(e.to_string()))?;

        Ok(Self(value))
    }
}

// ============================================================================
// RBAC typed extractors
// ============================================================================

/// Extractor that succeeds only for admin users (account_type = "admin").
/// Returns 403 otherwise.
pub struct AdminUser(pub UserClaims);

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthenticatedUser(claims) = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !claims.is_admin() {
            return Err(AppError::Authorization("Admin access required".to_string()));
        }
        Ok(Self(claims))
    }
}

/// Extractor that succeeds for librarian or admin users.
/// Returns 403 for guests / plain readers.
pub struct StaffUser(pub UserClaims);

#[async_trait]
impl FromRequestParts<AppState> for StaffUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthenticatedUser(claims) = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !claims.is_admin() && !claims.is_librarian() {
            return Err(AppError::Authorization("Staff access required".to_string()));
        }
        Ok(Self(claims))
    }
}

// ============================================================================
// AuthenticatedUser extractor
// ============================================================================

/// Parse and validate a Bearer JWT from the request headers.
fn extract_claims(parts: &Parts, secret: &str) -> Result<UserClaims, AppError> {
    let auth_header = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Authentication("Missing authorization header".to_string()))?;

    if !auth_header.starts_with("Bearer ") {
        return Err(AppError::Authentication("Invalid authorization header format".to_string()));
    }

    UserClaims::from_token(&auth_header[7..], secret)
        .map_err(|e| AppError::Authentication(e.to_string()))
}

/// Extractor for authenticated user from JWT token.
///
/// Rejects tokens with scope `change_password_only` — those are only accepted
/// by the dedicated `POST /auth/change-password` endpoint via [`PasswordChangeUser`].
pub struct AuthenticatedUser(pub UserClaims);

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let claims = extract_claims(parts, &state.config.users.jwt_secret)?;

        if claims.is_password_change_scope() {
            return Err(AppError::Authorization(
                "Password change required before accessing this endpoint".to_string(),
            ));
        }

        Ok(AuthenticatedUser(claims))
    }
}

/// Extractor that accepts **only** scoped `change_password_only` tokens.
///
/// Used exclusively by `POST /auth/change-password`.
pub struct PasswordChangeUser(pub UserClaims);

#[async_trait]
impl FromRequestParts<AppState> for PasswordChangeUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let claims = extract_claims(parts, &state.config.users.jwt_secret)?;

        if claims.scope.as_deref() != Some(SCOPE_CHANGE_PASSWORD) {
            return Err(AppError::Authorization(
                "This endpoint requires a password-change token".to_string(),
            ));
        }

        Ok(PasswordChangeUser(claims))
    }
}

