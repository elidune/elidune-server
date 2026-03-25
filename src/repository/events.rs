//! Events domain methods on Repository

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime, Utc};

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::event::{CreateEvent, Event, EventQuery, UpdateEvent},
};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EventsRepository: Send + Sync {
    async fn events_list(&self, query: &EventQuery) -> AppResult<(Vec<Event>, i64)>;
    async fn events_get_by_id(&self, id: i64) -> AppResult<Event>;
    async fn events_create(&self, data: &CreateEvent) -> AppResult<Event>;
    async fn events_update(&self, id: i64, data: &UpdateEvent) -> AppResult<Event>;
    async fn events_set_announcement_sent_at(&self, id: i64) -> AppResult<()>;
    async fn events_delete(&self, id: i64) -> AppResult<()>;
    async fn events_annual_stats(&self, year: i32) -> AppResult<EventAnnualStats>;
}

/// Combined repository trait used by [`crate::services::events::EventsService`].
pub trait EventsServiceRepository: EventsRepository + crate::repository::UsersRepository + Send + Sync {}

impl<T: EventsRepository + crate::repository::UsersRepository + Send + Sync> EventsServiceRepository for T {}

#[async_trait::async_trait]
impl EventsRepository for super::Repository {
    async fn events_list(&self, query: &crate::models::event::EventQuery) -> crate::error::AppResult<(Vec<crate::models::event::Event>, i64)> {
        super::Repository::events_list(self, query).await
    }
    async fn events_get_by_id(&self, id: i64) -> crate::error::AppResult<crate::models::event::Event> {
        super::Repository::events_get_by_id(self, id).await
    }
    async fn events_create(&self, data: &crate::models::event::CreateEvent) -> crate::error::AppResult<crate::models::event::Event> {
        super::Repository::events_create(self, data).await
    }
    async fn events_update(&self, id: i64, data: &crate::models::event::UpdateEvent) -> crate::error::AppResult<crate::models::event::Event> {
        super::Repository::events_update(self, id, data).await
    }
    async fn events_set_announcement_sent_at(&self, id: i64) -> crate::error::AppResult<()> {
        super::Repository::events_set_announcement_sent_at(self, id).await
    }
    async fn events_delete(&self, id: i64) -> crate::error::AppResult<()> {
        super::Repository::events_delete(self, id).await
    }
    async fn events_annual_stats(&self, year: i32) -> crate::error::AppResult<EventAnnualStats> {
        super::Repository::events_annual_stats(self, year).await
    }
}


impl Repository {
    /// List events with optional filters and pagination
    #[tracing::instrument(skip(self), err)]
    pub async fn events_list(&self, query: &EventQuery) -> AppResult<(Vec<Event>, i64)> {
        let page = query.page.unwrap_or(1);
        let per_page = query.per_page.unwrap_or(50);
        let offset = (page - 1) * per_page;

        let mut conditions = Vec::new();
        let mut idx = 1;

        if query.start_date.is_some() {
            conditions.push(format!("event_date >= ${}", idx));
            idx += 1;
        }
        if query.end_date.is_some() {
            conditions.push(format!("event_date <= ${}", idx));
            idx += 1;
        }
        if query.event_type.is_some() {
            conditions.push(format!("event_type = ${}", idx));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Parse dates once
        let start = query.start_date.as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let end = query.end_date.as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        // Count total
        let count_q = format!("SELECT COUNT(*) FROM events {}", where_clause);
        let mut count_builder = sqlx::query_scalar::<_, i64>(&count_q);
        if let Some(sd) = start { count_builder = count_builder.bind(sd); }
        if let Some(ed) = end { count_builder = count_builder.bind(ed); }
        if let Some(et) = query.event_type { count_builder = count_builder.bind(et); }
        let total = count_builder.fetch_one(&self.pool).await?;

        // Fetch rows
        let select_q = format!(
            "SELECT * FROM events {} ORDER BY event_date DESC LIMIT {} OFFSET {}",
            where_clause, per_page, offset
        );
        let mut builder = sqlx::query_as::<_, Event>(&select_q);
        if let Some(sd) = start { builder = builder.bind(sd); }
        if let Some(ed) = end { builder = builder.bind(ed); }
        if let Some(et) = query.event_type { builder = builder.bind(et); }

        let rows = builder.fetch_all(&self.pool).await?;
        Ok((rows, total))
    }

    /// Get event by ID
    #[tracing::instrument(skip(self), err)]
    pub async fn events_get_by_id(&self, id: i64) -> AppResult<Event> {
        sqlx::query_as::<_, Event>("SELECT * FROM events WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Event {} not found", id)))
    }

    /// Create an event
    #[tracing::instrument(skip(self), err)]
    pub async fn events_create(&self, data: &CreateEvent) -> AppResult<Event> {
        let event_date = NaiveDate::parse_from_str(&data.event_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("Invalid event_date".to_string()))?;
        let start_time = data.start_time.as_ref()
            .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok());
        let end_time = data.end_time.as_ref()
            .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok());

        let row = sqlx::query_as::<_, Event>(
            r#"
            INSERT INTO events (
                name, event_type, event_date, start_time, end_time,
                attendees_count, target_public,
                school_name, class_name, students_count,
                partner_name, description, notes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING *
            "#,
        )
        .bind(&data.name)
        .bind(data.event_type.unwrap_or(0))
        .bind(event_date)
        .bind(start_time)
        .bind(end_time)
        .bind(data.attendees_count)
        .bind(data.target_public)
        .bind(&data.school_name)
        .bind(&data.class_name)
        .bind(data.students_count)
        .bind(&data.partner_name)
        .bind(&data.description)
        .bind(&data.notes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Update an event
    #[tracing::instrument(skip(self), err)]
    pub async fn events_update(&self, id: i64, data: &UpdateEvent) -> AppResult<Event> {
        let now = Utc::now();
        let mut sets = vec!["update_at = $1".to_string()];
        let mut idx = 2;

        macro_rules! add_f {
            ($field:expr, $name:expr) => {
                if $field.is_some() { sets.push(format!("{} = ${}", $name, idx)); idx += 1; }
            };
        }

        add_f!(data.name, "name");
        add_f!(data.event_type, "event_type");
        add_f!(data.event_date, "event_date");
        add_f!(data.start_time, "start_time");
        add_f!(data.end_time, "end_time");
        add_f!(data.attendees_count, "attendees_count");
        add_f!(data.target_public, "target_public");
        add_f!(data.school_name, "school_name");
        add_f!(data.class_name, "class_name");
        add_f!(data.students_count, "students_count");
        add_f!(data.partner_name, "partner_name");
        add_f!(data.description, "description");
        add_f!(data.notes, "notes");

        let query = format!("UPDATE events SET {} WHERE id = {} RETURNING *", sets.join(", "), id);

        // Parse special types
        let event_date = data.event_date.as_ref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let start_time = data.start_time.as_ref()
            .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok());
        let end_time = data.end_time.as_ref()
            .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok());

        let mut builder = sqlx::query_as::<_, Event>(&query).bind(now);

        macro_rules! bind_f {
            ($field:expr) => {
                if let Some(ref val) = $field { builder = builder.bind(val); }
            };
        }

        bind_f!(data.name);
        bind_f!(data.event_type);
        if data.event_date.is_some() { builder = builder.bind(event_date); }
        if data.start_time.is_some() { builder = builder.bind(start_time); }
        if data.end_time.is_some() { builder = builder.bind(end_time); }
        bind_f!(data.attendees_count);
        bind_f!(data.target_public);
        bind_f!(data.school_name);
        bind_f!(data.class_name);
        bind_f!(data.students_count);
        bind_f!(data.partner_name);
        bind_f!(data.description);
        bind_f!(data.notes);

        builder
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Event {} not found", id)))
    }

    /// Set the announcement_sent_at timestamp on an event
    #[tracing::instrument(skip(self), err)]
    pub async fn events_set_announcement_sent_at(&self, id: i64) -> AppResult<()> {
        sqlx::query("UPDATE events SET announcement_sent_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete an event
    #[tracing::instrument(skip(self), err)]
    pub async fn events_delete(&self, id: i64) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM events WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Event {} not found", id)));
        }
        Ok(())
    }

    /// Get event stats for a year (for annual report)
    #[tracing::instrument(skip(self), err)]
    pub async fn events_annual_stats(&self, year: i32) -> AppResult<EventAnnualStats> {
        let start = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();

        // Total events and attendees
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_events,
                COALESCE(SUM(attendees_count), 0)::bigint as total_attendees
            FROM events
            WHERE event_date >= $1 AND event_date <= $2
            "#
        )
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await?;

        let total_events: i64 = sqlx::Row::get(&row, "total_events");
        let total_attendees: i64 = sqlx::Row::get(&row, "total_attendees");

        // School visits stats
        let school_row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_visits,
                COUNT(DISTINCT class_name) as distinct_classes,
                COALESCE(SUM(students_count), 0)::bigint as total_students
            FROM events
            WHERE event_date >= $1 AND event_date <= $2 AND event_type = 1
            "#
        )
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await?;

        let school_visits: i64 = sqlx::Row::get(&school_row, "total_visits");
        let distinct_classes: i64 = sqlx::Row::get(&school_row, "distinct_classes");
        let total_students: i64 = sqlx::Row::get(&school_row, "total_students");

        // Events by type
        let type_rows = sqlx::query(
            r#"
            SELECT event_type, COUNT(*) as count, COALESCE(SUM(attendees_count), 0)::bigint as attendees
            FROM events
            WHERE event_date >= $1 AND event_date <= $2
            GROUP BY event_type ORDER BY count DESC
            "#
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;

        let by_type: Vec<EventTypeStats> = type_rows.iter().map(|r| {
            EventTypeStats {
                event_type: sqlx::Row::get(r, "event_type"),
                count: sqlx::Row::get(r, "count"),
                attendees: sqlx::Row::get(r, "attendees"),
            }
        }).collect();

        Ok(EventAnnualStats {
            total_events,
            total_attendees,
            school_visits,
            distinct_classes,
            total_students,
            by_type,
        })
    }
}

/// Annual event statistics
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventAnnualStats {
    pub total_events: i64,
    pub total_attendees: i64,
    pub school_visits: i64,
    pub distinct_classes: i64,
    pub total_students: i64,
    pub by_type: Vec<EventTypeStats>,
}

/// Event stats by type
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventTypeStats {
    pub event_type: i16,
    pub count: i64,
    pub attendees: i64,
}

