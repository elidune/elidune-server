//! Error types for Elidune server

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

use crate::models::item::ItemShort;
use crate::models::specimen::SpecimenShort;

/// Application error codes matching the original C implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ErrorCode {
    Success = 0,
    Failure = 1,
    NotAuthorized = 2,
    DbFailure = 3,
    NoSuchUser = 4,
    NoSuchItem = 5,
    ServerMemoryFailure = 6,
    ItemNotAvailable = 7,
    Duplicate = 8,
    Z3950Failure = 9,
    Z3950Timeout = 10,
    MaxBorrowsReached = 11,
    NotBorrowable = 12,
    SpecimenBorrowed = 13,
    ItemIdentificationMissing = 14,
    SpecimenIdentificationMissing = 15,
    ItemAlreadyExists = 16,
    SpecimenAlreadyExists = 17,
    BadValue = 18,
    UserIdentificationAlreadyExists = 19,
    NoSuchData = 20,
    UserHasBorrowedSpecimens = 21,
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
        existing_item: ItemShort,
        message: String,
    },

    #[error("Duplicate barcode requires confirmation")]
    DuplicateBarcodeNeedsConfirmation {
        existing_id: i64,
        existing_specimen: SpecimenShort,
        message: String,
    },
}

/// Error response body
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub code: u32,
    pub error: String,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Authentication(msg) => {
                (StatusCode::UNAUTHORIZED, ErrorCode::NotAuthorized, msg.clone())
            }
            AppError::Authorization(msg) => {
                (StatusCode::FORBIDDEN, ErrorCode::NotAuthorized, msg.clone())
            }
            AppError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, ErrorCode::NoSuchItem, msg.clone())
            }
            AppError::Validation(msg) => {
                (StatusCode::BAD_REQUEST, ErrorCode::BadValue, msg.clone())
            }
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorCode::DbFailure,
                    "Database error".to_string(),
                )
            }
            AppError::Conflict(msg) => {
                (StatusCode::CONFLICT, ErrorCode::Duplicate, msg.clone())
            }
            AppError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, ErrorCode::BadValue, msg.clone())
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorCode::Failure,
                    "Internal server error".to_string(),
                )
            }
            AppError::Z3950(msg) => {
                (StatusCode::BAD_GATEWAY, ErrorCode::Z3950Failure, msg.clone())
            }
            AppError::BusinessRule(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::Failure, msg.clone())
            }
            AppError::DuplicateNeedsConfirmation {
                existing_id,
                existing_item,
                ref message,
            } => {
                let body = Json(crate::models::import_report::DuplicateConfirmationRequired {
                    code: "duplicate_isbn_needs_confirmation".to_string(),
                    existing_id: *existing_id,
                    existing_item: existing_item.clone(),
                    message: message.clone(),
                });
                return (StatusCode::CONFLICT, body).into_response();
            }
            AppError::DuplicateBarcodeNeedsConfirmation {
                existing_id,
                existing_specimen,
                ref message,
            } => {
                let body = Json(crate::models::import_report::DuplicateSpecimenBarcodeRequired {
                    code: "duplicate_barcode_needs_confirmation".to_string(),
                    existing_id: *existing_id,
                    existing_specimen: existing_specimen.clone(),
                    message: message.clone(),
                });
                return (StatusCode::CONFLICT, body).into_response();
            }
        };

        let body = Json(ErrorResponse {
            code: code as u32,
            error: format!("{:?}", code),
            message,
        });

        (status, body).into_response()
    }
}

/// Result type alias for application operations
pub type AppResult<T> = Result<T, AppError>;

