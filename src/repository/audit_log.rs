//! Audit log table (`audit_log`) persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;

use super::Repository;
use crate::{
    error::AppResult,
    models::audit::{AuditLogEntry, AuditLogPage, AuditQueryParams},
};

/// DB access for `audit_log`. Implemented by [`Repository`].
#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn audit_insert(
        &self,
        event_type: &str,
        user_id: Option<i64>,
        entity_type: Option<&str>,
        entity_id: Option<i64>,
        ip_address: Option<&str>,
        payload: Option<Value>,
        outcome: &str,
        http_status: Option<i16>,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn audit_query_page(&self, params: AuditQueryParams) -> AppResult<AuditLogPage>;

    async fn audit_export(
        &self,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        event_type: Option<&str>,
    ) -> AppResult<Vec<AuditLogEntry>>;

    async fn audit_cleanup(&self, retention_days: u32) -> AppResult<u64>;
}

#[async_trait]
impl AuditLogRepository for Repository {
    async fn audit_insert(
        &self,
        event_type: &str,
        user_id: Option<i64>,
        entity_type: Option<&str>,
        entity_id: Option<i64>,
        ip_address: Option<&str>,
        payload: Option<Value>,
        outcome: &str,
        http_status: Option<i16>,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        Repository::audit_insert(
            self,
            event_type,
            user_id,
            entity_type,
            entity_id,
            ip_address,
            payload,
            outcome,
            http_status,
            error_code,
            error_message,
        )
        .await
    }

    async fn audit_query_page(&self, params: AuditQueryParams) -> AppResult<AuditLogPage> {
        Repository::audit_query_page(self, params).await
    }

    async fn audit_export(
        &self,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        event_type: Option<&str>,
    ) -> AppResult<Vec<AuditLogEntry>> {
        Repository::audit_export(self, from_date, to_date, event_type).await
    }

    async fn audit_cleanup(&self, retention_days: u32) -> AppResult<u64> {
        Repository::audit_cleanup(self, retention_days).await
    }
}

impl Repository {
    /// Insert one audit row (used from async tasks).
    pub async fn audit_insert(
        &self,
        event_type: &str,
        user_id: Option<i64>,
        entity_type: Option<&str>,
        entity_id: Option<i64>,
        ip_address: Option<&str>,
        payload: Option<Value>,
        outcome: &str,
        http_status: Option<i16>,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO audit_log (
                event_type, user_id, entity_type, entity_id, ip_address, payload,
                outcome, http_status, error_code, error_message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(event_type)
        .bind(user_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(ip_address)
        .bind(payload)
        .bind(outcome)
        .bind(http_status)
        .bind(error_code)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Query audit log entries with filters and pagination.
    pub async fn audit_query_page(&self, params: AuditQueryParams) -> AppResult<AuditLogPage> {
        let page = params.page.unwrap_or(1).max(1);
        let per_page = params.per_page.unwrap_or(50).clamp(1, 500);
        let offset = (page - 1) * per_page;

        let mut conditions: Vec<String> = Vec::new();
        let mut bind_idx = 1usize;

        if params.event_type.is_some() {
            conditions.push(format!("event_type = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.entity_type.is_some() {
            conditions.push(format!("entity_type = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.entity_id.is_some() {
            conditions.push(format!("entity_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.user_id.is_some() {
            conditions.push(format!("user_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.from_date.is_some() {
            conditions.push(format!("created_at >= ${}", bind_idx));
            bind_idx += 1;
        }
        if params.to_date.is_some() {
            conditions.push(format!("created_at <= ${}", bind_idx));
            bind_idx += 1;
        }
        if params.outcome.is_some() {
            conditions.push(format!("outcome = ${}", bind_idx));
            bind_idx += 1;
        }
        if params.error_code.is_some() {
            conditions.push(format!("error_code = ${}", bind_idx));
            bind_idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM audit_log {}", where_clause);
        let data_sql = format!(
            "SELECT id, event_type, outcome, user_id, entity_type, entity_id, ip_address, payload, \
             http_status, error_code, error_message, created_at \
             FROM audit_log {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            where_clause, bind_idx, bind_idx + 1,
        );

        let pool = &self.pool;
        let mut cq = sqlx::query_scalar::<sqlx::Postgres, i64>(&count_sql);
        if let Some(ref v) = params.event_type {
            cq = cq.bind(v.clone());
        }
        if let Some(ref v) = params.entity_type {
            cq = cq.bind(v.clone());
        }
        if let Some(v) = params.entity_id {
            cq = cq.bind(v);
        }
        if let Some(v) = params.user_id {
            cq = cq.bind(v);
        }
        if let Some(v) = params.from_date {
            cq = cq.bind(v);
        }
        if let Some(v) = params.to_date {
            cq = cq.bind(v);
        }
        if let Some(ref v) = params.outcome {
            cq = cq.bind(v.clone());
        }
        if let Some(ref v) = params.error_code {
            cq = cq.bind(v.clone());
        }

        let total: i64 = cq.fetch_one(pool).await?;

        let mut dq = sqlx::query(&data_sql);
        if let Some(ref v) = params.event_type {
            dq = dq.bind(v.clone());
        }
        if let Some(ref v) = params.entity_type {
            dq = dq.bind(v.clone());
        }
        if let Some(v) = params.entity_id {
            dq = dq.bind(v);
        }
        if let Some(v) = params.user_id {
            dq = dq.bind(v);
        }
        if let Some(v) = params.from_date {
            dq = dq.bind(v);
        }
        if let Some(v) = params.to_date {
            dq = dq.bind(v);
        }
        if let Some(ref v) = params.outcome {
            dq = dq.bind(v.clone());
        }
        if let Some(ref v) = params.error_code {
            dq = dq.bind(v.clone());
        }
        dq = dq.bind(per_page).bind(offset);

        let rows = dq.fetch_all(pool).await?;

        let entries = rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                event_type: row.get("event_type"),
                outcome: row.get("outcome"),
                user_id: row.get("user_id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                ip_address: row.get("ip_address"),
                payload: row.get("payload"),
                http_status: row.get("http_status"),
                error_code: row.get("error_code"),
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(AuditLogPage {
            entries,
            total,
            page,
            per_page,
        })
    }

    /// Export audit log entries for a date range (unbounded, for CSV/JSON export).
    pub async fn audit_export(
        &self,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        event_type: Option<&str>,
    ) -> AppResult<Vec<AuditLogEntry>> {
        let mut conditions = Vec::new();
        if from_date.is_some() {
            conditions.push("created_at >= $1");
        }
        if to_date.is_some() {
            conditions.push(if from_date.is_some() {
                "created_at <= $2"
            } else {
                "created_at <= $1"
            });
        }
        if event_type.is_some() {
            let idx = from_date.is_some() as usize + to_date.is_some() as usize + 1;
            conditions.push(Box::leak(format!("event_type = ${}", idx).into_boxed_str()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, event_type, outcome, user_id, entity_type, entity_id, ip_address, payload, \
             http_status, error_code, error_message, created_at \
             FROM audit_log {} ORDER BY created_at DESC LIMIT 50000",
            where_clause
        );

        let pool = &self.pool;
        let mut q = sqlx::query(&sql);
        if let Some(v) = from_date {
            q = q.bind(v);
        }
        if let Some(v) = to_date {
            q = q.bind(v);
        }
        if let Some(v) = event_type {
            q = q.bind(v);
        }

        let rows = q.fetch_all(pool).await?;

        Ok(rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                event_type: row.get("event_type"),
                outcome: row.get("outcome"),
                user_id: row.get("user_id"),
                entity_type: row.get("entity_type"),
                entity_id: row.get("entity_id"),
                ip_address: row.get("ip_address"),
                payload: row.get("payload"),
                http_status: row.get("http_status"),
                error_code: row.get("error_code"),
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// Delete audit log entries older than `retention_days` days; returns deleted count.
    pub async fn audit_cleanup(&self, retention_days: u32) -> AppResult<u64> {
        let deleted = sqlx::query_scalar::<_, i64>(
            "WITH deleted AS (DELETE FROM audit_log WHERE created_at < NOW() - ($1 || ' days')::INTERVAL RETURNING id) SELECT COUNT(*) FROM deleted",
        )
        .bind(retention_days as i64)
        .fetch_one(&self.pool)
        .await?;

        Ok(deleted as u64)
    }
}
