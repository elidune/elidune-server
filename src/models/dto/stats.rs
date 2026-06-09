//! Stats DTOs shared by API, services, and repository layers.

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::{IntoParams, ToSchema};

use crate::models::biblio::MediaType;

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
