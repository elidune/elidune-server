//! Reservation / Hold service

use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::reservation::{CreateReservation, Reservation},
    repository::ReservationsRepository,
};

#[derive(Clone)]
pub struct ReservationsService {
    repository: Arc<dyn ReservationsRepository>,
}

impl ReservationsService {
    pub fn new(repository: Arc<dyn ReservationsRepository>) -> Self {
        Self { repository }
    }

    /// Place a hold — rejects if the user already has a pending/ready reservation for this item.
    #[tracing::instrument(skip(self), err)]
    pub async fn place_hold(&self, data: CreateReservation) -> AppResult<Reservation> {
        // Prevent duplicate reservations
        let existing = self
            .repository
            .reservations_list_for_user(data.user_id)
            .await?;
        if existing.iter().any(|r| {
            r.item_id == data.item_id
                && matches!(
                    r.status,
                    crate::models::reservation::ReservationStatus::Pending
                        | crate::models::reservation::ReservationStatus::Ready
                )
        }) {
            return Err(AppError::Conflict(
                "User already has an active reservation for this item".to_string(),
            ));
        }
        self.repository.reservations_create(&data).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_for_item(&self, item_id: i64) -> AppResult<Vec<Reservation>> {
        self.repository.reservations_list_for_item(item_id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_for_user(&self, user_id: i64) -> AppResult<Vec<Reservation>> {
        self.repository.reservations_list_for_user(user_id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn cancel(&self, id: i64, requesting_user_id: i64, is_staff: bool) -> AppResult<Reservation> {
        let reservation = self.repository.reservations_get_by_id(id).await?;
        if !is_staff && reservation.user_id != requesting_user_id {
            return Err(AppError::Authorization(
                "Cannot cancel another user's reservation".to_string(),
            ));
        }
        self.repository.reservations_cancel(id).await
    }

    /// Notify the first pending reservation when a loan is returned.
    /// Called by the loans service after a return.
    #[tracing::instrument(skip(self), err)]
    pub async fn notify_next(&self, item_id: i64, expiry_days: i32) -> AppResult<Option<Reservation>> {
        let next = self.repository.reservations_get_next_pending(item_id).await?;
        if let Some(ref r) = next {
            self.repository.reservations_mark_ready(r.id, expiry_days).await?;
        }
        Ok(next)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn expire_overdue(&self) -> AppResult<u64> {
        self.repository.reservations_expire_overdue().await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn count_for_item(&self, item_id: i64) -> AppResult<i64> {
        self.repository.reservations_count_for_item(item_id).await
    }
}
