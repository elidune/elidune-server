//! Fine / penalty endpoints

use axum::{
    extract::{Path, State},
    Json,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    models::fine::{Fine, FineRule, PayFineRequest, WaiveFineRequest},
    services::audit,
};

use super::{AuthenticatedUser, ClientIp, StaffUser};

/// Upsert fine rule request
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertFineRuleRequest {
    pub media_type: Option<String>,
    pub daily_rate: Decimal,
    pub max_amount: Option<Decimal>,
    #[serde(default)]
    pub grace_days: i32,
}

/// Unpaid fine summary for a user
#[derive(Serialize, ToSchema)]
pub struct UnpaidFinesSummary {
    pub total_unpaid: Decimal,
    pub fines: Vec<Fine>,
}

/// List all fines for a user
#[utoipa::path(
    get,
    path = "/users/{id}/fines",
    tag = "fines",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "User fines with total unpaid", body = UnpaidFinesSummary),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "User not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_user_fines(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(user_id): Path<i64>,
) -> AppResult<Json<UnpaidFinesSummary>> {
    claims.require_read_users()?;
    let fines = state.services.fines.list_for_user(user_id).await?;
    let total_unpaid = state.services.fines.total_unpaid(user_id).await?;
    Ok(Json(UnpaidFinesSummary { total_unpaid, fines }))
}

/// Pay a fine (record payment)
#[utoipa::path(
    post,
    path = "/fines/{id}/pay",
    tag = "fines",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Fine ID")),
    request_body = PayFineRequest,
    responses(
        (status = 200, description = "Fine updated with payment", body = Fine),
        (status = 400, description = "Invalid amount", body = crate::error::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Fine not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn pay_fine(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(req): Json<PayFineRequest>,
) -> AppResult<Json<Fine>> {
    let fine = state.services.fines.pay(id, req.amount, req.notes.as_deref()).await?;

    state.services.audit.log(
        audit::event::FINE_PAID,
        Some(claims.user_id),
        Some("fine"),
        Some(id),
        ip,
        Some(serde_json::json!({ "amount": req.amount })),
    );

    Ok(Json(fine))
}

/// Waive a fine
#[utoipa::path(
    post,
    path = "/fines/{id}/waive",
    tag = "fines",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Fine ID")),
    request_body = WaiveFineRequest,
    responses(
        (status = 200, description = "Fine waived", body = Fine),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Staff access required", body = crate::error::ErrorResponse),
        (status = 404, description = "Fine not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn waive_fine(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(req): Json<WaiveFineRequest>,
) -> AppResult<Json<Fine>> {
    let fine = state.services.fines.waive(id, req.notes.as_deref()).await?;

    state.services.audit.log(
        audit::event::FINE_WAIVED,
        Some(claims.user_id),
        Some("fine"),
        Some(id),
        ip,
        None::<()>,
    );

    Ok(Json(fine))
}

/// List fine rules
#[utoipa::path(
    get,
    path = "/fines/rules",
    tag = "fines",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Fine rules per media type", body = Vec<FineRule>),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_fine_rules(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<Vec<FineRule>>> {
    claims.require_read_settings()?;
    Ok(Json(state.services.fines.list_rules().await?))
}

/// Upsert a fine rule (admin only)
#[utoipa::path(
    put,
    path = "/fines/rules",
    tag = "fines",
    security(("bearer_auth" = [])),
    request_body = UpsertFineRuleRequest,
    responses(
        (status = 200, description = "Fine rule saved", body = FineRule),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 403, description = "Staff access required", body = crate::error::ErrorResponse)
    )
)]
pub async fn upsert_fine_rule(
    State(state): State<crate::AppState>,
    StaffUser(_staff): StaffUser,
    Json(req): Json<UpsertFineRuleRequest>,
) -> AppResult<Json<FineRule>> {
    let rule = state
        .services
        .fines
        .upsert_rule(
            req.media_type.as_deref(),
            req.daily_rate,
            req.max_amount,
            req.grace_days,
        )
        .await?;
    Ok(Json(rule))
}

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/users/:id/fines", get(list_user_fines))
        .route("/fines/rules", get(list_fine_rules).put(upsert_fine_rule))
        .route("/fines/:id/pay", post(pay_fine))
        .route("/fines/:id/waive", post(waive_fine))
}
