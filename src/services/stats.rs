//! Statistics service

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sqlx::Row;

use crate::{
    api::stats::{
        Interval, ItemStats, LoanStats, LoanStatsResponse,
        StatEntry, StatsResponse, TimeSeriesEntry, UserLoanStats, UserStats,
        UserStatsSortBy,
    },
    error::AppResult,
    models::biblio::MediaType,
    repository::Repository,
    services::Services,
};

/// Filter for GET /stats (optional year, time interval, public_type, media_type).
/// When set, item stats are computed as of reference_date and filtered by public_type/media_type.
#[derive(Debug)]
pub struct StatsFilter {
    /// Holdings as of this date (e.g. 31/12 for a given year).
    pub reference_date: Option<NaiveDate>,
    /// Restrict to this public type (e.g. "adult", "juvenile").
    pub public_type: Option<String>,
    /// Restrict to this media type (DB code string).
    pub media_type: Option<String>,
}

#[derive(Clone)]
pub struct StatsService {
    repository: Repository,
}

impl StatsService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    /// Build WHERE clause for item-based queries.
    /// Items (physical copies) are joined with biblios via `s` (items) and `i` (biblios) aliases.
    fn item_where_clause(filter: &Option<StatsFilter>) -> (String, Vec<String>) {
        let f = match filter {
            None => {
                return (
                    "s.archived_at IS NULL".to_string(),
                    vec![],
                );
            }
            Some(f) => f,
        };
        let mut conditions = Vec::new();
        let mut param_order = Vec::new();
        if f.reference_date.is_some() {
            conditions.push("(s.created_at <= $1 AND (s.archived_at IS NULL OR s.archived_at > $1))".to_string());
            param_order.push("ref_date".into());
        } else {
            conditions.push("s.archived_at IS NULL".to_string());
        }
        if f.public_type.is_some() {
            let i = param_order.len() + 1;
            param_order.push("public_type".into());
            conditions.push(format!("i.audience_type = ${}", i));
        }
        if f.media_type.is_some() {
            let i = param_order.len() + 1;
            param_order.push("media_type".into());
            conditions.push(format!("i.media_type = ${}", i));
        }
        (conditions.join(" AND "), param_order)
    }

    /// Get library statistics, optionally filtered by year, date range, public_type, media_type.
    /// Counts are based on active items/physical copies (joined with biblios for media_type/public_type).
    #[tracing::instrument(skip(self), err)]
    pub async fn get_stats(&self, filter: Option<StatsFilter>) -> AppResult<StatsResponse> {
        let pool = &self.repository.pool;
        let (spec_where, _param_order) = Self::item_where_clause(&filter);

        // Specimen stats (with optional filter)
        let total_items: i64 = {
            let q = format!(
                "SELECT COUNT(*) FROM items s JOIN biblios i ON s.biblio_id = i.id WHERE {}",
                spec_where
            );
            let mut query = sqlx::query_scalar::<_, i64>(&q);
            if let Some(ref f) = filter {
                if let Some(ref d) = f.reference_date {
                    query = query.bind(d);
                }
                if let Some(ref pt) = f.public_type {
                    query = query.bind(pt.as_str());
                }
                if let Some(ref mt) = f.media_type {
                    query = query.bind(mt.as_str());
                }
            }
            query.fetch_one(pool).await?
        };

        let items_by_media_type = {
            let q = format!(
                r#"SELECT COALESCE(i.media_type, 'unknown') as label, COUNT(*) as value
                   FROM items s JOIN biblios i ON s.biblio_id = i.id
                   WHERE {} GROUP BY i.media_type ORDER BY value DESC"#,
                spec_where
            );
            let mut query = sqlx::query(&q);
            if let Some(ref f) = filter {
                if let Some(ref d) = f.reference_date {
                    query = query.bind(d);
                }
                if let Some(ref pt) = f.public_type {
                    query = query.bind(pt.as_str());
                }
                if let Some(ref mt) = f.media_type {
                    query = query.bind(mt.as_str());
                }
            }
            query
                .fetch_all(pool)
                .await?
                .into_iter()
                .map(|row| StatEntry {
                    label: row.get("label"),
                    value: row.get("value"),
                })
                .collect()
        };

        let items_by_public_type = {
            let q = format!(
                r#"SELECT COALESCE(i.audience_type, 'unknown') as label,
                          COUNT(*) as value
                   FROM items s JOIN biblios i ON s.biblio_id = i.id
                   WHERE {} GROUP BY i.audience_type ORDER BY value DESC"#,
                spec_where
            );
            let mut query = sqlx::query(&q);
            if let Some(ref f) = filter {
                if let Some(ref d) = f.reference_date {
                    query = query.bind(d);
                }
                if let Some(ref pt) = f.public_type {
                    query = query.bind(pt.as_str());
                }
                if let Some(ref mt) = f.media_type {
                    query = query.bind(mt.as_str());
                }
            }
            query
                .fetch_all(pool)
                .await?
                .into_iter()
                .map(|row| StatEntry {
                    label: row.get("label"),
                    value: row.get("value"),
                })
                .collect()
        };

        // User stats (exclude deleted users: status != 2)
        let total_users: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE (status IS NULL OR status != 2)"
        )
            .fetch_one(pool)
            .await?;

        let active_users: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT user_id) FROM loans WHERE returned_at IS NULL"
        )
        .fetch_one(pool)
        .await?;

        let users_by_account_type = sqlx::query(
            r#"
            SELECT COALESCE(u.account_type, 'unknown') as label, COUNT(*) as value
            FROM users u
            WHERE (u.status IS NULL OR u.status != 2)
            GROUP BY u.account_type
            ORDER BY value DESC
            "#,
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry {
            label: row.get("label"),
            value: row.get("value"),
        })
        .collect();

        // Loan stats
        let active_loans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loans WHERE returned_at IS NULL")
            .fetch_one(pool)
            .await?;

        let overdue_loans: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans WHERE returned_at IS NULL AND issue_at < NOW()"
        )
        .fetch_one(pool)
        .await?;

        let returned_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loans_archives WHERE returned_at >= DATE_TRUNC('day', NOW())"
        )
        .fetch_one(pool)
        .await?;

        let loans_by_media_type = sqlx::query(
            r#"
            SELECT COALESCE(i.media_type, 'unknown') as label, COUNT(*) as value
            FROM loans l
            JOIN items s ON l.item_id = s.id
            JOIN biblios i ON s.biblio_id = i.id
            WHERE l.returned_at IS NULL
            GROUP BY i.media_type
            ORDER BY value DESC
            "#,
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry {
            label: row.get("label"),
            value: row.get("value"),
        })
        .collect();

        // Acquisitions and withdrawals (only when a reference date / year is set)
        // Based on items table joined with biblios for media_type/public_type filters.
        let (acquisitions, acquisitions_by_media_type, withdrawals, withdrawals_by_media_type) = if let Some(ref f) = filter {
            if let Some(ref_date) = f.reference_date {
                let year_start = chrono::NaiveDate::from_ymd_opt(ref_date.year(), 1, 1).unwrap();
                let year_end = ref_date;

                // Build optional media_type / public_type filter fragments (on items alias i)
                let mut extra_cond = String::new();
                let mut param_offset = 2_usize; // $1 and $2 are year_start and year_end
                if f.public_type.is_some() {
                    param_offset += 1;
                    extra_cond.push_str(&format!(" AND i.audience_type = ${}", param_offset));
                }
                if f.media_type.is_some() {
                    param_offset += 1;
                    extra_cond.push_str(&format!(" AND i.media_type = ${}", param_offset));
                }
                let _ = param_offset;

                // Acquisitions total
                let acq_q = format!(
                    "SELECT COUNT(*) FROM items s JOIN biblios i ON s.biblio_id = i.id WHERE s.created_at >= $1 AND s.created_at <= $2 AND s.archived_at IS NULL{}",
                    extra_cond
                );
                let mut acq_builder = sqlx::query_scalar::<_, i64>(&acq_q)
                    .bind(year_start)
                    .bind(year_end);
                if let Some(ref pt) = f.public_type { acq_builder = acq_builder.bind(pt.as_str()); }
                if let Some(ref mt) = f.media_type { acq_builder = acq_builder.bind(mt.as_str()); }
                let acq_total = acq_builder.fetch_one(pool).await?;

                // Acquisitions by media type
                let acq_mt_q = format!(
                    "SELECT COALESCE(i.media_type, 'unknown') as label, COUNT(*) as value FROM items s JOIN biblios i ON s.biblio_id = i.id WHERE s.created_at >= $1 AND s.created_at <= $2 AND s.archived_at IS NULL{} GROUP BY i.media_type ORDER BY value DESC",
                    extra_cond
                );
                let mut acq_mt_builder = sqlx::query(&acq_mt_q)
                    .bind(year_start)
                    .bind(year_end);
                if let Some(ref pt) = f.public_type { acq_mt_builder = acq_mt_builder.bind(pt.as_str()); }
                if let Some(ref mt) = f.media_type { acq_mt_builder = acq_mt_builder.bind(mt.as_str()); }
                let acq_by_mt: Vec<StatEntry> = acq_mt_builder.fetch_all(pool).await?
                    .into_iter().map(|row| StatEntry { label: row.get("label"), value: row.get("value") }).collect();

                // Withdrawals total
                let wd_q = format!(
                    "SELECT COUNT(*) FROM items s JOIN biblios i ON s.biblio_id = i.id WHERE s.archived_at >= $1 AND s.archived_at <= $2{}",
                    extra_cond
                );
                let mut wd_builder = sqlx::query_scalar::<_, i64>(&wd_q)
                    .bind(year_start)
                    .bind(year_end);
                if let Some(ref pt) = f.public_type { wd_builder = wd_builder.bind(pt.as_str()); }
                if let Some(ref mt) = f.media_type { wd_builder = wd_builder.bind(mt.as_str()); }
                let wd_total = wd_builder.fetch_one(pool).await?;

                // Withdrawals by media type
                let wd_mt_q = format!(
                    "SELECT COALESCE(i.media_type, 'unknown') as label, COUNT(*) as value FROM items s JOIN biblios i ON s.biblio_id = i.id WHERE s.archived_at >= $1 AND s.archived_at <= $2{} GROUP BY i.media_type ORDER BY value DESC",
                    extra_cond
                );
                let mut wd_mt_builder = sqlx::query(&wd_mt_q)
                    .bind(year_start)
                    .bind(year_end);
                if let Some(ref pt) = f.public_type { wd_mt_builder = wd_mt_builder.bind(pt.as_str()); }
                if let Some(ref mt) = f.media_type { wd_mt_builder = wd_mt_builder.bind(mt.as_str()); }
                let wd_by_mt: Vec<StatEntry> = wd_mt_builder.fetch_all(pool).await?
                    .into_iter().map(|row| StatEntry { label: row.get("label"), value: row.get("value") }).collect();

                (acq_total, acq_by_mt, wd_total, wd_by_mt)
            } else {
                (0, vec![], 0, vec![])
            }
        } else {
            (0, vec![], 0, vec![])
        };

        Ok(StatsResponse {
            items: ItemStats {
                total: total_items,
                by_media_type: items_by_media_type,
                by_public_type: items_by_public_type,
                acquisitions,
                acquisitions_by_media_type,
                withdrawals,
                withdrawals_by_media_type,
            },
            users: UserStats {
                total: total_users,
                active: active_users,
                by_account_type: users_by_account_type,
            },
            loans: LoanStats {
                active: active_loans,
                overdue: overdue_loans,
                returned_today,
                by_media_type: loans_by_media_type,
            },
        })
    }

    /// Get per-user loan statistics (total, active, overdue)
    #[tracing::instrument(skip(self), err)]
    pub async fn get_user_stats(
        &self,
        sort_by: UserStatsSortBy,
        limit: i64,
    ) -> AppResult<Vec<UserLoanStats>> {
        let pool = &self.repository.pool;

        let order_by = match sort_by {
            UserStatsSortBy::TotalLoans => "total_loans",
            UserStatsSortBy::ActiveLoans => "active_loans",
            UserStatsSortBy::OverdueLoans => "overdue_loans",
        };

        let query = format!(
            r#"
            SELECT 
                u.id as user_id,
                u.firstname,
                u.lastname,
                COALESCE(t.total_loans, 0) as total_loans,
                COALESCE(a.active_loans, 0) as active_loans,
                COALESCE(o.overdue_loans, 0) as overdue_loans
            FROM users u
            LEFT JOIN (
                SELECT user_id, COUNT(*) as total_loans
                FROM (
                    SELECT user_id FROM loans
                    UNION ALL
                    SELECT user_id FROM loans_archives
                ) l
                GROUP BY user_id
            ) t ON t.user_id = u.id
            LEFT JOIN (
                SELECT user_id, COUNT(*) as active_loans
                FROM loans
                WHERE returned_at IS NULL
                GROUP BY user_id
            ) a ON a.user_id = u.id
            LEFT JOIN (
                SELECT user_id, COUNT(*) as overdue_loans
                FROM loans
                WHERE returned_at IS NULL AND issue_at < NOW()
                GROUP BY user_id
            ) o ON o.user_id = u.id
            WHERE (u.status IS NULL OR u.status != 2)
            ORDER BY {order_by} DESC, u.id ASC
            LIMIT $1
            "#
        );

        let rows = sqlx::query(&query).bind(limit).fetch_all(pool).await?;

        let stats = rows
            .into_iter()
            .map(|row| UserLoanStats {
                user_id: row.get("user_id"),
                firstname: row.get("firstname"),
                lastname: row.get("lastname"),
                total_loans: row.get("total_loans"),
                active_loans: row.get("active_loans"),
                overdue_loans: row.get("overdue_loans"),
            })
            .collect();

        Ok(stats)
    }

    /// Get advanced loan statistics with time series
    #[tracing::instrument(skip(self), err)]
    pub async fn get_loan_stats(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        interval: Interval,
        media_type: Option<&MediaType>,
        public_type: Option<&str>,
        user_id: Option<i64>,
    ) -> AppResult<LoanStatsResponse> {
        let pool = &self.repository.pool;

        // Default date range: last 30 days if not specified
        let start = start_date.unwrap_or_else(|| Utc::now() - chrono::Duration::days(30));
        let end = end_date.unwrap_or_else(Utc::now);

        // Build date truncation expression based on interval
        let date_trunc = match interval {
            Interval::Day => "DATE_TRUNC('day', date)",
            Interval::Week => "DATE_TRUNC('week', date)",
            Interval::Month => "DATE_TRUNC('month', date)",
            Interval::Year => "DATE_TRUNC('year', date)",
        };

        let date_format = match interval {
            Interval::Day => "YYYY-MM-DD",
            Interval::Week => "IYYY-\"W\"IW",
            Interval::Month => "YYYY-MM",
            Interval::Year => "YYYY",
        };

        // Build WHERE clause
        let mut where_clauses = vec![
            format!("l.date >= '{}'", start.format("%Y-%m-%d %H:%M:%S")),
            format!("l.date <= '{}'", end.format("%Y-%m-%d %H:%M:%S")),
        ];

        if let Some(mt) = media_type {
            where_clauses.push(format!("i.media_type = '{}'", mt));
        }

        if let Some(pt) = public_type {
            where_clauses.push(format!("i.audience_type = '{}'", pt));
        }

        if let Some(uid) = user_id {
            where_clauses.push(format!("l.user_id = {}", uid));
        }

        let where_clause = where_clauses.join(" AND ");

        // Query for loans (from active loans table)
        let loans_query = format!(
            r#"
            SELECT 
                TO_CHAR({}, '{}') as period,
                COUNT(*) as count
            FROM loans l
            JOIN items s ON l.item_id = s.id
            JOIN biblios i ON s.biblio_id = i.id
            WHERE {}
            GROUP BY {}
            ORDER BY period
            "#,
            date_trunc, date_format, where_clause, date_trunc
        );

        let loans_data: Vec<(String, i64)> = sqlx::query(&loans_query)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| {
                let period: String = row.get("period");
                let count: i64 = row.get("count");
                (period, count)
            })
            .collect();

        // Query for loans from archives table (historical loans)
        let mut archived_loans_where = vec![
            format!("la.date >= '{}'", start.format("%Y-%m-%d %H:%M:%S")),
            format!("la.date <= '{}'", end.format("%Y-%m-%d %H:%M:%S")),
        ];

        if let Some(mt) = media_type {
            archived_loans_where.push(format!("i.media_type = '{}'", mt));
        }

        if let Some(pt) = public_type {
            archived_loans_where.push(format!("i.audience_type = '{}'", pt));
        }

        if let Some(uid) = user_id {
            archived_loans_where.push(format!("la.user_id = {}", uid));
        }

        let archived_loans_where_clause = archived_loans_where.join(" AND ");
        let archived_loans_date_trunc = date_trunc.replace("date", "la.date");

        let archived_loans_query = format!(
            r#"
            SELECT 
                TO_CHAR({}, '{}') as period,
                COUNT(*) as count
            FROM loans_archives la
            JOIN items s ON la.item_id = s.id
            JOIN biblios i ON s.biblio_id = i.id
            WHERE {}
            GROUP BY {}
            ORDER BY period
            "#,
            archived_loans_date_trunc, date_format, archived_loans_where_clause, archived_loans_date_trunc
        );

        let archived_loans_data: Vec<(String, i64)> = sqlx::query(&archived_loans_query)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| {
                let period: String = row.get("period");
                let count: i64 = row.get("count");
                (period, count)
            })
            .collect();

        // Query for returns (from loans_archives table)
        let mut returns_where = vec![
            format!("la.returned_at >= '{}'", start.format("%Y-%m-%d %H:%M:%S")),
            format!("la.returned_at <= '{}'", end.format("%Y-%m-%d %H:%M:%S")),
            "la.returned_at IS NOT NULL".to_string(),
        ];

        if let Some(mt) = media_type {
            returns_where.push(format!("i.media_type = '{}'", mt));
        }

        if let Some(pt) = public_type {
            returns_where.push(format!("i.audience_type = '{}'", pt));
        }

        if let Some(uid) = user_id {
            returns_where.push(format!("la.user_id = {}", uid));
        }

        let returns_where_clause = returns_where.join(" AND ");
        let returns_date_trunc = date_trunc.replace("date", "la.returned_at");

        let returns_query = format!(
            r#"
            SELECT 
                TO_CHAR({}, '{}') as period,
                COUNT(*) as count
            FROM loans_archives la
            JOIN items s ON la.item_id = s.id
            JOIN biblios i ON s.biblio_id = i.id
            WHERE {}
            GROUP BY {}
            ORDER BY period
            "#,
            returns_date_trunc, date_format, returns_where_clause, returns_date_trunc
        );

        let returns_data: Vec<(String, i64)> = sqlx::query(&returns_query)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| {
                let period: String = row.get("period");
                let count: i64 = row.get("count");
                (period, count)
            })
            .collect();

        // Combine loans and returns by period
        use std::collections::HashMap;
        let mut period_map: HashMap<String, (i64, i64)> = HashMap::new();

        for (period, count) in loans_data {
            period_map.entry(period).or_insert((0, 0)).0 += count;
        }

        for (period, count) in archived_loans_data {
            period_map.entry(period).or_insert((0, 0)).0 += count;
        }

        for (period, count) in returns_data {
            period_map.entry(period).or_insert((0, 0)).1 += count;
        }

        let mut time_series: Vec<TimeSeriesEntry> = period_map
            .into_iter()
            .map(|(period, (loans, returns))| TimeSeriesEntry {
                period,
                loans,
                returns,
            })
            .collect();

        time_series.sort_by_key(|e| e.period.clone());

        // Get total counts
        let total_loans: i64 = time_series.iter().map(|e| e.loans).sum();
        let total_returns: i64 = time_series.iter().map(|e| e.returns).sum();

        // Get statistics by media type
        let by_media_type_query = format!(
            r#"
            SELECT 
                COALESCE(i.media_type, 'unknown') as label,
                COUNT(*) as value
            FROM loans l
            JOIN items s ON l.item_id = s.id
            JOIN biblios i ON s.biblio_id = i.id
            WHERE {}
            GROUP BY i.media_type
            ORDER BY value DESC
            "#,
            where_clause
        );

        let by_media_type = sqlx::query(&by_media_type_query)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| StatEntry {
                label: row.get("label"),
                value: row.get("value"),
            })
            .collect();

        Ok(LoanStatsResponse {
            total_loans,
            total_returns,
            time_series,
            by_media_type,
        })
    }

    /// Get aggregated user statistics for a period (new users, active borrowers)
    #[tracing::instrument(skip(self), err)]
    pub async fn get_user_aggregates(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> AppResult<crate::api::stats::UserStatsAggregate> {
        let pool = &self.repository.pool;

        // Default to the last 365 days if no explicit range is provided
        let default_end = Utc::now();
        let default_start = DateTime::from_timestamp(0, 0).unwrap();

        let start = start_date.unwrap_or(default_start);
        let end = end_date.unwrap_or(default_end);

        // Total users: all users not deleted
        let users_total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM users
            WHERE (status IS NULL OR status != 2)
            "#,
        )
        .fetch_one(pool)
        .await?;

        // Users by public type (JOIN public_types for label)
        let users_by_public_type: Vec<StatEntry> = sqlx::query(
            r#"
            SELECT COALESCE(pt.name, 'unknown') as label, COUNT(*) as value
            FROM users u
            LEFT JOIN public_types pt ON u.public_type = pt.id
            WHERE (u.status IS NULL OR u.status != 2)
            GROUP BY pt.name ORDER BY value DESC
            "#,
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry { label: row.get("label"), value: row.get("value") })
        .collect();

        // Users by sex
        let users_by_sex: Vec<StatEntry> = sqlx::query(
            r#"
            SELECT CASE sex WHEN 70 THEN 'female' WHEN 77 THEN 'male' ELSE 'unknown' END as label,
                   COUNT(*) as value
            FROM users
            WHERE (status IS NULL OR status != 2)
            GROUP BY sex ORDER BY value DESC
            "#,
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry { label: row.get("label"), value: row.get("value") })
        .collect();

        // New users: created in the period and not deleted
        let new_users_total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM users
            WHERE (status IS NULL OR status != 2)
              AND created_at IS NOT NULL
              AND created_at >= $1
              AND created_at <= $2
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await?;

        // New users by public type (JOIN public_types for label)
        let new_users_by_public_type: Vec<StatEntry> = sqlx::query(
            r#"
            SELECT COALESCE(pt.name, 'unknown') as label, COUNT(*) as value
            FROM users u
            LEFT JOIN public_types pt ON u.public_type = pt.id
            WHERE (u.status IS NULL OR u.status != 2)
              AND u.created_at IS NOT NULL AND u.created_at >= $1 AND u.created_at <= $2
            GROUP BY pt.name ORDER BY value DESC
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry { label: row.get("label"), value: row.get("value") })
        .collect();

        // New users by sex
        let new_users_by_sex: Vec<StatEntry> = sqlx::query(
            r#"
            SELECT CASE sex WHEN 70 THEN 'female' WHEN 77 THEN 'male' ELSE 'unknown' END as label,
                   COUNT(*) as value
            FROM users
            WHERE (status IS NULL OR status != 2)
              AND created_at IS NOT NULL AND created_at >= $1 AND created_at <= $2
            GROUP BY sex ORDER BY value DESC
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry { label: row.get("label"), value: row.get("value") })
        .collect();

        // Active borrowers: at least one loan (active or archived) in the period
        let active_borrowers_total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT u.id)
            FROM users u
            WHERE (u.status IS NULL OR u.status != 2)
              AND EXISTS (
                SELECT 1
                FROM (
                  SELECT user_id, date
                  FROM loans
                  UNION ALL
                  SELECT user_id, date
                  FROM loans_archives
                ) l
                WHERE l.user_id = u.id
                  AND l.date >= $1
                  AND l.date <= $2
              )
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await?;

        // Active borrowers by public type (JOIN public_types for label)
        let active_borrowers_by_public_type: Vec<StatEntry> = sqlx::query(
            r#"
            SELECT COALESCE(pt.name, 'unknown') as label, COUNT(DISTINCT u.id) as value
            FROM users u
            LEFT JOIN public_types pt ON u.public_type = pt.id
            WHERE (u.status IS NULL OR u.status != 2)
              AND EXISTS (
                SELECT 1
                FROM (
                  SELECT user_id, date FROM loans
                  UNION ALL
                  SELECT user_id, date FROM loans_archives
                ) l
                WHERE l.user_id = u.id AND l.date >= $1 AND l.date <= $2
              )
            GROUP BY pt.name ORDER BY value DESC
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| StatEntry { label: row.get("label"), value: row.get("value") })
        .collect();

        // Groups total (collectivites with active registration up to end date)
        let groups_total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM users
            WHERE account_type = 'group'
              AND (status IS NULL OR status != 2)
              AND (created_at IS NULL OR created_at <= $1)
            "#,
        )
        .bind(end)
        .fetch_one(pool)
        .await?;

        Ok(crate::api::stats::UserStatsAggregate {
            users_total,
            users_by_public_type,
            users_by_sex,
            new_users_total,
            new_users_by_public_type,
            new_users_by_sex,
            active_borrowers_total,
            active_borrowers_by_public_type,
            groups_total,
        })
    }

    /// Get catalog statistics: active items, entered items, archived items
    /// with optional breakdowns by source, media_type, public_type.
    #[tracing::instrument(skip(self), err)]
    pub async fn get_catalog_stats(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        by_source: bool,
        by_media_type: bool,
        by_public_type: bool,
    ) -> AppResult<crate::api::stats::CatalogStatsResponse> {
        let pool = &self.repository.pool;

        let start = start_date.unwrap_or(chrono::DateTime::from_timestamp(0, 0).unwrap());
        let end = end_date.unwrap_or(Utc::now());

        // --- Totals ---
        
        let active_items: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM items WHERE created_at < $1 AND (archived_at IS NULL OR archived_at > $2) "
        )
        .bind(end)
        .bind(end_date.unwrap_or(start))
        .fetch_one(pool)
        .await?;

        // Entered specimens in period
        let entered_items: i64 = 
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM items WHERE created_at >= $1 AND created_at <= $2"
            )
            .bind(start)
            .bind(end)
            .fetch_one(pool)
            .await?;

        // Archived specimens in period
        let archived_items: i64 = 
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM items WHERE archived_at >= $1 AND archived_at <= $2"
            )
            .bind(start)
            .bind(end)
            .fetch_one(pool)
            .await?;

        // Loans in period (active + archived via specimens)
        let total_loans: i64 = 
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM (
                    SELECT id, date FROM loans
                    UNION ALL
                    SELECT id, date FROM loans_archives
                ) all_loans WHERE date >= $1 AND date <= $2"#
            )
            .bind(start)
            .bind(end)
            .fetch_one(pool)
            .await?;

        let totals = crate::api::stats::CatalogStatsTotals {
            active_items,
            entered_items,
            archived_items,
            loans: total_loans,
        };

        // --- By source (with optional nested media_type / public_type breakdowns) ---
        // When multiple flags are active, only the nested result is returned.
        // Hierarchy: source → media_type → public_type
        use std::collections::HashMap;
        let mut by_source_data = if by_source {
            if by_media_type && by_public_type {
                // 3-level nesting: source → media_type → public_type
                let rows = 
                    sqlx::query(
                        r#"
                        SELECT
                            COALESCE(src.id, 0) as source_id,
                            COALESCE(src.name, 'unknown') as source_name,
                            COALESCE(i.media_type, 'unknown') as media_type_label,
                            COALESCE(i.audience_type, 'unknown') as public_type_label,
                            COUNT(*) FILTER (WHERE (sp.archived_at IS NULL OR sp.archived_at > $3)) as active_items,
                            COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                            COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                        FROM items sp
                        LEFT JOIN sources src ON sp.source_id = src.id
                        JOIN items i ON sp.item_id = i.id
                        GROUP BY src.id, src.name, i.media_type, i.audience_type
                        "#
                    )
                    .bind(start)
                    .bind(end)
                    .bind(end_date.unwrap_or(start))

                    .fetch_all(pool)
                    .await?;

                let mut source_map: HashMap<i64, (String, HashMap<String, HashMap<String, (i64, i64, i64)>>)> = HashMap::new();
                for row in &rows {
                    let sid: i64 = row.get("source_id");
                    let sname: String = row.get("source_name");
                    let mt: String = row.get("media_type_label");
                    let pt: String = row.get("public_type_label");
                    let a: i64 = row.get("active_items");
                    let e: i64 = row.get("entered_items");
                    let ar: i64 = row.get("archived_items");
                    let source_entry = source_map.entry(sid).or_insert_with(|| (sname, HashMap::new()));
                    let mt_entry = source_entry.1.entry(mt).or_default();
                    let pt_entry = mt_entry.entry(pt).or_insert((0, 0, 0));
                    pt_entry.0 += a;
                    pt_entry.1 += e;
                    pt_entry.2 += ar;
                }

                let mut result: Vec<crate::api::stats::CatalogSourceStats> = source_map.into_iter().map(|(source_id, (source_name, media_map))| {
                    let mut by_mt: Vec<crate::api::stats::CatalogBreakdownStats> = media_map.into_iter().map(|(label, pt_map)| {
                        let mut by_pt: Vec<crate::api::stats::CatalogBreakdownStats> = pt_map.into_iter().map(|(pt_label, (a, e, ar))| {
                            crate::api::stats::CatalogBreakdownStats { label: pt_label, active_items: a, entered_items: e, archived_items: ar, loans: 0, by_public_type: None }
                        }).collect();
                        by_pt.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                        let (active, entered, archived) = by_pt.iter().fold((0i64, 0i64, 0i64), |acc, x| (acc.0 + x.active_items, acc.1 + x.entered_items, acc.2 + x.archived_items));
                        crate::api::stats::CatalogBreakdownStats { label, active_items: active, entered_items: entered, archived_items: archived, loans: 0, by_public_type: Some(by_pt) }
                    }).collect();
                    by_mt.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                    let (active, entered, archived) = by_mt.iter().fold((0i64, 0i64, 0i64), |acc, x| (acc.0 + x.active_items, acc.1 + x.entered_items, acc.2 + x.archived_items));
                    crate::api::stats::CatalogSourceStats { source_id, source_name, active_items: active, entered_items: entered, archived_items: archived, loans: 0, by_media_type: Some(by_mt), by_public_type: None }
                }).collect();
                result.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                Some(result)

            } else if by_media_type {
                // 2-level nesting: source → media_type
                let rows = 
                    sqlx::query(
                        r#"
                        SELECT
                            COALESCE(src.id, 0) as source_id,
                            COALESCE(src.name, 'unknown') as source_name,
                            COALESCE(i.media_type, 'unknown') as media_type_label,
                            COUNT(*) FILTER (WHERE sp.archived_at IS NULL) as active_items,
                            COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                            COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                        FROM items sp
                        LEFT JOIN sources src ON sp.source_id = src.id
                        JOIN items i ON sp.item_id = i.id
                        GROUP BY src.id, src.name, i.media_type
                        "#
                    )
                    .bind(start)
                    .bind(end)
                    .fetch_all(pool)
                    .await?;

                let mut source_map: HashMap<i64, (String, HashMap<String, (i64, i64, i64)>)> = HashMap::new();
                for row in &rows {
                    let sid: i64 = row.get("source_id");
                    let sname: String = row.get("source_name");
                    let mt: String = row.get("media_type_label");
                    let a: i64 = row.get("active_items");
                    let e: i64 = row.get("entered_items");
                    let ar: i64 = row.get("archived_items");
                    let source_entry = source_map.entry(sid).or_insert_with(|| (sname, HashMap::new()));
                    let mt_entry = source_entry.1.entry(mt).or_insert((0, 0, 0));
                    mt_entry.0 += a;
                    mt_entry.1 += e;
                    mt_entry.2 += ar;
                }

                let mut result: Vec<crate::api::stats::CatalogSourceStats> = source_map.into_iter().map(|(source_id, (source_name, media_map))| {
                    let mut by_mt: Vec<crate::api::stats::CatalogBreakdownStats> = media_map.into_iter().map(|(label, (a, e, ar))| {
                        crate::api::stats::CatalogBreakdownStats { label, active_items: a, entered_items: e, archived_items: ar, loans: 0, by_public_type: None }
                    }).collect();
                    by_mt.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                    let (active, entered, archived) = by_mt.iter().fold((0i64, 0i64, 0i64), |acc, x| (acc.0 + x.active_items, acc.1 + x.entered_items, acc.2 + x.archived_items));
                    crate::api::stats::CatalogSourceStats { source_id, source_name, active_items: active, entered_items: entered, archived_items: archived, loans: 0, by_media_type: Some(by_mt), by_public_type: None }
                }).collect();
                result.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                Some(result)

            } else if by_public_type {
                // 2-level nesting: source → public_type
                let rows = 
                    sqlx::query(
                        r#"
                        SELECT
                            COALESCE(src.id, 0) as source_id,
                            COALESCE(src.name, 'unknown') as source_name,
                            COALESCE(i.audience_type, 'unknown') as public_type_label,
                            COUNT(*) FILTER (WHERE sp.archived_at IS NULL) as active_items,
                            COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                            COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                        FROM items sp
                        LEFT JOIN sources src ON sp.source_id = src.id
                        JOIN items i ON sp.item_id = i.id
                        GROUP BY src.id, src.name, i.audience_type
                        "#
                    )
                    .bind(start)
                    .bind(end)
                    .fetch_all(pool)
                    .await?;
               

                let mut source_map: HashMap<i64, (String, HashMap<String, (i64, i64, i64)>)> = HashMap::new();
                for row in &rows {
                    let sid: i64 = row.get("source_id");
                    let sname: String = row.get("source_name");
                    let pt: String = row.get("public_type_label");
                    let a: i64 = row.get("active_items");
                    let e: i64 = row.get("entered_items");
                    let ar: i64 = row.get("archived_items");
                    let source_entry = source_map.entry(sid).or_insert_with(|| (sname, HashMap::new()));
                    let pt_entry = source_entry.1.entry(pt).or_insert((0, 0, 0));
                    pt_entry.0 += a;
                    pt_entry.1 += e;
                    pt_entry.2 += ar;
                }

                let mut result: Vec<crate::api::stats::CatalogSourceStats> = source_map.into_iter().map(|(source_id, (source_name, pt_map))| {
                    let mut by_pt: Vec<crate::api::stats::CatalogBreakdownStats> = pt_map.into_iter().map(|(label, (a, e, ar))| {
                        crate::api::stats::CatalogBreakdownStats { label, active_items: a, entered_items: e, archived_items: ar, loans: 0, by_public_type: None }
                    }).collect();
                    by_pt.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                    let (active, entered, archived) = by_pt.iter().fold((0i64, 0i64, 0i64), |acc, x| (acc.0 + x.active_items, acc.1 + x.entered_items, acc.2 + x.archived_items));
                    crate::api::stats::CatalogSourceStats { source_id, source_name, active_items: active, entered_items: entered, archived_items: archived, loans: 0, by_media_type: None, by_public_type: Some(by_pt) }
                }).collect();
                result.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                Some(result)

            } else {
                // Flat source only
                let rows = 
                    sqlx::query(
                        r#"
                        SELECT
                            COALESCE(src.id, 0) as source_id,
                            COALESCE(src.name, 'unknown') as source_name,
                            COUNT(*) FILTER (WHERE sp.archived_at IS NULL) as active_items,
                            COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                            COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                        FROM items sp
                        LEFT JOIN sources src ON sp.source_id = src.id
                        GROUP BY src.id, src.name
                        ORDER BY active_items DESC
                        "#
                    )
                    .bind(start)
                    .bind(end)
                    .fetch_all(pool)
                    .await?;
                
                Some(rows.into_iter().map(|row| crate::api::stats::CatalogSourceStats {
                    source_id: row.get("source_id"),
                    source_name: row.get("source_name"),
                    active_items: row.get("active_items"),
                    entered_items: row.get("entered_items"),
                    archived_items: row.get("archived_items"),
                    loans: 0,
                    by_media_type: None,
                    by_public_type: None,
                }).collect())
            }
        } else {
            None
        };

        // --- Top-level by media type ---
        // Only when by_source is off (otherwise media is nested inside source)
        let mut by_media_type_data = if by_media_type && !by_source {
            if by_public_type {
                // 2-level nesting: media_type → public_type
                let rows = 
                    sqlx::query(
                        r#"
                        SELECT
                            COALESCE(i.media_type, 'unknown') as label,
                            COALESCE(i.audience_type, 'unknown') as public_type_label,
                            COUNT(*) FILTER (WHERE sp.archived_at IS NULL) as active_items,
                            COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                            COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                        FROM items sp
                        JOIN items i ON sp.item_id = i.id
                        GROUP BY i.media_type, i.audience_type
                        "#
                    )
                    .bind(start)
                    .bind(end)
                    .fetch_all(pool)
                    .await?;
              

                let mut media_map: HashMap<String, HashMap<String, (i64, i64, i64)>> = HashMap::new();
                for row in &rows {
                    let mt: String = row.get("label");
                    let pt: String = row.get("public_type_label");
                    let a: i64 = row.get("active_items");
                    let e: i64 = row.get("entered_items");
                    let ar: i64 = row.get("archived_items");
                    let mt_entry = media_map.entry(mt).or_default();
                    let pt_entry = mt_entry.entry(pt).or_insert((0, 0, 0));
                    pt_entry.0 += a;
                    pt_entry.1 += e;
                    pt_entry.2 += ar;
                }

                let mut result: Vec<crate::api::stats::CatalogBreakdownStats> = media_map.into_iter().map(|(label, pt_map)| {
                    let mut by_pt: Vec<crate::api::stats::CatalogBreakdownStats> = pt_map.into_iter().map(|(pt_label, (a, e, ar))| {
                        crate::api::stats::CatalogBreakdownStats { label: pt_label, active_items: a, entered_items: e, archived_items: ar, loans: 0, by_public_type: None }
                    }).collect();
                    by_pt.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                    let (active, entered, archived) = by_pt.iter().fold((0i64, 0i64, 0i64), |acc, x| (acc.0 + x.active_items, acc.1 + x.entered_items, acc.2 + x.archived_items));
                    crate::api::stats::CatalogBreakdownStats { label, active_items: active, entered_items: entered, archived_items: archived, loans: 0, by_public_type: Some(by_pt) }
                }).collect();
                result.sort_by(|a, b| b.active_items.cmp(&a.active_items));
                Some(result)
            } else {
                // Flat media type breakdown
                let rows = 
                    sqlx::query(
                        r#"
                        SELECT
                            COALESCE(i.media_type, 'unknown') as label,
                            COUNT(*) FILTER (WHERE sp.archived_at IS NULL) as active_items,
                            COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                            COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                        FROM items sp
                        JOIN items i ON sp.item_id = i.id
                        GROUP BY i.media_type
                        ORDER BY active_items DESC
                        "#
                    )
                    .bind(start)
                    .bind(end)
                    .fetch_all(pool)
                    .await?;
               
                Some(rows.into_iter().map(|row| crate::api::stats::CatalogBreakdownStats {
                    label: row.get("label"),
                    active_items: row.get("active_items"),
                    entered_items: row.get("entered_items"),
                    archived_items: row.get("archived_items"),
                    loans: 0,
                    by_public_type: None,
                }).collect())
            }   
        } else {
            None
        };

        // --- Top-level by public type ---
        // Only when neither by_source nor by_media_type is on (otherwise public is nested)
        let mut by_public_type_data = if by_public_type && !by_source && !by_media_type {
            let rows = 
                sqlx::query(
                    r#"
                    SELECT
                        COALESCE(i.audience_type, 'unknown') as label,
                        COUNT(*) FILTER (WHERE sp.archived_at IS NULL) as active_items,
                        COUNT(*) FILTER (WHERE sp.created_at >= $1 AND sp.created_at <= $2) as entered_items,
                        COUNT(*) FILTER (WHERE sp.archived_at >= $1 AND sp.archived_at <= $2) as archived_items
                    FROM items sp
                    JOIN items i ON sp.item_id = i.id
                    GROUP BY i.audience_type
                    ORDER BY active_items DESC
                    "#
                )
                .bind(start)
                .bind(end)
                .fetch_all(pool)
                .await?;
               
            Some(rows.into_iter().map(|row| crate::api::stats::CatalogBreakdownStats {
                label: row.get("label"),
                active_items: row.get("active_items"),
                entered_items: row.get("entered_items"),
                archived_items: row.get("archived_items"),
                loans: 0,
                by_public_type: None,
            }).collect::<Vec<_>>())
        } else {
            None
        };

        // --- Merge loan counts into breakdown structures ---
        // All loans (active + archived) grouped by (source_id, media_type, public_type)
        // via specimens for both tables.
        
            let loan_rows = sqlx::query(
                r#"
                SELECT
                    COALESCE(sp.source_id, 0) as source_id,
                    COALESCE(i.media_type, 'unknown') as media_type,
                    COALESCE(i.audience_type, 'unknown') as public_type,
                    COUNT(*) as loans
                FROM (
                    SELECT specimen_id, date FROM loans
                    UNION ALL
                    SELECT specimen_id, date FROM loans_archives
                ) all_loans
                JOIN specimens sp ON all_loans.specimen_id = sp.id
                JOIN items i ON sp.item_id = i.id
                WHERE all_loans.date >= $1 AND all_loans.date <= $2
                GROUP BY sp.source_id, i.media_type, i.audience_type
                "#
            )
            .bind(start)
            .bind(end)
            .fetch_all(pool)
            .await?;

            // source_id → media_type → public_type → count
            let mut loan_map: HashMap<i64, HashMap<String, HashMap<String, i64>>> = HashMap::new();
            for row in &loan_rows {
                let sid: i64 = row.get("source_id");
                let mt: String = row.get("media_type");
                let pt: String = row.get("public_type");
                let cnt: i64 = row.get("loans");
                *loan_map.entry(sid).or_default().entry(mt).or_default().entry(pt).or_insert(0) += cnt;
            }

            // Merge into by_source
            if let Some(ref mut sources) = by_source_data {
                for source in sources.iter_mut() {
                    let sid = source.source_id;
                    source.loans = loan_map.get(&sid)
                        .map(|mm| mm.values().flat_map(|pm| pm.values()).sum::<i64>())
                        .unwrap_or(0);

                    if let Some(ref mut medias) = source.by_media_type {
                        for media in medias.iter_mut() {
                            media.loans = loan_map.get(&sid)
                                .and_then(|mm| mm.get(&media.label))
                                .map(|pm| pm.values().sum::<i64>())
                                .unwrap_or(0);
                            if let Some(ref mut publics) = media.by_public_type {
                                for public in publics.iter_mut() {
                                    public.loans = loan_map.get(&sid)
                                        .and_then(|mm| mm.get(&media.label))
                                        .and_then(|pm| pm.get(&public.label))
                                        .copied()
                                        .unwrap_or(0);
                                }
                            }
                        }
                    }

                    if let Some(ref mut publics) = source.by_public_type {
                        for public in publics.iter_mut() {
                            public.loans = loan_map.get(&sid)
                                .map(|mm| mm.values().filter_map(|pm| pm.get(&public.label)).sum::<i64>())
                                .unwrap_or(0);
                        }
                    }
                }
            }

            // Merge into top-level by_media_type (sum across all sources)
            if let Some(ref mut medias) = by_media_type_data {
                for media in medias.iter_mut() {
                    media.loans = loan_map.values()
                        .filter_map(|mm| mm.get(&media.label))
                        .flat_map(|pm| pm.values())
                        .sum();

                    if let Some(ref mut publics) = media.by_public_type {
                        for public in publics.iter_mut() {
                            public.loans = loan_map.values()
                                .filter_map(|mm| mm.get(&media.label))
                                .filter_map(|pm| pm.get(&public.label))
                                .sum();
                        }
                    }
                }
            }

            // Merge into top-level by_public_type (sum across all sources + media)
            if let Some(ref mut publics) = by_public_type_data {
                for public in publics.iter_mut() {
                    public.loans = loan_map.values()
                        .flat_map(|mm| mm.values())
                        .filter_map(|pm| pm.get(&public.label))
                        .sum();
                }
            }
        

        Ok(crate::api::stats::CatalogStatsResponse {
            totals,
            by_source: by_source_data,
            by_media_type: by_media_type_data,
            by_public_type: by_public_type_data,
        })
    }
}


