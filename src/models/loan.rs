//! Loan (borrow) model and related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

use super::item::{ItemShort, MediaType};
use super::user::UserShort;

/// Loan model from database
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Loan {
    #[serde_as(as = "DisplayFromStr")]
    pub id: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub specimen_id: i64,
    pub date: DateTime<Utc>,
    pub renew_at: Option<DateTime<Utc>>,
    pub nb_renews: Option<i16>,
    pub issue_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub returned_at: Option<DateTime<Utc>>,
    pub last_reminder_sent_at: Option<DateTime<Utc>>,
    pub reminder_count: Option<i32>,
}

/// Loan with full details for display
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoanDetails {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub start_date: DateTime<Utc>,
    pub issue_at: DateTime<Utc>,
    pub renewal_date: Option<DateTime<Utc>>,
    pub nb_renews: i16,
    pub returned_at: Option<DateTime<Utc>>,
    pub item: ItemShort,
    pub user: Option<UserShort>,
    pub specimen_identification: Option<String>,
    pub is_overdue: bool,
}

/// Create loan request
#[serde_as]
#[derive(Debug, Deserialize)]
pub struct CreateLoan {
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub specimen_id: Option<i64>,
    pub specimen_identification: Option<String>,
    pub force: bool,
}

/// Loan settings by media type
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LoanSettings {
    #[serde_as(as = "DisplayFromStr")]
    pub id: i64,
    pub media_type: Option<MediaType>,
    pub nb_max: Option<i16>,
    pub nb_renews: Option<i16>,
    pub duration: Option<i16>,
    pub notes: Option<String>,
    pub account_type: Option<String>,
}

/// Archived loan for statistics
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LoanArchive {
    #[serde_as(as = "DisplayFromStr")]
    pub id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub specimen_id: Option<i64>,
    pub date: DateTime<Utc>,
    pub nb_renews: Option<i16>,
    pub issue_at: Option<DateTime<Utc>>,
    pub returned_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub borrower_public_type: Option<i64>,
    pub addr_city: Option<String>,
    pub account_type: Option<String>,
}

