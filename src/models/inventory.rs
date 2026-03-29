//! Inventory / stocktaking model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Result of resolving a scanned barcode against `items`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InventoryScanResult {
    Found,
    UnknownBarcode,
    FoundArchived,
}

impl InventoryScanResult {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Found => "found",
            Self::UnknownBarcode => "unknown_barcode",
            Self::FoundArchived => "found_archived",
        }
    }
}

impl From<String> for InventoryScanResult {
    fn from(s: String) -> Self {
        match s.as_str() {
            "unknown_barcode" => Self::UnknownBarcode,
            "found_archived" => Self::FoundArchived,
            _ => Self::Found,
        }
    }
}

impl sqlx::Type<sqlx::Postgres> for InventoryScanResult {
    /// Match `inventory_scans.result VARCHAR` (not `TEXT`, which is what `String` maps to).
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("VARCHAR")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == Self::type_info()
            || <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for InventoryScanResult {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s: String = sqlx::Decode::<sqlx::Postgres>::decode(value)?;
        Ok(Self::from(s))
    }
}

impl sqlx::Encode<'_, sqlx::Postgres> for InventoryScanResult {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Postgres>>::encode(self.as_str().to_string(), buf)
    }
}

impl sqlx::postgres::PgHasArrayType for InventoryScanResult {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::postgres::PgHasArrayType>::array_type_info()
    }
}

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
        if s == "closed" {
            Self::Closed
        } else {
            Self::Open
        }
    }
}

impl sqlx::Type<sqlx::Postgres> for InventoryStatus {
    /// Match `inventory_sessions.status VARCHAR` (not `TEXT`, which is what `String` maps to).
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("VARCHAR")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == Self::type_info()
            || <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
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
#[serde(rename_all = "camelCase")]
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
    /// When set, report and missing list only include active items with this `items.place`.
    pub scope_place: Option<i16>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub created_by: Option<i64>,
}

/// Create inventory session request
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInventorySession {
    pub name: String,
    pub location_filter: Option<String>,
    pub notes: Option<String>,
    pub scope_place: Option<i16>,
}

/// Individual scan within a session
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
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
    pub result: InventoryScanResult,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub scanned_by: Option<i64>,
}

/// Scan a barcode in a session
#[derive(Debug, Deserialize, ToSchema)]
pub struct ScanBarcode {
    pub barcode: String,
}

/// Batch scan request (`POST .../scans/batch`)
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchScanBarcodes {
    pub barcodes: Vec<String>,
}

/// One physical copy in scope that was never linked by any scan in the session (`item_id`).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryMissingRow {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub item_id: i64,
    pub barcode: Option<String>,
    pub call_number: Option<String>,
    pub place: Option<i16>,
    pub biblio_title: Option<String>,
}

/// Discrepancy report for a session (enriched).
///
/// Count formulas (session `S`):
/// - `expectedInScope`: active items where `(S.scope_place IS NULL OR item.place = S.scope_place)`.
/// - `missingCount`: in-scope active items with no scan row having `item_id = item.id`.
/// - `missingScannable`: subset of `missingCount` with non-null barcode.
/// - `missingWithoutBarcode`: in-scope active with `barcode IS NULL` (cannot be captured by barcode scan).
/// - `distinctItemsScanned`: `COUNT(DISTINCT item_id)` over scans for S where `item_id IS NOT NULL`.
/// - `duplicateScanCount`: scans with non-null `item_id` minus `distinctItemsScanned`.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReport {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub session_id: i64,
    pub expected_in_scope: i64,
    pub total_scanned: i64,
    pub total_found: i64,
    pub total_found_archived: i64,
    pub total_unknown: i64,
    pub distinct_items_scanned: i64,
    pub duplicate_scan_count: i64,
    pub missing_count: i64,
    pub missing_scannable: i64,
    pub missing_without_barcode: i64,
}
