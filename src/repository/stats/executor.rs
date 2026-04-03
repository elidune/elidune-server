//! Execute dynamic stats SQL with a timeout and map rows to JSON.

use std::time::Duration;

use sqlx::{Column, Row, TypeInfo};
use sqlx::postgres::PgRow;
use sqlx::PgPool;

use crate::error::AppError;
use crate::models::stats_builder::{ColumnMeta, StatsTableResponse};

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

fn labeled_sql(data_sql: &str, count_sql: &str) -> String {
    format!(
        "-- data\n{}\n\n-- count\n{}",
        data_sql.trim_end(),
        count_sql.trim_end()
    )
}

fn sql_failure_response(
    limit: u32,
    offset: u32,
    data_sql: &str,
    count_sql: &str,
    message: impl Into<String>,
) -> StatsTableResponse {
    StatsTableResponse {
        columns: Vec::new(),
        rows: Vec::new(),
        total_rows: 0,
        limit,
        offset,
        sql: labeled_sql(data_sql, count_sql),
        sql_error: Some(message.into()),
    }
}

fn format_sqlx_error(e: &sqlx::Error) -> String {
    match e {
        sqlx::Error::Database(db) => db.message().to_string(),
        _ => e.to_string(),
    }
}

pub async fn execute(
    pool: &PgPool,
    data_sql: &str,
    count_sql: &str,
    binds: &[serde_json::Value],
    limit: u32,
    offset: u32,
) -> Result<StatsTableResponse, AppError> {
    let work = async {
        let rows = match execute_raw(pool, data_sql, binds).await {
            Ok(r) => r,
            Err(e) => {
                return Ok(sql_failure_response(
                    limit,
                    offset,
                    data_sql,
                    count_sql,
                    format_sqlx_error(&e),
                ));
            }
        };

        let total_rows = match fetch_count_row(pool, count_sql, binds).await {
            Ok(row) => parse_count_total(&row)?,
            Err(e) => {
                return Ok(sql_failure_response(
                    limit,
                    offset,
                    data_sql,
                    count_sql,
                    format_sqlx_error(&e),
                ));
            }
        };

        let columns: Vec<ColumnMeta> = if let Some(first) = rows.first() {
            first
                .columns()
                .iter()
                .map(|col| ColumnMeta {
                    name: col.name().to_string(),
                    label: col.name().to_string(),
                    data_type: col.type_info().name().to_string(),
                })
                .collect()
        } else {
            Vec::new()
        };

        let json_rows: Vec<serde_json::Map<String, serde_json::Value>> =
            rows.iter().map(row_to_json).collect();

        Ok(StatsTableResponse {
            columns,
            rows: json_rows,
            total_rows,
            limit,
            offset,
            sql: labeled_sql(data_sql, count_sql),
            sql_error: None,
        })
    };

    match tokio::time::timeout(QUERY_TIMEOUT, work).await {
        Ok(inner) => inner,
        Err(_) => Ok(sql_failure_response(
            limit,
            offset,
            data_sql,
            count_sql,
            format!(
                "Stats query timed out after {}s",
                QUERY_TIMEOUT.as_secs()
            ),
        )),
    }
}

async fn execute_raw(
    pool: &PgPool,
    sql: &str,
    binds: &[serde_json::Value],
) -> Result<Vec<PgRow>, sqlx::Error> {
    let mut q = sqlx::query(sql);
    q = bind_values(q, binds);
    q.fetch_all(pool).await
}

async fn fetch_count_row(
    pool: &PgPool,
    sql: &str,
    binds: &[serde_json::Value],
) -> Result<PgRow, sqlx::Error> {
    let mut q = sqlx::query(sql);
    q = bind_values(q, binds);
    q.fetch_one(pool).await
}

fn parse_count_total(row: &PgRow) -> Result<u64, AppError> {
    let total: i64 = row
        .try_get("__total")
        .map_err(|e| AppError::Internal(format!("Reading total row count: {}", e)))?;
    Ok(total as u64)
}

fn bind_values<'q>(
    mut q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    binds: &'q [serde_json::Value],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    for bind in binds {
        q = match bind {
            serde_json::Value::String(s) => q.bind(s.as_str()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    q.bind(i)
                } else if let Some(f) = n.as_f64() {
                    q.bind(f)
                } else {
                    q.bind(n.to_string())
                }
            }
            serde_json::Value::Bool(b) => q.bind(*b),
            serde_json::Value::Null => q.bind(Option::<String>::None),
            _ => q.bind(bind.to_string()),
        };
    }
    q
}

fn row_to_json(row: &PgRow) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();

    for col in row.columns() {
        let name = col.name();
        let type_name = col.type_info().name();

        let val: serde_json::Value = match type_name {
            "INT2" => row
                .try_get::<i16, _>(name)
                .map(|v| serde_json::Value::from(v as i64))
                .unwrap_or(serde_json::Value::Null),
            "INT4" => row
                .try_get::<i32, _>(name)
                .map(|v| serde_json::Value::from(v as i64))
                .unwrap_or(serde_json::Value::Null),
            "INT8" => row
                .try_get::<i64, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
            "FLOAT4" => row
                .try_get::<f32, _>(name)
                .ok()
                .and_then(|v| serde_json::Number::from_f64(v as f64))
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            "FLOAT8" | "NUMERIC" => row
                .try_get::<f64, _>(name)
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            "BOOL" => row
                .try_get::<bool, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
            "TEXT" | "VARCHAR" | "CHAR" | "NAME" | "BPCHAR" => row
                .try_get::<String, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
            "DATE" => row
                .try_get::<chrono::NaiveDate, _>(name)
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            "TIME" => row
                .try_get::<chrono::NaiveTime, _>(name)
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            "TIMESTAMP" => row
                .try_get::<chrono::NaiveDateTime, _>(name)
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            "TIMESTAMPTZ" => row
                .try_get::<chrono::DateTime<chrono::Utc>, _>(name)
                .map(|v| serde_json::Value::String(v.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null),
            "JSON" | "JSONB" => row
                .try_get::<serde_json::Value, _>(name)
                .unwrap_or(serde_json::Value::Null),
            "UUID" => row
                .try_get::<uuid::Uuid, _>(name)
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            _ => row
                .try_get::<String, _>(name)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
        };

        map.insert(name.to_string(), val);
    }

    map
}
