//! Import report models for ISBN deduplication logic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::models::biblio::BiblioShort;
use crate::models::item::ItemShort;

/// Result of an ISBN duplicate lookup before import.
#[derive(Debug, Clone)]
pub struct DuplicateCandidate {
    pub biblio_id: i64,
    pub archived_at: Option<DateTime<Utc>>,
    /// Number of active (non-archived) items (physical copies) linked to this biblio.
    pub item_count: i64,
}

/// What happened during import.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImportAction {
    Created,
    MergedBibliographic,
    ReplacedArchived,
    ReplacedConfirmed,
}

/// Report returned alongside the imported/updated biblio.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub action: ImportAction,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Body returned on 409 when confirmation is required (duplicate ISBN).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateConfirmationRequired {
    pub code: String,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub existing_id: i64,
    pub existing_biblio: BiblioShort,
    pub message: String,
}

/// Body returned on 409 when a physical item barcode conflict is detected.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateItemBarcodeRequired {
    pub code: String,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub existing_id: i64,
    pub existing_item: ItemShort,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::biblio::MediaType;

    #[test]
    fn duplicate_isbn_response_serializes_with_existing_biblio() {
        let resp = DuplicateConfirmationRequired {
            code: "duplicate_isbn_needs_confirmation".to_string(),
            existing_id: 42,
            existing_biblio: BiblioShort {
                id: 42,
                media_type: MediaType::PrintedText,
                isbn: Some(crate::models::biblio::Isbn::new("9782070408504")),
                title: Some("Test Book".to_string()),
                date: None,
                status: 0,
                is_valid: None,
                archived_at: None,
                author: None,
                items: Vec::new(),
            },
            message: "Duplicate".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"code\":\"duplicate_isbn_needs_confirmation\""));
        assert!(json.contains("\"existingId\":\"42\""));
        assert!(json.contains("\"existingBiblio\""));
        assert!(json.contains("\"title\":\"Test Book\""));
    }

    #[test]
    fn import_report_created_action_serializes() {
        let report = ImportReport {
            action: ImportAction::Created,
            existing_id: None,
            warnings: vec!["No ISBN".to_string()],
            message: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"action\":\"created\""));
        assert!(json.contains("\"No ISBN\""));
        assert!(!json.contains("\"existingId\""));
        assert!(!json.contains("\"message\""));
    }

    #[test]
    fn import_report_merged_action_serializes() {
        let report = ImportReport {
            action: ImportAction::MergedBibliographic,
            existing_id: Some(42),
            warnings: vec![],
            message: Some("Merged into 42".to_string()),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"action\":\"mergedBibliographic\""));
        assert!(json.contains("\"existingId\":\"42\""));
        assert!(json.contains("\"message\":\"Merged into 42\""));
        assert!(!json.contains("\"warnings\""));
    }
}
