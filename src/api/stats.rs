//! Statistics endpoints

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{extract::Query, extract::State, Json, Router};
use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    error::AppResult,
    models::biblio::MediaType,
    models::stats_builder::{SavedStatsQuery, SavedStatsQueryWrite, StatsBuilderBody},
    services::stats::discovery_json,
};

use super::{AuthenticatedUser, StaffUser};

pub use crate::models::dto::stats::{
    CatalogBreakdownStats, CatalogSourceStats, CatalogStatsQuery, CatalogStatsResponse,
    CatalogStatsTotals, Interval, ItemStats, LoanStats, LoanStatsQuery, LoanStatsResponse,
    StatEntry, StatsQuery, StatsResponse, TimeSeriesEntry, UserLoanStats, UserStats,
    UserStatsAggregate, UserStatsMode, UserStatsQuery, UserStatsResponse, UserStatsSortBy,
};


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
        (status = 200, description = "Tabular stats", body = crate::models::stats_builder::StatsTableResponse),
        (status = 422, description = "PostgreSQL rejected the generated SQL", body = crate::models::stats_builder::StatsTableResponse),
        (status = 400, description = "Invalid query"),
        (status = 403, description = "Staff only")
    )
)]
pub async fn post_stats_query(
    State(state): State<crate::AppState>,
    _staff: StaffUser,
    Json(body): Json<StatsBuilderBody>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let res = state.services.stats.run_query(&body).await?;
    let status = if res.sql_error.is_some() {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::OK
    };
    Ok((status, Json(res)))
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
    let list = state
        .services
        .stats
        .list_saved_queries(claims.user_id, claims.is_admin())
        .await?;
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
    let row = state
        .services
        .stats
        .create_saved_query(claims.user_id, &body)
        .await?;
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
    let row = state
        .services
        .stats
        .update_saved_query(id, claims.user_id, claims.is_admin(), &body)
        .await?;
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
    state
        .services
        .stats
        .delete_saved_query(id, claims.user_id, claims.is_admin())
        .await?;
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
        (status = 200, description = "Tabular stats", body = crate::models::stats_builder::StatsTableResponse),
        (status = 422, description = "PostgreSQL rejected the generated SQL", body = crate::models::stats_builder::StatsTableResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found")
    )
)]
pub async fn run_saved_query(
    State(state): State<crate::AppState>,
    StaffUser(claims): StaffUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let res = state
        .services
        .stats
        .run_saved_query(id, claims.user_id, claims.is_admin())
        .await?;
    let status = if res.sql_error.is_some() {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::OK
    };
    Ok((status, Json(res)))
}
