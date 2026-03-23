//! Fine domain methods on Repository

use async_trait::async_trait;
use rust_decimal::Decimal;
use snowflaked::Generator;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::fine::{Fine, FineRule, FineStatus},
};

#[async_trait]
pub trait FinesRepository: Send + Sync {
    async fn fines_list_for_user(&self, user_id: i64) -> AppResult<Vec<Fine>>;
    async fn fines_get_by_id(&self, id: i64) -> AppResult<Fine>;
    async fn fines_create(
        &self,
        loan_id: i64,
        user_id: i64,
        amount: Decimal,
        notes: Option<&str>,
    ) -> AppResult<Fine>;
    async fn fines_pay(
        &self,
        id: i64,
        payment: Decimal,
        notes: Option<&str>,
    ) -> AppResult<Fine>;
    async fn fines_waive(&self, id: i64, notes: Option<&str>) -> AppResult<Fine>;
    async fn fines_list_rules(&self) -> AppResult<Vec<FineRule>>;
    async fn fines_upsert_rule(
        &self,
        media_type: Option<&str>,
        daily_rate: Decimal,
        max_amount: Option<Decimal>,
        grace_days: i32,
    ) -> AppResult<FineRule>;
    async fn fines_total_unpaid(&self, user_id: i64) -> AppResult<Decimal>;
}

#[async_trait::async_trait]
impl FinesRepository for Repository {
    async fn fines_list_for_user(&self, user_id: i64) -> AppResult<Vec<Fine>> {
        Repository::fines_list_for_user(self, user_id).await
    }
    async fn fines_get_by_id(&self, id: i64) -> AppResult<Fine> {
        Repository::fines_get_by_id(self, id).await
    }
    async fn fines_create(
        &self, loan_id: i64, user_id: i64, amount: Decimal, notes: Option<&str>,
    ) -> AppResult<Fine> {
        Repository::fines_create(self, loan_id, user_id, amount, notes).await
    }
    async fn fines_pay(
        &self, id: i64, payment: Decimal, notes: Option<&str>,
    ) -> AppResult<Fine> {
        Repository::fines_pay(self, id, payment, notes).await
    }
    async fn fines_waive(&self, id: i64, notes: Option<&str>) -> AppResult<Fine> {
        Repository::fines_waive(self, id, notes).await
    }
    async fn fines_list_rules(&self) -> AppResult<Vec<FineRule>> {
        Repository::fines_list_rules(self).await
    }
    async fn fines_upsert_rule(
        &self,
        media_type: Option<&str>,
        daily_rate: Decimal,
        max_amount: Option<Decimal>,
        grace_days: i32,
    ) -> AppResult<FineRule> {
        Repository::fines_upsert_rule(self, media_type, daily_rate, max_amount, grace_days).await
    }
    async fn fines_total_unpaid(&self, user_id: i64) -> AppResult<Decimal> {
        Repository::fines_total_unpaid(self, user_id).await
    }
}


static SNOWFLAKE: std::sync::LazyLock<std::sync::Mutex<Generator>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Generator::new(2)));

fn next_id() -> i64 {
    SNOWFLAKE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .generate::<i64>()
}

impl Repository {
    /// List fines for a user
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_list_for_user(&self, user_id: i64) -> AppResult<Vec<Fine>> {
        let rows = sqlx::query_as::<_, Fine>(
            "SELECT * FROM fines WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get a fine by ID
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_get_by_id(&self, id: i64) -> AppResult<Fine> {
        sqlx::query_as::<_, Fine>("SELECT * FROM fines WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Fine {id} not found")))
    }

    /// Create a fine for a loan
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_create(
        &self,
        loan_id: i64,
        user_id: i64,
        amount: Decimal,
        notes: Option<&str>,
    ) -> AppResult<Fine> {
        let id = next_id();
        let row = sqlx::query_as::<_, Fine>(
            r#"
            INSERT INTO fines (id, loan_id, user_id, amount, notes)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(loan_id)
        .bind(user_id)
        .bind(amount)
        .bind(notes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Apply a payment to a fine
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_pay(
        &self,
        id: i64,
        payment: Decimal,
        notes: Option<&str>,
    ) -> AppResult<Fine> {
        sqlx::query_as::<_, Fine>(
            r#"
            UPDATE fines SET
                paid_amount = paid_amount + $2,
                notes       = COALESCE($3, notes),
                paid_at     = CASE WHEN paid_amount + $2 >= amount THEN NOW() ELSE NULL END,
                status      = CASE
                    WHEN paid_amount + $2 >= amount THEN 'paid'
                    ELSE 'partial'
                END
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(payment)
        .bind(notes)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Fine {id} not found")))
    }

    /// Waive a fine (write off)
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_waive(&self, id: i64, notes: Option<&str>) -> AppResult<Fine> {
        sqlx::query_as::<_, Fine>(
            "UPDATE fines SET status = 'waived', paid_at = NOW(), notes = COALESCE($2, notes)
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(notes)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Fine {id} not found")))
    }

    /// Get fine rules (per media type + default)
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_list_rules(&self) -> AppResult<Vec<FineRule>> {
        let rows = sqlx::query_as::<_, FineRule>(
            "SELECT * FROM fine_rules ORDER BY media_type NULLS FIRST",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Upsert a fine rule for a media type (or default)
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_upsert_rule(
        &self,
        media_type: Option<&str>,
        daily_rate: Decimal,
        max_amount: Option<Decimal>,
        grace_days: i32,
    ) -> AppResult<FineRule> {
        let row = sqlx::query_as::<_, FineRule>(
            r#"
            INSERT INTO fine_rules (media_type, daily_rate, max_amount, grace_days)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (media_type) DO UPDATE
            SET daily_rate = $2, max_amount = $3, grace_days = $4
            RETURNING *
            "#,
        )
        .bind(media_type)
        .bind(daily_rate)
        .bind(max_amount)
        .bind(grace_days)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Sum of unpaid fines for a user
    #[tracing::instrument(skip(self), err)]
    pub async fn fines_total_unpaid(&self, user_id: i64) -> AppResult<Decimal> {
        let total: Option<Decimal> = sqlx::query_scalar(
            "SELECT SUM(amount - paid_amount) FROM fines
             WHERE user_id = $1 AND status IN ('pending','partial')",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(total.unwrap_or(Decimal::ZERO))
    }
}

