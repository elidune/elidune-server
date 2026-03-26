//! Loan management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::AppResult,
    models::loan::{CreateLoan, LoanDetails},
    services::{
        audit::{self},
        reminders::{OverdueLoansPage, ReminderReport},
    },
};

use super::{biblios::PaginatedResponse, AuthenticatedUser, ClientIp};


/// Build the loans routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/loans", post(create_loan))
        .route("/loans/overdue", get(get_overdue_loans))
        .route("/loans/send-overdue-reminders", post(send_overdue_reminders))
        .route("/loans/:id/return", post(return_loan))
        .route("/loans/:id/renew", post(renew_loan))
        .route("/loans/items/:item_id/return", post(return_loan_by_item))
        .route("/loans/items/:item_id/renew", post(renew_loan_by_item))
}



/// Create loan request
#[serde_as]
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateLoanRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub item_id: Option<i64>,
    pub item_identification: Option<String>,
    pub force: Option<bool>,
}

#[derive(Serialize)]
struct LoanCreatedAudit {
    user_id: i64,
    item_id: Option<i64>,
    item_identification: Option<String>,
    force: bool,
    expiry_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct RenewLoanAudit {
    new_expiry_at: DateTime<Utc>,
    renew_count: i16,
}

#[derive(Serialize)]
struct RenewLoanByItemAudit {
    item_identification: String,
    new_expiry_at: DateTime<Utc>,
    renew_count: i16,
}

#[derive(Serialize)]
struct ReminderBatchManualAudit {
    triggered_by: &'static str,
    emails_sent: u32,
    loans_reminded: u32,
    errors: usize,
}

/// Loan response with calculated dates
#[serde_as]
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanResponse {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub expiry_at: DateTime<Utc>,
    pub message: String,
}

/// Return response with loan details
#[derive(Serialize, ToSchema)]
pub struct ReturnResponse {
    pub status: String,
    pub loan: LoanDetails,
}

/// Query parameters for overdue loans list
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct OverdueLoansQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Query parameters for sending reminders
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SendRemindersQuery {
    /// If true, no emails are sent; only shows what would be sent
    pub dry_run: Option<bool>,
}

/// Get loans for a specific user (paginated).
#[utoipa::path(
    get,
    path = "/users/{id}/loans",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "User ID"),
        GetUserLoansQuery
    ),
    responses(
        (status = 200, description = "User's loans", body = PaginatedResponse<LoanDetails>),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user_loans(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
    Query(query): Query<GetUserLoansQuery>,
) -> AppResult<Json<PaginatedResponse<LoanDetails>>> {
    claims.require_read_users()?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 200);

    let (items, total) = if query.archived.unwrap_or(false) {
        state
            .services
            .loans
            .get_user_archived_loans(user_id, page, per_page)
            .await?
    } else {
        state.services.loans.get_user_loans(user_id, page, per_page).await?
    };

    Ok(Json(PaginatedResponse::new(items, total, page, per_page)))
}

#[derive(Debug, Deserialize, Default, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetUserLoansQuery {
    /// If true, return past (returned) loans from the archive table
    pub archived: Option<bool>,
    /// Page number (1-based, default 1)
    pub page: Option<i64>,
    /// Page size (default 20, max 200)
    pub per_page: Option<i64>,
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
    ClientIp(ip): ClientIp,
    Json(request): Json<CreateLoanRequest>,
) -> AppResult<(StatusCode, Json<LoanResponse>)> {
    claims.require_write_borrows()?;
    let loan = CreateLoan {
        user_id: request.user_id,
        item_id: request.item_id,
        item_identification: request.item_identification.clone(),
        force: request.force.unwrap_or(false),
    };

    let (loan_id, expiry_at) = state.services.loans.create_loan(loan).await?;

    state.services.audit.log(
        audit::event::LOAN_CREATED,
        Some(claims.user_id),
        Some("loan"),
        Some(loan_id),
        ip,
        Some(LoanCreatedAudit {
            user_id: request.user_id,
            item_id: request.item_id,
            item_identification: request.item_identification.clone(),
            force: request.force.unwrap_or(false),
            expiry_at,
        }),
    );

    Ok((
        StatusCode::CREATED,
        Json(LoanResponse {
            id: loan_id,
            expiry_at,
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
    params(("id" = i32, Path, description = "Loan ID")),
    responses(
        (status = 200, description = "Item returned", body = ReturnResponse),
        (status = 404, description = "Loan not found"),
        (status = 409, description = "Already returned")
    )
)]
pub async fn return_loan(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(loan_id): Path<i64>,
) -> AppResult<Json<ReturnResponse>> {
    claims.require_write_borrows()?;
    let loan = state.services.loans.return_loan(loan_id).await?;

    state.services.audit.log(
        audit::event::LOAN_RETURNED,
        Some(claims.user_id),
        Some("loan"),
        Some(loan_id),
        ip,
        Some(&loan),
    );

    Ok(Json(ReturnResponse { status: "returned".to_string(), loan }))
}

/// Renew a loan
#[utoipa::path(
    post,
    path = "/loans/{id}/renew",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Loan ID")),
    responses(
        (status = 200, description = "Loan renewed", body = LoanResponse),
        (status = 404, description = "Loan not found"),
        (status = 409, description = "Max renewals reached or already returned")
    )
)]
pub async fn renew_loan(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(loan_id): Path<i64>,
) -> AppResult<Json<LoanResponse>> {
    claims.require_write_borrows()?;
    let (new_expiry_date, renew_count) = state.services.loans.renew_loan(loan_id).await?;

    state.services.audit.log(
        audit::event::LOAN_RENEWED,
        Some(claims.user_id),
        Some("loan"),
        Some(loan_id),
        ip,
        Some(RenewLoanAudit {
            new_expiry_at: new_expiry_date,
            renew_count,
        }),
    );

    Ok(Json(LoanResponse {
        id: loan_id,
        expiry_at: new_expiry_date,
        message: format!("Loan renewed ({} renewals)", renew_count),
    }))
}

/// Return a borrowed item by item identification (barcode or call number)
#[utoipa::path(
    post,
    path = "/loans/items/{item_id}/return",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(("item_id" = String, Path, description = "Item barcode or call number")),
    responses(
        (status = 200, description = "Item returned", body = ReturnResponse),
        (status = 404, description = "Item or active loan not found"),
        (status = 409, description = "Already returned")
    )
)]
pub async fn return_loan_by_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(item_id): Path<String>,
) -> AppResult<Json<ReturnResponse>> {
    claims.require_write_borrows()?;
    let loan = state.services.loans.return_loan_by_item(&item_id).await?;
    let loan_id = loan.id;

    state.services.audit.log(
        audit::event::LOAN_RETURNED,
        Some(claims.user_id),
        Some("loan"),
        Some(loan_id),
        ip,
        Some((item_id.as_str(), &loan)),
    );

    Ok(Json(ReturnResponse { status: "returned".to_string(), loan }))
}

/// Renew a loan by item identification (barcode or call number)
#[utoipa::path(
    post,
    path = "/loans/items/{item_id}/renew",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(("item_id" = String, Path, description = "Item barcode or call number")),
    responses(
        (status = 200, description = "Loan renewed", body = LoanResponse),
        (status = 404, description = "Item or active loan not found"),
        (status = 409, description = "Max renewals reached or already returned")
    )
)]
pub async fn renew_loan_by_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(item_id): Path<String>,
) -> AppResult<Json<LoanResponse>> {
    claims.require_write_borrows()?;
    let (loan_id, new_expiry_date, renew_count) = state
        .services
        .loans
        .renew_loan_by_item(&item_id)
        .await?;

    state.services.audit.log(
        audit::event::LOAN_RENEWED,
        Some(claims.user_id),
        Some("loan"),
        Some(loan_id),
        ip,
        Some(RenewLoanByItemAudit {
            item_identification: item_id,
            new_expiry_at: new_expiry_date,
            renew_count,
        }),
    );

    Ok(Json(LoanResponse {
        id: loan_id,
        expiry_at: new_expiry_date,
        message: format!("Loan renewed ({} renewals)", renew_count),
    }))
}

/// Get all overdue loans (admin dashboard)
#[utoipa::path(
    get,
    path = "/loans/overdue",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(OverdueLoansQuery),
    responses(
        (status = 200, description = "Paginated overdue loans", body = OverdueLoansPage),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn get_overdue_loans(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<OverdueLoansQuery>,
) -> AppResult<Json<OverdueLoansPage>> {
    claims.require_read_loans()?;

    let page = state
        .services
        .reminders
        .get_overdue_loans(
            query.page.unwrap_or(1),
            query.per_page.unwrap_or(50),
        )
        .await?;

    Ok(Json(page))
}

/// Trigger overdue reminder emails (admin only)
#[utoipa::path(
    post,
    path = "/loans/send-overdue-reminders",
    tag = "loans",
    security(("bearer_auth" = [])),
    params(SendRemindersQuery),
    responses(
        (status = 200, description = "Reminder report", body = ReminderReport),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn send_overdue_reminders(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Query(query): Query<SendRemindersQuery>,
) -> AppResult<Json<ReminderReport>> {
    claims.require_admin()?;

    let dry_run = query.dry_run.unwrap_or(false);

    let report = state
        .services
        .reminders
        .send_overdue_reminders(dry_run, Some(claims.user_id), ip.clone())
        .await?;

    if !dry_run {
        state.services.audit.log(
            audit::event::SYSTEM_REMINDERS_BATCH_COMPLETED,
            Some(claims.user_id),
            None,
            None,
            ip,
            Some(ReminderBatchManualAudit {
                triggered_by: "manual",
                emails_sent: report.emails_sent,
                loans_reminded: report.loans_reminded,
                errors: report.errors.len(),
            }),
        );
    }

    Ok(Json(report))
}

