//! Schedules domain methods on Repository (periods, slots, closures)

use chrono::{NaiveDate, NaiveTime, Utc};
use sqlx::{Pool, Postgres};

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::schedule::{
        CreateScheduleClosure, CreateSchedulePeriod, CreateScheduleSlot,
        ScheduleClosure, SchedulePeriod, ScheduleSlot, UpdateSchedulePeriod,
    },
};

impl Repository {
    // ---- Periods ----

    /// List all schedule periods, ordered by start_date desc
    pub async fn schedules_list_periods(&self) -> AppResult<Vec<SchedulePeriod>> {
        let rows = sqlx::query_as::<_, SchedulePeriod>(
            "SELECT * FROM schedule_periods ORDER BY start_date DESC"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get a schedule period by ID
    pub async fn schedules_get_period(&self, id: i64) -> AppResult<SchedulePeriod> {
        sqlx::query_as::<_, SchedulePeriod>("SELECT * FROM schedule_periods WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Schedule period {} not found", id)))
    }

    /// Create a schedule period
    pub async fn schedules_create_period(&self, data: &CreateSchedulePeriod) -> AppResult<SchedulePeriod> {
        let start = NaiveDate::parse_from_str(&data.start_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("Invalid start_date".to_string()))?;
        let end = NaiveDate::parse_from_str(&data.end_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("Invalid end_date".to_string()))?;

        let row = sqlx::query_as::<_, SchedulePeriod>(
            r#"
            INSERT INTO schedule_periods (name, start_date, end_date, notes)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(&data.name)
        .bind(start)
        .bind(end)
        .bind(&data.notes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Update a schedule period
    pub async fn schedules_update_period(&self, id: i64, data: &UpdateSchedulePeriod) -> AppResult<SchedulePeriod> {
        let now = Utc::now();
        let mut sets = vec!["modif_date = $1".to_string()];
        let mut idx = 2;

        if data.name.is_some() { sets.push(format!("name = ${}", idx)); idx += 1; }
        if data.start_date.is_some() { sets.push(format!("start_date = ${}", idx)); idx += 1; }
        if data.end_date.is_some() { sets.push(format!("end_date = ${}", idx)); idx += 1; }
        if data.notes.is_some() { sets.push(format!("notes = ${}", idx)); }

        let query = format!("UPDATE schedule_periods SET {} WHERE id = {} RETURNING *", sets.join(", "), id);

        let start = data.start_date.as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let end = data.end_date.as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let mut builder = sqlx::query_as::<_, SchedulePeriod>(&query).bind(now);
        if let Some(ref name) = data.name { builder = builder.bind(name); }
        if let Some(sd) = start { builder = builder.bind(sd); }
        if let Some(ed) = end { builder = builder.bind(ed); }
        if let Some(ref notes) = data.notes { builder = builder.bind(notes); }

        builder
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Schedule period {} not found", id)))
    }

    /// Delete a schedule period (cascade deletes slots)
    pub async fn schedules_delete_period(&self, id: i64) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM schedule_periods WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Schedule period {} not found", id)));
        }
        Ok(())
    }

    // ---- Slots ----

    /// List slots for a given period
    pub async fn schedules_list_slots(&self, period_id: i64) -> AppResult<Vec<ScheduleSlot>> {
        let rows = sqlx::query_as::<_, ScheduleSlot>(
            "SELECT * FROM schedule_slots WHERE period_id = $1 ORDER BY day_of_week, open_time"
        )
        .bind(period_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Create a slot for a period
    pub async fn schedules_create_slot(&self, period_id: i64, data: &CreateScheduleSlot) -> AppResult<ScheduleSlot> {
        let open = NaiveTime::parse_from_str(&data.open_time, "%H:%M")
            .map_err(|_| AppError::Validation("Invalid open_time (use HH:MM)".to_string()))?;
        let close = NaiveTime::parse_from_str(&data.close_time, "%H:%M")
            .map_err(|_| AppError::Validation("Invalid close_time (use HH:MM)".to_string()))?;

        let row = sqlx::query_as::<_, ScheduleSlot>(
            r#"
            INSERT INTO schedule_slots (period_id, day_of_week, open_time, close_time)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(period_id)
        .bind(data.day_of_week)
        .bind(open)
        .bind(close)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Delete a slot
    pub async fn schedules_delete_slot(&self, id: i64) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM schedule_slots WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Schedule slot {} not found", id)));
        }
        Ok(())
    }

    // ---- Closures ----

    /// List closures, optionally filtered by date range
    pub async fn schedules_list_closures(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> AppResult<Vec<ScheduleClosure>> {
        let mut conditions = Vec::new();
        let mut idx = 1;

        if start_date.is_some() {
            conditions.push(format!("closure_date >= ${}", idx));
            idx += 1;
        }
        if end_date.is_some() {
            conditions.push(format!("closure_date <= ${}", idx));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let query = format!(
            "SELECT * FROM schedule_closures {} ORDER BY closure_date",
            where_clause
        );

        let mut builder = sqlx::query_as::<_, ScheduleClosure>(&query);
        if let Some(sd) = start_date { builder = builder.bind(sd); }
        if let Some(ed) = end_date { builder = builder.bind(ed); }

        let rows = builder.fetch_all(&self.pool).await?;
        Ok(rows)
    }

    /// Count opening days for a year (excluding closures)
    pub async fn schedules_count_opening_days(&self, year: i32) -> AppResult<i64> {
        let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();

        // Count closure days in the year
        let closures: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM schedule_closures WHERE closure_date >= $1 AND closure_date <= $2"
        )
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await?;

        // Count days that have at least one slot in any active period
        let scheduled_days: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT ss.day_of_week)
            FROM schedule_slots ss
            JOIN schedule_periods sp ON ss.period_id = sp.id
            WHERE sp.start_date <= $2 AND sp.end_date >= $1
            "#
        )
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await?;

        // Approximate: scheduled_days_per_week * 52 - closures
        let approx_days = scheduled_days * 52 - closures;
        Ok(approx_days.max(0))
    }

    /// Calculate weekly opening hours from schedule slots for a year
    pub async fn schedules_weekly_hours(&self, year: i32) -> AppResult<f64> {
        let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();

        // Get the sum of hours per week from the most recent period covering the year
        let result: Option<f64> = sqlx::query_scalar(
            r#"
            SELECT SUM(EXTRACT(EPOCH FROM (close_time - open_time)) / 3600.0)::float8
            FROM schedule_slots ss
            JOIN schedule_periods sp ON ss.period_id = sp.id
            WHERE sp.start_date <= $2 AND sp.end_date >= $1
            GROUP BY sp.id
            ORDER BY sp.start_date DESC
            LIMIT 1
            "#
        )
        .bind(start)
        .bind(end)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.unwrap_or(0.0))
    }

    /// Create a closure
    pub async fn schedules_create_closure(&self, data: &CreateScheduleClosure) -> AppResult<ScheduleClosure> {
        let date = NaiveDate::parse_from_str(&data.closure_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("Invalid closure_date".to_string()))?;

        let row = sqlx::query_as::<_, ScheduleClosure>(
            "INSERT INTO schedule_closures (closure_date, reason) VALUES ($1, $2) RETURNING *"
        )
        .bind(date)
        .bind(&data.reason)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Delete a closure
    pub async fn schedules_delete_closure(&self, id: i64) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM schedule_closures WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Closure {} not found", id)));
        }
        Ok(())
    }
}

