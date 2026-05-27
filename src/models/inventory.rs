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
    /// Copy exists in catalog but does not match session scope (source and/or place).
    FoundOutOfScope,
}

impl InventoryScanResult {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Found => "found",
            Self::UnknownBarcode => "unknown_barcode",
            Self::FoundArchived => "found_archived",
            Self::FoundOutOfScope => "found_out_of_scope",
        }
    }
}

impl From<String> for InventoryScanResult {
    fn from(s: String) -> Self {
        match s.as_str() {
            "unknown_barcode" => Self::UnknownBarcode,
            "found_archived" => Self::FoundArchived,
            "found_out_of_scope" => Self::FoundOutOfScope,
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
    /// When set, scope is limited to active items with this `items.source_id`.
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub scope_source_id: Option<i64>,
    /// Resolved from `sources.name` when `scope_source_id` is set (not stored).
    #[serde(default)]
    #[sqlx(default)]
    pub scope_source_name: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub created_by: Option<i64>,
    /// Set when missing copies were archived from the catalog (`POST …/consolidate`).
    pub consolidated_at: Option<DateTime<Utc>>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub consolidated_by: Option<i64>,
}

/// Create inventory session request
#[serde_as]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInventorySession {
    pub name: String,
    pub location_filter: Option<String>,
    pub notes: Option<String>,
    pub scope_place: Option<i16>,
    /// When set, only active items with this `items.source_id` are in scope.
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub scope_source_id: Option<i64>,
}

/// Response for `POST /inventory/sessions` (includes optional warnings).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInventorySessionResponse {
    pub session: InventorySession,
    /// Non-blocking warnings (e.g. zero expected copies in scope).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Active copy count matching session scope at creation time.
    pub expected_in_scope: i64,
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
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub source_id: Option<i64>,
    pub source_name: Option<String>,
}

/// Discrepancy report for a session (enriched).
///
/// Count formulas (session `S`):
/// - `expectedInScope`: active items where scope predicates on `items.source_id` and `items.place` match.
/// - `missingCount`: in-scope active items with no scan row having `item_id = item.id` and `result = found`.
/// - `missingScannable`: subset of `missingCount` with non-null barcode.
/// - `missingWithoutBarcode`: in-scope active with `barcode IS NULL` (cannot be captured by barcode scan).
/// - `totalFound`: scans with `result = found` (in-scope confirmations only).
/// - `totalFoundOutOfScope`: scans with `result = found_out_of_scope`.
/// - `distinctItemsScanned`: `COUNT(DISTINCT item_id)` over scans for S where `result = found`.
/// - `duplicateScanCount`: in-scope found scans minus `distinctItemsScanned`.
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
    pub total_found_out_of_scope: i64,
    pub total_unknown: i64,
    pub distinct_items_scanned: i64,
    pub duplicate_scan_count: i64,
    pub missing_count: i64,
    pub missing_scannable: i64,
    pub missing_without_barcode: i64,
}

/// Request body for `POST /inventory/sessions/{id}/consolidate`.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidateInventorySession {
    /// When true, active loans are auto-returned before archiving missing copies.
    #[serde(default)]
    pub force: bool,
}

/// One copy that could not be archived during consolidation.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationSkipped {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub item_id: i64,
    pub reason: String,
}

/// Result of consolidating a closed inventory session against the catalog.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationResult {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub session_id: i64,
    /// Missing copies considered for archival.
    pub attempted: i64,
    /// Copies successfully archived.
    pub deleted: i64,
    /// Copies skipped (e.g. currently on loan when `force` is false).
    pub skipped: Vec<InventoryConsolidationSkipped>,
    /// Whether the session was marked consolidated (`true` only when nothing was skipped).
    pub consolidated: bool,
    /// Bibliographic records archived because they had no active copies left.
    pub archived_biblios: i64,
    /// Emails sent to readers whose loans were closed (`force: true` only).
    pub loan_closure_emails_sent: u64,
    /// Email delivery failures (consolidation still succeeds).
    pub loan_closure_email_errors: Vec<InventoryConsolidationEmailError>,
}

/// Email delivery failure during consolidation loan-closure notifications.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationEmailError {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    pub email: String,
    pub error_message: String,
}

/// Summary counts for consolidation preview (`GET …/consolidate/preview`).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationPreviewSummary {
    pub total_missing: i64,
    pub on_loan_count: i64,
    pub deletable_without_force: i64,
    /// Distinct biblios that would have zero active copies after consolidation.
    pub orphan_biblios_count: i64,
    /// Distinct readers with an active loan on a missing copy.
    pub affected_readers_count: i64,
}

/// Active loan on a missing copy (preview).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationPreviewLoan {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub loan_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    pub user_email: Option<String>,
    pub user_firstname: Option<String>,
    pub user_lastname: Option<String>,
    pub expiry_at: Option<DateTime<Utc>>,
}

/// One missing copy that would be archived on consolidation.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationPreviewRow {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub item_id: i64,
    pub barcode: Option<String>,
    pub call_number: Option<String>,
    pub place: Option<i16>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub source_id: Option<i64>,
    pub source_name: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub biblio_id: Option<i64>,
    pub biblio_title: Option<String>,
    pub on_loan: bool,
    pub would_skip_without_force: bool,
    pub biblio_would_be_orphaned: bool,
    pub active_loan: Option<InventoryConsolidationPreviewLoan>,
}

/// Paginated consolidation preview for a closed session.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsolidationPreview {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub session_id: i64,
    pub summary: InventoryConsolidationPreviewSummary,
    pub items: Vec<InventoryConsolidationPreviewRow>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub page_count: i64,
}
