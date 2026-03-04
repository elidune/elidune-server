//! Visitor counts domain methods on Repository

use chrono::NaiveDate;
use sqlx::{Pool, Postgres};

use super::Repository;
use crate::{
    error::AppResult,
    models::visitor_count::{CreateVisitorCount, VisitorCount},
};

impl Repository {
    /// List visitor counts, optionally filtered by date range
    pub async fn visitor_counts_list(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> AppResult<Vec<VisitorCount>> {
        let mut conditions = Vec::new();
        let mut idx = 1;

        if start_date.is_some() {
            conditions.push(format!("count_date >= ${}", idx));
            idx += 1;
        }
        if end_date.is_some() {
            conditions.push(format!("count_date <= ${}", idx));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let query = format!(
            "SELECT * FROM visitor_counts {} ORDER BY count_date DESC",
            where_clause
        );

        let mut builder = sqlx::query_as::<_, VisitorCount>(&query);
        if let Some(sd) = start_date {
            builder = builder.bind(sd);
        }
        if let Some(ed) = end_date {
            builder = builder.bind(ed);
        }

        let rows = builder.fetch_all(&self.pool).await?;
        Ok(rows)
    }

    /// Get total visitor count for a date range
    pub async fn visitor_counts_total(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> AppResult<i64> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(count), 0)::bigint FROM visitor_counts WHERE count_date >= $1 AND count_date <= $2"
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_one(&self.pool)
        .await?;
        Ok(total)
    }

    /// Create a new visitor count record
    pub async fn visitor_counts_create(&self, data: &CreateVisitorCount) -> AppResult<VisitorCount> {
        let count_date = NaiveDate::parse_from_str(&data.count_date, "%Y-%m-%d")
            .map_err(|_| crate::error::AppError::Validation("Invalid count_date format".to_string()))?;

        let row = sqlx::query_as::<_, VisitorCount>(
            r#"
            INSERT INTO visitor_counts (count_date, count, source, notes)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(count_date)
        .bind(data.count)
        .bind(&data.source)
        .bind(&data.notes)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    /// Delete a visitor count record
    pub async fn visitor_counts_delete(&self, id: i64) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM visitor_counts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(crate::error::AppError::NotFound(
                format!("Visitor count with id {} not found", id),
            ));
        }
        Ok(())
    }
}

