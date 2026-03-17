//! Loan management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    models::loan::{CreateLoan, LoanDetails},
};

use super::AuthenticatedUser;

/// Create loan request
#[serde_as]
#[derive(Deserialize, ToSchema)]
pub struct CreateLoanRequest {
    /// User ID
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    /// Specimen ID (optional if identification provided)
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub specimen_id: Option<i64>,
    /// Specimen barcode/identification
    pub specimen_identification: Option<String>,
    /// Force loan even if rules are violated
    pub force: Option<bool>,
}

/// Loan response with calculated dates
#[serde_as]
#[derive(Serialize, ToSchema)]
pub struct LoanResponse {
    /// Loan ID
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    /// Due date (ISO 8601 format)
    pub issue_date: DateTime<Utc>,
    /// Status message
    pub message: String,
}

/// Return response with loan details
#[derive(Serialize, ToSchema)]
pub struct ReturnResponse {
    /// Return status
    pub status: String,
    /// Loan details
    pub loan: LoanDetails,
}

/// Get loans for a specific user
#[utoipa::path(
    get,
    path = "/users/{id}/loans",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "User ID"),
        ("include_returned" = Option<bool>, Query, description = "Include returned loans (default: false)")
    ),
    responses(
        (status = 200, description = "User's loans", body = Vec<LoanDetails>),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user_loans(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
    Query(query): Query<GetUserLoansQuery>,
) -> AppResult<Json<Vec<LoanDetails>>> {
    claims.require_read_users()?;

    let loans = state
        .services
        .loans
        .get_user_loans(user_id, query.include_returned)
        .await?;
    Ok(Json(loans))
}

#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct GetUserLoansQuery {
    #[serde(default)]
    pub include_returned: bool,
}

/// Create a new loan (borrow an item)
#[utoipa::path(
    post,
    path = "/loans",
    tag = "loans",
    security(("bearer_auth" = [])),
    request_body = CreateLoanRequest,
    responses(
        (status = 201, description = "Loan created", body = LoanResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "User or specimen not found"),
        (status = 409, description = "Specimen already borrowed or max loans reached")
    )
)]
pub async fn create_loan(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(request): Json<CreateLoanRequest>,
) -> AppResult<(StatusCode, Json<LoanResponse>)> {
    claims.require_write_borrows()?;

    let loan = CreateLoan {
        user_id: request.user_id,
        specimen_id: request.specimen_id,
        specimen_identification: request.specimen_identification,
        force: request.force.unwrap_or(false),
    };

    let (loan_id, issue_date) = state.services.loans.create_loan(loan).await?;

    Ok((
        StatusCode::CREATED,
        Json(LoanResponse {
            id: loan_id,
            issue_date,
            message: "Item borrowed successfully".to_string(),
        }),
    ))
}

/// Return a borrowed item
#[utoipa::path(
    post,
    path = "/loans/{id}/return",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Loan ID")
    ),
    responses(
        (status = 200, description = "Item returned", body = ReturnResponse),
        (status = 404, description = "Loan not found"),
        (status = 409, description = "Already returned")
    )
)]
pub async fn return_loan(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(loan_id): Path<i64>,
) -> AppResult<Json<ReturnResponse>> {
    claims.require_write_borrows()?;

    let loan = state.services.loans.return_loan(loan_id).await?;

    Ok(Json(ReturnResponse {
        status: "returned".to_string(),
        loan,
    }))
}

/// Renew a loan
#[utoipa::path(
    post,
    path = "/loans/{id}/renew",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Loan ID")
    ),
    responses(
        (status = 200, description = "Loan renewed", body = LoanResponse),
        (status = 404, description = "Loan not found"),
        (status = 409, description = "Max renewals reached or already returned")
    )
)]
pub async fn renew_loan(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(loan_id): Path<i64>,
) -> AppResult<Json<LoanResponse>> {
    claims.require_write_borrows()?;

    let (new_issue_date, renew_count) = state.services.loans.renew_loan(loan_id).await?;

    Ok(Json(LoanResponse {
        id: loan_id,
        issue_date: new_issue_date,
        message: format!("Loan renewed ({} renewals)", renew_count),
    }))
}

/// Return a borrowed item by specimen ID
#[utoipa::path(
    post,
    path = "/loans/specimens/{specimen_id}/return",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(
        ("specimen_id" = String, Path, description = "Specimen ID")
    ),
    responses(
        (status = 200, description = "Item returned", body = ReturnResponse),
        (status = 404, description = "Specimen or active loan not found"),
        (status = 409, description = "Already returned")
    )
)]
pub async fn return_loan_by_specimen(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(specimen_id): Path<String>,
) -> AppResult<Json<ReturnResponse>> {
    claims.require_write_borrows()?;

    let loan = state.services.loans.return_loan_by_specimen(&specimen_id).await?;

    Ok(Json(ReturnResponse {
        status: "returned".to_string(),
        loan,
    }))
}

/// Renew a loan by specimen ID
#[utoipa::path(
    post,
    path = "/loans/specimens/{specimen_id}/renew",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(
        ("specimen_id" = String, Path, description = "Specimen ID")
    ),
    responses(
        (status = 200, description = "Loan renewed", body = LoanResponse),
        (status = 404, description = "Specimen or active loan not found"),
        (status = 409, description = "Max renewals reached or already returned")
    )
)]
pub async fn renew_loan_by_specimen(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(specimen_id): Path<String>,
) -> AppResult<Json<LoanResponse>> {
    claims.require_write_borrows()?;

    let (loan_id, new_issue_date, renew_count) = state.services.loans.renew_loan_by_specimen(&specimen_id).await?;

    Ok(Json(LoanResponse {
        id: loan_id,
        issue_date: new_issue_date,
        message: format!("Loan renewed ({} renewals)", renew_count),
    }))
}
