//! Visitor count model

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};

/// Visitor count record
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct VisitorCount {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    /// Date of the count
    pub count_date: NaiveDate,
    /// Number of visitors
    pub count: i32,
    /// Source of the count (manual, counter, estimate)
    pub source: Option<String>,
    pub notes: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Create visitor count request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateVisitorCount {
    /// Date of the count (YYYY-MM-DD)
    pub count_date: String,
    /// Number of visitors
    pub count: i32,
    /// Source: manual, counter, estimate
    pub source: Option<String>,
    pub notes: Option<String>,
}

/// Query parameters for visitor counts
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct VisitorCountQuery {
    /// Start date (YYYY-MM-DD)
    pub start_date: Option<String>,
    /// End date (YYYY-MM-DD)
    pub end_date: Option<String>,
}
