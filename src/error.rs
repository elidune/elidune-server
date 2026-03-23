//! Error types for Elidune server

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

use crate::models::biblio::BiblioShort;
use crate::models::item::ItemShort;

/// Machine-readable string error codes used in API responses.
///
/// Using string codes instead of legacy integers makes the API self-documenting
/// and avoids tight coupling with a specific numbering scheme.
pub mod error_code {
    pub const AUTHENTICATION: &str = "authentication_failed";
    pub const AUTHORIZATION: &str = "authorization_failed";
    pub const NOT_FOUND: &str = "not_found";
    pub const VALIDATION: &str = "validation_error";
    pub const DATABASE: &str = "database_error";
    pub const CONFLICT: &str = "conflict";
    pub const BAD_REQUEST: &str = "bad_request";
    pub const INTERNAL: &str = "internal_error";
    pub const Z3950: &str = "z3950_error";
    pub const BUSINESS_RULE: &str = "business_rule_violation";
    pub const DUPLICATE_ISBN: &str = "duplicate_isbn_needs_confirmation";
    pub const DUPLICATE_BARCODE: &str = "duplicate_barcode_needs_confirmation";
}

/// Main application error type
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Authorization failed: {0}")]
    Authorization(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Z39.50 error: {0}")]
    Z3950(String),

    #[error("Business rule violation: {0}")]
    BusinessRule(String),

    #[error("Duplicate ISBN requires confirmation")]
    DuplicateNeedsConfirmation {
        existing_id: i64,
        existing_item: BiblioShort,
        message: String,
    },

    #[error("Duplicate barcode requires confirmation")]
    DuplicateBarcodeNeedsConfirmation {
        existing_id: i64,
        existing_item: ItemShort,
        message: String,
    },
}

/// Error response body returned for all API errors.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Machine-readable error code (e.g. `"not_found"`, `"validation_error"`)
    pub code: String,
    /// Human-readable error category
    pub error: String,
    /// Detailed error message
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        use error_code as ec;

        let (status, code, error_label, message) = match &self {
            AppError::Authentication(msg) => (
                StatusCode::UNAUTHORIZED,
                ec::AUTHENTICATION,
                "Unauthorized",
                msg.clone(),
            ),
            AppError::Authorization(msg) => (
                StatusCode::FORBIDDEN,
                ec::AUTHORIZATION,
                "Forbidden",
                msg.clone(),
            ),
            AppError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, ec::NOT_FOUND, "Not Found", msg.clone())
            }
            AppError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                ec::VALIDATION,
                "Validation Error",
                msg.clone(),
            ),
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ec::DATABASE,
                    "Internal Server Error",
                    "A database error occurred".to_string(),
                )
            }
            AppError::Conflict(msg) => {
                (StatusCode::CONFLICT, ec::CONFLICT, "Conflict", msg.clone())
            }
            AppError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                ec::BAD_REQUEST,
                "Bad Request",
                msg.clone(),
            ),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ec::INTERNAL,
                    "Internal Server Error",
                    "An unexpected error occurred".to_string(),
                )
            }
            AppError::Z3950(msg) => (
                StatusCode::BAD_GATEWAY,
                ec::Z3950,
                "Z39.50 Error",
                msg.clone(),
            ),
            AppError::BusinessRule(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ec::BUSINESS_RULE,
                "Business Rule Violation",
                msg.clone(),
            ),
            AppError::DuplicateNeedsConfirmation {
                existing_id,
                existing_item,
                ref message,
            } => {
                let body = Json(crate::models::import_report::DuplicateConfirmationRequired {
                    code: ec::DUPLICATE_ISBN.to_string(),
                    existing_id: *existing_id,
                    existing_biblio: existing_item.clone(),
                    message: message.clone(),
                });
                return (StatusCode::CONFLICT, body).into_response();
            }
            AppError::DuplicateBarcodeNeedsConfirmation {
                existing_id,
                existing_item,
                ref message,
            } => {
                let body = Json(crate::models::import_report::DuplicateItemBarcodeRequired {
                    code: ec::DUPLICATE_BARCODE.to_string(),
                    existing_id: *existing_id,
                    existing_item: existing_item.clone(),
                    message: message.clone(),
                });
                return (StatusCode::CONFLICT, body).into_response();
            }
        };

        let body = Json(ErrorResponse {
            code: code.to_string(),
            error: error_label.to_string(),
            message,
        });

        (status, body).into_response()
    }
}

/// Result type alias for application operations
pub type AppResult<T> = Result<T, AppError>;

