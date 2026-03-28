//! Statistics endpoints

use axum::extract::Path;
use axum::routing::{get, post, put};
use axum::{extract::Query, extract::State, Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::AppResult,
    models::biblio::MediaType,
    models::stats_builder::{SavedStatsQuery, SavedStatsQueryWrite, StatsBuilderBody, StatsTableResponse},
    services::stats::{discovery_json, run_stats_query},
    repository::stats::saved_queries,
};

use super::{AuthenticatedUser, StaffUser};


/// Build the stats routes for this domain (staff/authenticated; no IP governor — see public API layer in `main.rs`).
pub fn router() -> axum::Router<crate::AppState> {
    Router::new()
        .route("/stats", get(get_stats))
        .route("/stats/loans", get(get_loan_stats))
        .route("/stats/users", get(get_user_stats))
        .route("/stats/catalog", get(get_catalog_stats))
        .route("/stats/schema", get(get_stats_schema))
        .route("/stats/query", post(post_stats_query))
        .route(
            "/stats/saved",
            get(list_saved_queries).post(create_saved_query),
        )
        .route(
            "/stats/saved/:id",
            put(update_saved_query).delete(delete_saved_query),
        )
        .route("/stats/saved/:id/run", get(run_saved_query))
}

/// Statistics response
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    /// Item statistics
    pub items: ItemStats,
    /// User statistics
    pub users: UserStats,
    /// Loan statistics
    pub loans: LoanStats,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemStats {
    /// Total number of items
    pub total: i64,
    /// Items by media type
    pub by_media_type: Vec<StatEntry>,
    /// Items by public type
    pub by_public_type: Vec<StatEntry>,
    /// Number of items acquired in the period (created_at in year)
    pub acquisitions: i64,
    /// Acquisitions by media type
    pub acquisitions_by_media_type: Vec<StatEntry>,
    /// Number of items withdrawn in the period (archived_at in year)
    pub withdrawals: i64,
    /// Withdrawals by media type
    pub withdrawals_by_media_type: Vec<StatEntry>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserStats {
    /// Total number of users
    pub total: i64,
    /// Users with active loans
    pub active: i64,
    /// Users by account type
    pub by_account_type: Vec<StatEntry>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanStats {
    /// Active loans
    pub active: i64,
    /// Overdue loans
    pub overdue: i64,
    /// Items returned today
    pub returned_today: i64,
    /// Loans by media type
    pub by_media_type: Vec<StatEntry>,
}

#[derive(Serialize, ToSchema)]
pub struct StatEntry {
    /// Label
    pub label: String,
    /// Value
    pub value: i64,
}

/// Sorting options for user loan statistics
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum UserStatsSortBy {
    /// Sort by total number of loans (active + historical)
    TotalLoans,
    /// Sort by number of active loans
    ActiveLoans,
    /// Sort by number of overdue loans
    OverdueLoans,
}

/// Mode for user statistics endpoint
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum UserStatsMode {
    /// Leaderboard-style response (list of users with their loan counts)
    Leaderboard,
    /// Aggregated response (totals for new users, active borrowers, etc.)
    Aggregate,
}

/// Query parameters for user loan statistics
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserStatsQuery {
    /// Field to sort by (total_loans, active_loans, overdue_loans) - only used in leaderboard mode
    #[serde(default)]
    pub sort_by: Option<UserStatsSortBy>,
    /// Maximum number of users to return (default: 50, max: 1000) - only used in leaderboard mode
    pub limit: Option<i64>,
    /// Start date (ISO 8601 format) for period-based statistics (E1 section)
    pub start_date: Option<String>,
    /// End date (ISO 8601 format) for period-based statistics (E1 section)
    pub end_date: Option<String>,
    /// Response mode: leaderboard (default) or aggregate
    #[serde(default)]
    pub mode: Option<UserStatsMode>,
}

/// User loan statistics entry
#[serde_as]
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserLoanStats {
    /// User ID
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    /// First name
    pub firstname: Option<String>,
    /// Last name
    pub lastname: Option<String>,
    /// Total number of loans (active + archived)
    pub total_loans: i64,
    /// Number of active loans
    pub active_loans: i64,
    /// Number of overdue loans
    pub overdue_loans: i64,
}

/// Query parameters for main library statistics (GET /stats)
#[derive(Debug, Default, Clone, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsQuery {
    /// Reference year (e.g. 2024) — stats computed as of 31 December of this year
    pub year: Option<i32>,
    /// Start of time interval (ISO 8601 date)
    pub start_date: Option<String>,
    /// End of time interval (ISO 8601 date); used as reference date when year is not set
    pub end_date: Option<String>,
    /// Filter by public type (e.g. "adult", "juvenile")
    pub public_type: Option<String>,
    /// Filter by media type (e.g. 'b', 'bc', 'p')
    pub media_type: Option<MediaType>,
}

/// Time interval for grouping statistics
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Interval {
    Day,
    Week,
    Month,
    Year,
}


/// Advanced loan statistics query parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanStatsQuery {
    /// Start date (ISO 8601 format)
    pub start_date: Option<String>,
    /// End date (ISO 8601 format)
    pub end_date: Option<String>,
    /// Grouping interval (day, week, month, year)
    pub interval: Option<Interval>,
    /// Filter by media type (e.g., 'b', 'bc', 'amc', 'vd')
    pub media_type: Option<MediaType>,
    /// Filter by audience / public type (e.g., "adult", "juvenile", "children")
    pub public_type: Option<String>,
    /// Filter by specific user ID (admin only)
    pub user_id: Option<i64>,
}

/// Loan statistics response with time series data
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanStatsResponse {
    /// Total number of loans in the period
    pub total_loans: i64,
    /// Total number of returns in the period
    pub total_returns: i64,
    /// Time series data grouped by interval
    pub time_series: Vec<TimeSeriesEntry>,
    /// Statistics by media type
    pub by_media_type: Vec<StatEntry>,
}

/// Aggregated user statistics for E1 section (new users, active borrowers)
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserStatsAggregate {
    /// Total number of users (all users, with or without loans)
    pub users_total: i64,
    /// Users broken down by public type (adult/children)
    pub users_by_public_type: Vec<StatEntry>,
    /// Users broken down by sex (male/female/unknown)
    pub users_by_sex: Vec<StatEntry>,
    /// Number of new users in the period
    pub new_users_total: i64,
    /// New users broken down by public type (adult/children)
    pub new_users_by_public_type: Vec<StatEntry>,
    /// New users broken down by sex (male/female/unknown)
    pub new_users_by_sex: Vec<StatEntry>,
    /// Number of active borrowers in the period
    pub active_borrowers_total: i64,
    /// Active borrowers broken down by public type
    pub active_borrowers_by_public_type: Vec<StatEntry>,
    /// Total number of group accounts (collectivites)
    pub groups_total: i64,
}

/// User statistics response, either leaderboard-style or aggregate
#[derive(Serialize, ToSchema)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum UserStatsResponse {
    /// Leaderboard-style statistics
    Leaderboard {
        /// Users with their loan statistics
        users: Vec<UserLoanStats>,
    },
    /// Aggregated statistics (no per-user breakdown)
    Aggregate(UserStatsAggregate),
}

/// Time series entry for loan statistics
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesEntry {
    /// Period label (e.g., "2024-01-15" for day, "2024-W03" for week)
    pub period: String,
    /// Number of loans in this period
    pub loans: i64,
    /// Number of returns in this period
    pub returns: i64,
}

/// Query parameters for catalog statistics (GET /stats/catalog)
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatsQuery {
    /// Start date (ISO 8601 format) for period-based statistics
    pub start_date: Option<String>,
    /// End date (ISO 8601 format) for period-based statistics
    pub end_date: Option<String>,
    /// Group results by source (default: false = aggregated)
    #[serde(default)]
    pub by_source: Option<bool>,
    /// Group results by media type
    #[serde(default)]
    pub by_media_type: Option<bool>,
    /// Group results by public type
    #[serde(default)]
    pub by_public_type: Option<bool>,
}

/// Catalog statistics response
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatsResponse {
    /// Aggregated totals
    pub totals: CatalogStatsTotals,
    /// Breakdown by source (only if by_source=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_source: Option<Vec<CatalogSourceStats>>,
    /// Breakdown by media type (only if by_media_type=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_media_type: Option<Vec<CatalogBreakdownStats>>,
    /// Breakdown by public type (only if by_public_type=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_public_type: Option<Vec<CatalogBreakdownStats>>,
}

/// Aggregated catalog statistics totals
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatsTotals {
    /// Number of active items/physical copies (not archived)
    pub active_items: i64,
    /// Number of items entered in the period
    pub entered_items: i64,
    /// Number of items archived in the period
    pub archived_items: i64,
    /// Number of loans in the period (0 if no period specified)
    pub loans: i64,
}

/// Catalog statistics per source
#[serde_as]
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSourceStats {
    /// Source ID
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub source_id: i64,
    /// Source name
    pub source_name: String,
    /// Number of active items/physical copies
    pub active_items: i64,
    /// Number of items entered in the period
    pub entered_items: i64,
    /// Number of items archived in the period
    pub archived_items: i64,
    /// Number of loans in the period
    pub loans: i64,
    /// Breakdown by media type (only when by_source=true AND by_media_type=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_media_type: Option<Vec<CatalogBreakdownStats>>,
    /// Breakdown by public type (only when by_source=true AND by_public_type=true, without by_media_type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_public_type: Option<Vec<CatalogBreakdownStats>>,
}

/// Catalog statistics breakdown (by media_type or public_type)
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogBreakdownStats {
    /// Label (media type code or public type name)
    pub label: String,
    /// Number of active items/physical copies
    pub active_items: i64,
    /// Number of items entered in the period
    pub entered_items: i64,
    /// Number of items archived in the period
    pub archived_items: i64,
    /// Number of loans in the period
    pub loans: i64,
    /// Nested breakdown by public type (only when by_public_type=true on a media_type entry)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_public_type: Option<Vec<CatalogBreakdownStats>>,
}


fn resolve_reference_date(query: &StatsQuery) -> Option<NaiveDate> {
    if let Some(ref s) = query.end_date {
        if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Some(d);
        }
    }
    if let Some(y) = query.year {
        NaiveDate::from_ymd_opt(y, 12, 31)
    } else {
        None
    }
}

/// Get library statistics
#[utoipa::path(
    get,
    path = "/stats",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(StatsQuery),
    responses(
        (status = 200, description = "Library statistics", body = StatsResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
    )
)]
pub async fn get_stats(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<StatsQuery>,
) -> AppResult<Json<StatsResponse>> {
    claims.require_read_items()?;

    let filter = if query.year.is_none()
        && query.start_date.is_none()
        && query.end_date.is_none()
        && query.public_type.is_none()
        && query.media_type.is_none()
    {
        None
    } else {
        Some(crate::services::stats::StatsFilter {
            reference_date: resolve_reference_date(&query),
            public_type: query.public_type,
            media_type: query.media_type.as_ref().map(MediaType::as_code).map(String::from),
        })
    };
    let stats = state.services.stats.get_stats(filter).await?;
    Ok(Json(stats))
}

/// Get advanced loan statistics.
///
/// **Scope narrowing:** non-admin callers who omit `user_id` will automatically
/// receive their own statistics only. To query global statistics, admin privileges
/// are required. Passing another user's `user_id` without admin rights returns 403.
#[utoipa::path(
    get,
    path = "/stats/loans",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(LoanStatsQuery),
    responses(
        (status = 200, description = "Loan statistics (scoped to caller when not admin)", body = LoanStatsResponse),
        (status = 403, description = "Insufficient permissions or querying another user without admin rights")
    )
)]
pub async fn get_loan_stats(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<LoanStatsQuery>,
) -> AppResult<Json<LoanStatsResponse>> {
    claims.require_read_loans()?;

    // Parse dates
    let start_date = query.start_date
        .as_ref()
        .map(|s| {
        // On essaie de parser comme un DateTime complet (RFC 3339)
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            // Sinon, on essaie de parser comme une date seule et on ajoute minuit UTC
            .or_else(|_| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Utc).unwrap())
            })
    })
    .transpose()
        .map_err(|_| crate::error::AppError::Validation("Invalid start_date format. Use ISO 8601 (RFC 3339)".to_string()))?
        .map(|dt| dt.with_timezone(&Utc));

    let end_date = query.end_date
        .as_ref()
        .map(|s| {
        // On essaie de parser comme un DateTime complet (RFC 3339)
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            // Sinon, on essaie de parser comme une date seule et on ajoute minuit UTC
            .or_else(|_| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Utc).unwrap())
            })
    })
    .transpose()
        .map_err(|_| crate::error::AppError::Validation("Invalid end_date format. Use ISO 8601 (RFC 3339)".to_string()))?
        .map(|dt| dt.with_timezone(&Utc));

    // Check if user can query other users' stats
    let user_id = if let Some(uid) = query.user_id {
        if uid != claims.user_id && !claims.is_admin() {
            return Err(crate::error::AppError::Authorization(
                "Only administrators can query statistics for other users".to_string()
            ));
        }
        Some(uid)
    } else {
        // If not admin and no user_id specified, default to own stats
        if !claims.is_admin() {
            Some(claims.user_id)
        } else {
            None
        }
    };

    let interval = query.interval.unwrap_or(Interval::Day);

    let stats = state.services.stats.get_loan_stats(
        start_date,
        end_date,
        interval,
        query.media_type.as_ref(),
        query.public_type.as_deref(),
        user_id,
    ).await?;

    Ok(Json(stats))
}

/// Get user loan statistics (leaderboard-style)
#[utoipa::path(
    get,
    path = "/stats/users",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(UserStatsQuery),
    responses(
        (status = 200, description = "User loan statistics (leaderboard or aggregate)", body = UserStatsResponse),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn get_user_stats(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<UserStatsQuery>,
) -> AppResult<Json<UserStatsResponse>> {
    // Reading this requires loan statistics access
    claims.require_read_loans()?;

    // Parse dates for aggregate mode
    let start_date = query
        .start_date
        .as_ref()
       .map(|s| {
        // On essaie de parser comme un DateTime complet (RFC 3339)
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            // Sinon, on essaie de parser comme une date seule et on ajoute minuit UTC
            .or_else(|_| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Utc).unwrap())
            })
    })
        .transpose()
        .map_err(|_| crate::error::AppError::Validation(
            "Invalid start_date format. Use ISO 8601 (RFC 3339)".to_string(),
        ))?
        .map(|dt| dt.with_timezone(&Utc));

    let end_date = query
        .end_date
        .as_ref()
       .map(|s| {
        // On essaie de parser comme un DateTime complet (RFC 3339)
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            // Sinon, on essaie de parser comme une date seule et on ajoute minuit UTC
            .or_else(|_| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Utc).unwrap())
            })
    })
        .transpose()
        .map_err(|_| crate::error::AppError::Validation(
            "Invalid end_date format. Use ISO 8601 (RFC 3339)".to_string(),
        ))?
        .map(|dt| dt.with_timezone(&Utc));

    let mode = query.mode.unwrap_or(UserStatsMode::Leaderboard);

    match mode {
        UserStatsMode::Leaderboard => {
            let sort_by = query.sort_by.unwrap_or(UserStatsSortBy::TotalLoans);

            // Apply sane defaults and bounds for limit
            let mut limit = query.limit.unwrap_or(50);
            if limit < 1 {
                limit = 1;
            }
            if limit > 1000 {
                limit = 1000;
            }

            let users = state
                .services
                .stats
                .get_user_stats(sort_by, limit)
                .await?;

            Ok(Json(UserStatsResponse::Leaderboard { users }))
        }
        UserStatsMode::Aggregate => {
            let aggregates = state
                .services
                .stats
                .get_user_aggregates(start_date, end_date)
                .await?;

            Ok(Json(UserStatsResponse::Aggregate(aggregates)))
        }
    }
}

/// Get catalog statistics (items/physical copies: active, entered, archived) with optional breakdowns.
///
/// ## Frontend display guide
///
/// The response always contains `totals` (global counts). The optional breakdown
/// fields are populated depending on the query flags:
///
/// | Flags requested                               | Response shape                                                         |
/// |-----------------------------------------------|------------------------------------------------------------------------|
/// | *(none)*                                      | `totals` only                                                          |
/// | `by_source`                                   | `by_source[]` — flat list of sources with counts                       |
/// | `by_media_type`                               | `by_media_type[]` — flat list of media types                           |
/// | `by_public_type`                              | `by_public_type[]` — flat list of public types                         |
/// | `by_source` + `by_media_type`                 | `by_source[].by_media_type[]` — each source contains its media detail  |
/// | `by_media_type` + `by_public_type`            | `by_media_type[].by_public_type[]` — each media contains public detail |
/// | `by_source` + `by_media_type` + `by_public_type` | 3-level nesting: `by_source[].by_media_type[].by_public_type[]`     |
///
/// **Rendering rules:**
/// - When `by_source` has nested `by_media_type`, render a table/accordion per source
///   with media type rows inside.
/// - When `by_media_type` entries contain `by_public_type`, add a sub-level
///   (e.g. expandable row or indented sub-rows) showing adult/children split.
/// - Top-level `by_media_type` and `by_public_type` are always global aggregations
///   (regardless of nesting inside `by_source`), useful for summary charts/pie.
/// - Each entry at every level carries `active_items`, `entered_items`,
///   `archived_items` — the parent's counts are the sum of its children.
#[utoipa::path(
    get,
    path = "/stats/catalog",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(CatalogStatsQuery),
    responses(
        (status = 200, description = "Catalog statistics", body = CatalogStatsResponse),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn get_catalog_stats(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<CatalogStatsQuery>,
) -> AppResult<Json<CatalogStatsResponse>> {
    claims.require_read_items()?;

    // Parse dates
    let start_date = query.start_date
        .as_ref()
        .map(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Utc).unwrap())
                })
        })
        .transpose()
        .map_err(|_| crate::error::AppError::Validation("Invalid start_date format. Use ISO 8601 (RFC 3339)".to_string()))?;

    let end_date = query.end_date
        .as_ref()
        .map(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .map(|date| date.and_hms_opt(23, 59, 59).unwrap().and_local_timezone(Utc).unwrap())
                })
        })
        .transpose()
        .map_err(|_| crate::error::AppError::Validation("Invalid end_date format. Use ISO 8601 (RFC 3339)".to_string()))?;

    let stats = state.services.stats.get_catalog_stats(
        start_date,
        end_date,
        query.by_source.unwrap_or(false),
        query.by_media_type.unwrap_or(false),
        query.by_public_type.unwrap_or(false),
    ).await?;

    Ok(Json(stats))
}

// --- Flexible stats builder (whitelist SQL) ---------------------------------

/// Discovery document for the visual query builder (`entities`, `operators`, …).
#[utoipa::path(
    get,
    path = "/stats/schema",
    tag = "stats",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Stats schema for builder UI"),
        (status = 403, description = "Staff only")
    )
)]
pub async fn get_stats_schema(
    _staff: StaffUser,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(discovery_json()))
}

/// Run a declarative stats query (tabular result, paginated).
#[utoipa::path(
    post,
    path = "/stats/query",
    tag = "stats",
    security(("bearer_auth" = [])),
    request_body = StatsBuilderBody,
    responses(
        (status = 200, description = "Tabular stats", body = StatsTableResponse),
        (status = 400, description = "Invalid query"),
        (status = 403, description = "Staff only")
    )
)]
pub async fn post_stats_query(
    State(state): State<crate::AppState>,
    _staff: StaffUser,
    Json(body): Json<StatsBuilderBody>,
) -> AppResult<Json<StatsTableResponse>> {
    let pool = state.services.repository_pool();
    let res = run_stats_query(pool, Some(&state.services.redis), &body).await?;
    Ok(Json(res))
}

/// List saved stats queries (own + shared; admins see all).
#[utoipa::path(
    get,
    path = "/stats/saved",
    tag = "stats",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Saved queries", body = [SavedStatsQuery]),
        (status = 403, description = "Staff only")
    )
)]
pub async fn list_saved_queries(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
) -> AppResult<Json<Vec<SavedStatsQuery>>> {
    let pool = state.services.repository_pool();
    let list = saved_queries::list_for_user(pool, claims.user_id, claims.is_admin()).await?;
    Ok(Json(list))
}

/// Save a stats query for reuse.
#[utoipa::path(
    post,
    path = "/stats/saved",
    tag = "stats",
    security(("bearer_auth" = [])),
    request_body = SavedStatsQueryWrite,
    responses(
        (status = 200, description = "Created saved query", body = SavedStatsQuery),
        (status = 403, description = "Staff only")
    )
)]
pub async fn create_saved_query(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Json(body): Json<SavedStatsQueryWrite>,
) -> AppResult<Json<SavedStatsQuery>> {
    let pool = state.services.repository_pool();
    let row = saved_queries::insert(pool, claims.user_id, &body).await?;
    Ok(Json(row))
}

/// Update a saved query (owner or admin).
#[utoipa::path(
    put,
    path = "/stats/saved/{id}",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Saved query id")
    ),
    request_body = SavedStatsQueryWrite,
    responses(
        (status = 200, description = "Updated", body = SavedStatsQuery),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found")
    )
)]
pub async fn update_saved_query(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
    Json(body): Json<SavedStatsQueryWrite>,
) -> AppResult<Json<SavedStatsQuery>> {
    let pool = state.services.repository_pool();
    let row = saved_queries::update(pool, id, claims.user_id, claims.is_admin(), &body).await?;
    Ok(Json(row))
}

/// Delete a saved query (owner or admin).
#[utoipa::path(
    delete,
    path = "/stats/saved/{id}",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Saved query id")
    ),
    responses(
        (status = 200, description = "Deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found")
    )
)]
pub async fn delete_saved_query(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let pool = state.services.repository_pool();
    saved_queries::delete_by_id(pool, id, claims.user_id, claims.is_admin()).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Execute a saved query by id (same body as `POST /stats/query` would use).
#[utoipa::path(
    get,
    path = "/stats/saved/{id}/run",
    tag = "stats",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Saved query id")
    ),
    responses(
        (status = 200, description = "Tabular stats", body = StatsTableResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found")
    )
)]
pub async fn run_saved_query(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<Json<StatsTableResponse>> {
    let pool = state.services.repository_pool();
    let saved = saved_queries::get_by_id(pool, id, claims.user_id, claims.is_admin())
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound("Saved query not found".into()))?;
    let res = run_stats_query(pool, Some(&state.services.redis), &saved.query).await?;
    Ok(Json(res))
}
