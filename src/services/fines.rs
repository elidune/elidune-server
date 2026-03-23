//! Fine / penalty service

use std::sync::Arc;

use rust_decimal::Decimal;

use crate::{
    error::{AppError, AppResult},
    models::fine::{Fine, FineRule},
    repository::FinesRepository,
};

#[derive(Clone)]
pub struct FinesService {
    repository: Arc<dyn FinesRepository>,
}

impl FinesService {
    pub fn new(repository: Arc<dyn FinesRepository>) -> Self {
        Self { repository }
    }

    /// List all fines for a user
    #[tracing::instrument(skip(self), err)]
    pub async fn list_for_user(&self, user_id: i64) -> AppResult<Vec<Fine>> {
        self.repository.fines_list_for_user(user_id).await
    }

    /// Get a specific fine
    #[tracing::instrument(skip(self), err)]
    pub async fn get(&self, id: i64) -> AppResult<Fine> {
        self.repository.fines_get_by_id(id).await
    }

    /// Accrue a fine for an overdue loan (calculates amount from rules)
    #[tracing::instrument(skip(self), err)]
    pub async fn accrue(
        &self,
        loan_id: i64,
        user_id: i64,
        media_type: Option<&str>,
        overdue_days: i64,
    ) -> AppResult<Fine> {
        let rules = self.repository.fines_list_rules().await?;
        // Look for media-type specific rule first, then default
        let rule = rules
            .iter()
            .find(|r| r.media_type.as_deref() == media_type)
            .or_else(|| rules.iter().find(|r| r.media_type.is_none()))
            .ok_or_else(|| AppError::Internal("No fine rule configured".to_string()))?;

        let effective_days = (overdue_days - rule.grace_days as i64).max(0);
        let mut amount = rule.daily_rate * Decimal::from(effective_days);
        if let Some(max) = rule.max_amount {
            amount = amount.min(max);
        }

        if amount <= Decimal::ZERO {
            return Err(AppError::BusinessRule(
                "Fine amount is zero — within grace period".to_string(),
            ));
        }

        self.repository.fines_create(loan_id, user_id, amount, None).await
    }

    /// Apply a payment to a fine
    #[tracing::instrument(skip(self), err)]
    pub async fn pay(&self, id: i64, amount: Decimal, notes: Option<&str>) -> AppResult<Fine> {
        if amount <= Decimal::ZERO {
            return Err(AppError::Validation("Payment amount must be positive".to_string()));
        }
        self.repository.fines_pay(id, amount, notes).await
    }

    /// Waive (write off) a fine
    #[tracing::instrument(skip(self), err)]
    pub async fn waive(&self, id: i64, notes: Option<&str>) -> AppResult<Fine> {
        self.repository.fines_waive(id, notes).await
    }

    /// Get total unpaid fines for a user
    #[tracing::instrument(skip(self), err)]
    pub async fn total_unpaid(&self, user_id: i64) -> AppResult<Decimal> {
        self.repository.fines_total_unpaid(user_id).await
    }

    /// List fine rules
    #[tracing::instrument(skip(self), err)]
    pub async fn list_rules(&self) -> AppResult<Vec<FineRule>> {
        self.repository.fines_list_rules().await
    }

    /// Upsert a fine rule
    #[tracing::instrument(skip(self), err)]
    pub async fn upsert_rule(
        &self,
        media_type: Option<&str>,
        daily_rate: Decimal,
        max_amount: Option<Decimal>,
        grace_days: i32,
    ) -> AppResult<FineRule> {
        if daily_rate < Decimal::ZERO {
            return Err(AppError::Validation("Daily rate cannot be negative".to_string()));
        }
        self.repository
            .fines_upsert_rule(media_type, daily_rate, max_amount, grace_days)
            .await
    }
}
