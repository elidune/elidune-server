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
    pub const GONE: &str = "gone";
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

    #[error("Gone: {0}")]
    Gone(String),

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

struct HttpErrorFields {
    status: StatusCode,
    code: &'static str,
    label: &'static str,
    message: String,
}

impl AppError {
    fn http_fields(&self) -> HttpErrorFields {
        use error_code as ec;

        match self {
            AppError::Authentication(msg) => HttpErrorFields {
                status: StatusCode::UNAUTHORIZED,
                code: ec::AUTHENTICATION,
                label: "Unauthorized",
                message: msg.clone(),
            },
            AppError::Authorization(msg) => HttpErrorFields {
                status: StatusCode::FORBIDDEN,
                code: ec::AUTHORIZATION,
                label: "Forbidden",
                message: msg.clone(),
            },
            AppError::NotFound(msg) => HttpErrorFields {
                status: StatusCode::NOT_FOUND,
                code: ec::NOT_FOUND,
                label: "Not Found",
                message: msg.clone(),
            },
            AppError::Gone(msg) => HttpErrorFields {
                status: StatusCode::GONE,
                code: ec::GONE,
                label: "Gone",
                message: msg.clone(),
            },
            AppError::Validation(msg) => HttpErrorFields {
                status: StatusCode::BAD_REQUEST,
                code: ec::VALIDATION,
                label: "Validation Error",
                message: msg.clone(),
            },
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                HttpErrorFields {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: ec::DATABASE,
                    label: "Internal Server Error",
                    message: "A database error occurred".to_string(),
                }
            }
            AppError::Conflict(msg) => HttpErrorFields {
                status: StatusCode::CONFLICT,
                code: ec::CONFLICT,
                label: "Conflict",
                message: msg.clone(),
            },
            AppError::BadRequest(msg) => HttpErrorFields {
                status: StatusCode::BAD_REQUEST,
                code: ec::BAD_REQUEST,
                label: "Bad Request",
                message: msg.clone(),
            },
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                HttpErrorFields {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: ec::INTERNAL,
                    label: "Internal Server Error",
                    message: "An unexpected error occurred".to_string(),
                }
            }
            AppError::Z3950(msg) => HttpErrorFields {
                status: StatusCode::BAD_GATEWAY,
                code: ec::Z3950,
                label: "Z39.50 Error",
                message: msg.clone(),
            },
            AppError::BusinessRule(msg) => HttpErrorFields {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: ec::BUSINESS_RULE,
                label: "Business Rule Violation",
                message: msg.clone(),
            },
            AppError::DuplicateNeedsConfirmation { .. }
            | AppError::DuplicateBarcodeNeedsConfirmation { .. } => HttpErrorFields {
                status: StatusCode::CONFLICT,
                code: ec::CONFLICT,
                label: "Conflict",
                message: String::new(),
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        use error_code as ec;

        match self {
            AppError::DuplicateNeedsConfirmation {
                existing_id,
                existing_item,
                message,
            } => {
                let body = Json(crate::models::import_report::DuplicateConfirmationRequired {
                    code: ec::DUPLICATE_ISBN.to_string(),
                    existing_id,
                    existing_biblio: existing_item,
                    message,
                });
                return (StatusCode::CONFLICT, body).into_response();
            }
            AppError::DuplicateBarcodeNeedsConfirmation {
                existing_id,
                existing_item,
                message,
            } => {
                let body = Json(crate::models::import_report::DuplicateItemBarcodeRequired {
                    code: ec::DUPLICATE_BARCODE.to_string(),
                    existing_id,
                    existing_item,
                    message,
                });
                return (StatusCode::CONFLICT, body).into_response();
            }
            other => {
                let fields = other.http_fields();
                let body = Json(ErrorResponse {
                    code: fields.code.to_string(),
                    error: fields.label.to_string(),
                    message: fields.message,
                });
                return (fields.status, body).into_response();
            }
        }
    }
}

impl AppError {
    /// HTTP status, stable [`error_code`] string, and client-safe message (aligned with [`ErrorResponse`]).
    pub fn audit_http_fields(&self) -> (u16, &'static str, String) {
        use error_code as ec;

        match self {
            AppError::DuplicateNeedsConfirmation { message, .. } => {
                (409, ec::DUPLICATE_ISBN, message.clone())
            }
            AppError::DuplicateBarcodeNeedsConfirmation { message, .. } => {
                (409, ec::DUPLICATE_BARCODE, message.clone())
            }
            other => {
                let f = other.http_fields();
                (f.status.as_u16(), f.code, f.message)
            }
        }
    }
}

/// Result type alias for application operations
pub type AppResult<T> = Result<T, AppError>;

