//! Loan (borrow) model and related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

use super::biblio::{Biblio, BiblioShort, MediaType};
use super::user::UserShort;

/// Loan model from database
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Loan {
    #[serde_as(as = "DisplayFromStr")]
    pub id: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub item_id: i64,
    pub date: DateTime<Utc>,
    pub renew_at: Option<DateTime<Utc>>,
    pub nb_renews: Option<i16>,
    pub expiry_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub returned_at: Option<DateTime<Utc>>,
    pub last_reminder_sent_at: Option<DateTime<Utc>>,
    pub reminder_count: Option<i32>,
}

/// Loan with full details for display
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoanDetails {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    /// Borrowed specimen (`items.id`).
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub item_id: i64,
    pub start_date: DateTime<Utc>,
    pub expiry_at: DateTime<Utc>,
    pub renewal_date: Option<DateTime<Utc>>,
    pub nb_renews: i16,
    pub returned_at: Option<DateTime<Utc>>,
    pub biblio: BiblioShort,
    pub user: Option<UserShort>,
    pub item_identification: Option<String>,
    pub is_overdue: bool,
}

/// Result of [`crate::repository::Repository::loans_return`]: archived loan details and optional hold advanced to `ready`.
#[derive(Debug, Clone)]
pub struct LoanReturnOutcome {
    pub details: LoanDetails,
    pub readied_hold: Option<crate::models::hold::Hold>,
    /// Outcome of the hold-ready email attempt (when a hold was advanced to `ready`).
    pub hold_ready_email: Option<HoldReadyEmailOutcome>,
}

/// Hold-ready notification email attempt after a loan return.
#[derive(Debug, Clone)]
pub struct HoldReadyEmailOutcome {
    pub email: Option<String>,
    pub send_error: Option<String>,
}

/// Result of [`crate::repository::Repository::loans_create`].
#[derive(Debug, Clone)]
pub struct LoanCreateOutcome {
    pub loan_id: i64,
    pub expiry_at: DateTime<Utc>,
    /// Active hold fulfilled when the patron checked out (not when `force` clears holds).
    pub fulfilled_hold_id: Option<i64>,
}

/// How the new due date is computed when a loan is renewed (`loans_settings.renew_at`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoanSettingsRenewAt {
    /// New due date = instant of renewal + loan duration.
    #[default]
    Now,
    /// New due date = current due date + loan duration.
    AtDueDate,
}

impl LoanSettingsRenewAt {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::AtDueDate => "at_due_date",
        }
    }
}

impl From<&str> for LoanSettingsRenewAt {
    fn from(s: &str) -> Self {
        match s {
            "at_due_date" => Self::AtDueDate,
            _ => Self::Now,
        }
    }
}

impl sqlx::Type<sqlx::Postgres> for LoanSettingsRenewAt {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for LoanSettingsRenewAt {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s: String = <String as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(Self::from(s.as_str()))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for LoanSettingsRenewAt {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.as_db_str().to_string(), buf)
    }
}

/// Create loan request
#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLoan {
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub item_id: Option<i64>,
    pub item_identification: Option<String>,
    pub force: bool,
}

/// Loan settings: `nb_max` on the default row (`media_type` IS NULL) caps **all** active loans;
/// on a per-media row, `nb_max` caps loans for that medium only.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LoanSettings {
    #[serde_as(as = "DisplayFromStr")]
    pub id: i64,
    pub media_type: Option<MediaType>,
    pub nb_max: Option<i16>,
    pub nb_renews: Option<i16>,
    pub duration: Option<i16>,
    pub renew_at: Option<LoanSettingsRenewAt>,
    pub notes: Option<String>,
}

/// One loan row for MARC export (full list, no pagination).
#[derive(Debug, Clone)]
pub struct LoanMarcExportRow {
    pub biblio: Biblio,
    pub start_date: DateTime<Utc>,
    pub expiry_at: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
}

/// Maximum loans included in a single MARC export response (safety cap).
pub const LOANS_MARC_EXPORT_MAX: usize = 2000;

/// File format for [`crate::services::loans::LoansService::export_user_loans_marc_file`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LoanMarcExportFormat {
    #[default]
    Json,
    Marc21,
    Unimarc,
    Marcxml,
}

/// Binary MARC encoding for ISO2709 export (query param `encoding`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LoanMarcExportEncoding {
    #[default]
    Utf8,
    Marc8,
}

/// Archived loan for statistics
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LoanArchive {
    #[serde_as(as = "DisplayFromStr")]
    pub id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub item_id: Option<i64>,
    pub date: DateTime<Utc>,
    pub nb_renews: Option<i16>,
    pub expiry_at: Option<DateTime<Utc>>,
    pub returned_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub borrower_public_type: Option<i64>,
    pub addr_city: Option<String>,
    pub account_type: Option<String>,
}
