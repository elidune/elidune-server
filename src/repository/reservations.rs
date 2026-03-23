//! Reservation domain methods on Repository

use async_trait::async_trait;
use chrono::Utc;
use snowflaked::Generator;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::reservation::{CreateReservation, Reservation, ReservationStatus},
};

#[async_trait]
pub trait ReservationsRepository: Send + Sync {
    /// List reservations for a physical item (items table)
    async fn reservations_list_for_item(&self, item_id: i64) -> AppResult<Vec<Reservation>>;
    async fn reservations_list_for_user(&self, user_id: i64) -> AppResult<Vec<Reservation>>;
    async fn reservations_get_by_id(&self, id: i64) -> AppResult<Reservation>;
    async fn reservations_create(&self, data: &CreateReservation) -> AppResult<Reservation>;
    async fn reservations_mark_ready(&self, id: i64, expiry_days: i32) -> AppResult<Reservation>;
    async fn reservations_cancel(&self, id: i64) -> AppResult<Reservation>;
    async fn reservations_expire_overdue(&self) -> AppResult<u64>;
    async fn reservations_count_for_item(&self, item_id: i64) -> AppResult<i64>;
    async fn reservations_get_next_pending(&self, item_id: i64) -> AppResult<Option<Reservation>>;
    async fn reservations_fulfill(&self, id: i64) -> AppResult<Reservation>;
}

#[async_trait::async_trait]
impl ReservationsRepository for Repository {
    async fn reservations_list_for_item(&self, item_id: i64) -> AppResult<Vec<Reservation>> {
        Repository::reservations_list_for_item(self, item_id).await
    }
    async fn reservations_list_for_user(&self, user_id: i64) -> AppResult<Vec<Reservation>> {
        Repository::reservations_list_for_user(self, user_id).await
    }
    async fn reservations_get_by_id(&self, id: i64) -> AppResult<Reservation> {
        Repository::reservations_get_by_id(self, id).await
    }
    async fn reservations_create(&self, data: &CreateReservation) -> AppResult<Reservation> {
        Repository::reservations_create(self, data).await
    }
    async fn reservations_mark_ready(&self, id: i64, expiry_days: i32) -> AppResult<Reservation> {
        Repository::reservations_mark_ready(self, id, expiry_days).await
    }
    async fn reservations_cancel(&self, id: i64) -> AppResult<Reservation> {
        Repository::reservations_cancel(self, id).await
    }
    async fn reservations_expire_overdue(&self) -> AppResult<u64> {
        Repository::reservations_expire_overdue(self).await
    }
    async fn reservations_count_for_item(&self, item_id: i64) -> AppResult<i64> {
        Repository::reservations_count_for_item(self, item_id).await
    }
    async fn reservations_get_next_pending(&self, item_id: i64) -> AppResult<Option<Reservation>> {
        Repository::reservations_get_next_pending(self, item_id).await
    }
    async fn reservations_fulfill(&self, id: i64) -> AppResult<Reservation> {
        Repository::reservations_fulfill(self, id).await
    }
}


static SNOWFLAKE: std::sync::LazyLock<std::sync::Mutex<Generator>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Generator::new(1)));

fn next_id() -> i64 {
    SNOWFLAKE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .generate::<i64>()
}

impl Repository {
    /// List reservations for a specific item (queue)
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_list_for_item(
        &self,
        item_id: i64,
    ) -> AppResult<Vec<Reservation>> {
        let rows = sqlx::query_as::<_, Reservation>(
            "SELECT * FROM reservations WHERE item_id = $1 AND status IN ('pending','ready')
             ORDER BY position ASC",
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// List reservations for a specific user
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_list_for_user(
        &self,
        user_id: i64,
    ) -> AppResult<Vec<Reservation>> {
        let rows = sqlx::query_as::<_, Reservation>(
            "SELECT * FROM reservations WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get a reservation by ID
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_get_by_id(&self, id: i64) -> AppResult<Reservation> {
        sqlx::query_as::<_, Reservation>("SELECT * FROM reservations WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Reservation {id} not found")))
    }

    /// Create a reservation (append to queue for the item)
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_create(
        &self,
        data: &CreateReservation,
    ) -> AppResult<Reservation> {
        let id = next_id();
        let row = sqlx::query_as::<_, Reservation>(
            r#"
            INSERT INTO reservations (id, user_id, item_id, position, notes)
            VALUES (
                $1, $2, $3,
                COALESCE((SELECT MAX(position) FROM reservations
                          WHERE item_id = $3 AND status IN ('pending','ready')), 0) + 1,
                $4
            )
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(data.user_id)
        .bind(data.item_id)
        .bind(&data.notes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Mark a reservation as ready (item is available), set expiry
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_mark_ready(
        &self,
        id: i64,
        expiry_days: i32,
    ) -> AppResult<Reservation> {
        let expires_at = Utc::now() + chrono::Duration::days(expiry_days as i64);
        sqlx::query_as::<_, Reservation>(
            r#"UPDATE reservations
               SET status = 'ready', notified_at = NOW(), expires_at = $2
               WHERE id = $1 AND status = 'pending'
               RETURNING *"#,
        )
        .bind(id)
        .bind(expires_at)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Pending reservation {id} not found")))
    }

    /// Cancel a reservation
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_cancel(&self, id: i64) -> AppResult<Reservation> {
        sqlx::query_as::<_, Reservation>(
            "UPDATE reservations SET status = 'cancelled' WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Reservation {id} not found")))
    }

    /// Mark expired reservations as expired and re-queue next pending
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_expire_overdue(&self) -> AppResult<u64> {
        let result = sqlx::query(
            "UPDATE reservations SET status = 'expired'
             WHERE status = 'ready' AND expires_at < NOW()",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Count pending reservations for an item
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_count_for_item(&self, item_id: i64) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM reservations WHERE item_id = $1 AND status IN ('pending','ready')",
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Notify next pending reservation for an item when it becomes available
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_get_next_pending(
        &self,
        item_id: i64,
    ) -> AppResult<Option<Reservation>> {
        let row = sqlx::query_as::<_, Reservation>(
            "SELECT * FROM reservations WHERE item_id = $1 AND status = 'pending'
             ORDER BY position ASC LIMIT 1",
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Mark a reservation fulfilled (loan was created for it)
    #[tracing::instrument(skip(self), err)]
    pub async fn reservations_fulfill(&self, id: i64) -> AppResult<Reservation> {
        sqlx::query_as::<_, Reservation>(
            "UPDATE reservations SET status = 'fulfilled' WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Reservation {id} not found")))
    }
}

