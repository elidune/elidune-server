//! Inventory / stocktaking model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Inventory session status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InventoryStatus {
    Open,
    Closed,
}

impl InventoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

impl From<String> for InventoryStatus {
    fn from(s: String) -> Self {
        if s == "closed" { Self::Closed } else { Self::Open }
    }
}

impl sqlx::Type<sqlx::Postgres> for InventoryStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for InventoryStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s: String = sqlx::Decode::<sqlx::Postgres>::decode(value)?;
        Ok(Self::from(s))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for InventoryStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.as_str().to_string(), buf)
    }
}

/// Inventory session
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct InventorySession {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub name: String,
    pub started_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub status: InventoryStatus,
    pub location_filter: Option<String>,
    pub notes: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub created_by: Option<i64>,
}

/// Create inventory session request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInventorySession {
    pub name: String,
    pub location_filter: Option<String>,
    pub notes: Option<String>,
}

/// Individual scan within a session
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct InventoryScan {
    pub id: i64,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub session_id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub item_id: Option<i64>,
    pub barcode: String,
    pub scanned_at: DateTime<Utc>,
    pub result: String,
}

/// Scan a barcode in a session
#[derive(Debug, Deserialize, ToSchema)]
pub struct ScanBarcode {
    pub barcode: String,
}

/// Discrepancy report for a closed session
#[derive(Debug, Serialize, ToSchema)]
pub struct InventoryReport {
    pub session_id: i64,
    pub total_scanned: i64,
    pub total_found: i64,
    pub total_unknown: i64,
    /// Items (physical copies) not scanned during this session (potentially missing)
    pub missing_count: i64,
}
