//! Hold queue service (physical item holds).

use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::hold::{CreateHold, Hold, HoldDetails},
    repository::HoldsRepository,
};

#[derive(Clone)]
pub struct HoldsService {
    repository: Arc<dyn HoldsRepository>,
}

impl HoldsService {
    pub fn new(repository: Arc<dyn HoldsRepository>) -> Self {
        Self { repository }
    }

    /// Paginated list of all holds (newest first).
    #[tracing::instrument(skip(self), err)]
    pub async fn list_all(&self, page: i64, per_page: i64, active_only: bool) -> AppResult<(Vec<HoldDetails>, i64)> {
        self.repository.holds_list_all(page, per_page, active_only).await
    }

    /// Paginated holds for one user (`holds_rights == own` on `GET /holds`).
    #[tracing::instrument(skip(self), err)]
    pub async fn list_for_user_paginated(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
        active_only: bool,
    ) -> AppResult<(Vec<HoldDetails>, i64)> {
        self.repository
            .holds_list_for_user_paginated(user_id, page, per_page, active_only)
            .await
    }

    /// Place a hold — rejects if the user already has a pending/ready hold for this item.
    #[tracing::instrument(skip(self), err)]
    pub async fn place_hold(&self, data: CreateHold) -> AppResult<Hold> {
        if self
            .repository
            .holds_has_active_for_user_item(data.user_id, data.item_id)
            .await?
        {
            return Err(AppError::Conflict(
                "User already has an active hold for this item".to_string(),
            ));
        }

        self.repository.holds_create(&data).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_for_item(&self, item_id: i64) -> AppResult<Vec<HoldDetails>> {
        self.repository.holds_list_for_item(item_id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_for_user(&self, user_id: i64) -> AppResult<Vec<HoldDetails>> {
        self.repository.holds_list_for_user(user_id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn cancel(&self, id: i64, requesting_user_id: i64, can_manage_others: bool) -> AppResult<Hold> {
        let hold = self.repository.holds_get_by_id(id).await?;
        if !can_manage_others && hold.user_id != requesting_user_id {
            return Err(AppError::Authorization(
                "Cannot cancel another user's hold".to_string(),
            ));
        }
        self.repository.holds_cancel(id).await
    }

    /// Notify the first pending hold when a loan is returned.
    #[tracing::instrument(skip(self), err)]
    pub async fn notify_next(&self, item_id: i64, expiry_days: i32) -> AppResult<Option<Hold>> {
        self.repository.holds_notify_next(item_id, expiry_days).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn expire_overdue(&self) -> AppResult<Vec<i64>> {
        self.repository.holds_expire_overdue().await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn count_for_item(&self, item_id: i64) -> AppResult<i64> {
        self.repository.holds_count_for_item(item_id).await
    }

    /// Active holds (`pending` / `ready`) across all copies of a biblio.
    #[tracing::instrument(skip(self), err)]
    pub async fn count_active_for_biblio(&self, biblio_id: i64) -> AppResult<i64> {
        self.repository.holds_count_active_for_biblio(biblio_id).await
    }
}
