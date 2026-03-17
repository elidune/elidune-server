//! Import report models for ISBN deduplication logic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::models::item::ItemShort;
use crate::models::specimen::SpecimenShort;

/// Result of an ISBN duplicate lookup before import.
#[derive(Debug, Clone)]
pub struct DuplicateCandidate {
    pub item_id: i64,
    pub archived_at: Option<DateTime<Utc>>,
    /// Number of active (non-archived) specimens linked to this item.
    pub specimen_count: i64,
}

/// What happened during import.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportAction {
    Created,
    MergedBibliographic,
    ReplacedArchived,
    ReplacedConfirmed,
}

/// Report returned alongside the imported/updated item.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

/// Body returned on 409 when confirmation is required.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DuplicateConfirmationRequired {
    pub code: String,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub existing_id: i64,
    pub existing_item: ItemShort,
    pub message: String,
}

/// Body returned on 409 when a specimen barcode conflict is detected.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DuplicateSpecimenBarcodeRequired {
    pub code: String,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub existing_id: i64,
    pub existing_specimen: SpecimenShort,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::item::MediaType;

    #[test]
    fn duplicate_isbn_response_serializes_with_existing_item() {
        let resp = DuplicateConfirmationRequired {
            code: "duplicate_isbn_needs_confirmation".to_string(),
            existing_id: 42,
            existing_item: ItemShort {
                id: 42,
                media_type: MediaType::PrintedText,
                isbn: Some(crate::models::item::Isbn::new("9782070408504")),
                title: Some("Test Book".to_string()),
                date: None,
                status: 0,
                is_valid: None,
                archived_at: None,
                author: None,
                specimens: Vec::new(),
            },
            message: "Duplicate".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"code\":\"duplicate_isbn_needs_confirmation\""));
        assert!(json.contains("\"existing_id\":\"42\""));
        assert!(json.contains("\"existing_item\""));
        assert!(json.contains("\"title\":\"Test Book\""));
    }

    #[test]
    fn duplicate_isbn_response_roundtrips() {
        let resp = DuplicateConfirmationRequired {
            code: "duplicate_isbn_needs_confirmation".to_string(),
            existing_id: 99,
            existing_item: ItemShort {
                id: 99,
                media_type: MediaType::Unknown,
                isbn: None,
                title: Some("Roundtrip".to_string()),
                date: None,
                status: 0,
                is_valid: None,
                archived_at: None,
                author: None,
                specimens: Vec::new(),
            },
            message: "test".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DuplicateConfirmationRequired = serde_json::from_str(&json).unwrap();
        assert_eq!(back.existing_id, 99);
        assert_eq!(back.code, "duplicate_isbn_needs_confirmation");
    }

    #[test]
    fn duplicate_barcode_response_serializes_with_existing_specimen() {
        let resp = DuplicateSpecimenBarcodeRequired {
            code: "duplicate_barcode_needs_confirmation".to_string(),
            existing_id: 7,
            existing_specimen: SpecimenShort {
                id: 7,
                barcode: Some("BC001".to_string()),
                call_number: Some("A1".to_string()),
                borrowable: true,
                source_name: None,
                availability: Some(0),
            },
            message: "Barcode conflict".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"code\":\"duplicate_barcode_needs_confirmation\""));
        assert!(json.contains("\"existing_id\":\"7\""));
        assert!(json.contains("\"existing_specimen\""));
        assert!(json.contains("\"barcode\":\"BC001\""));
    }

    #[test]
    fn duplicate_barcode_response_roundtrips() {
        let resp = DuplicateSpecimenBarcodeRequired {
            code: "duplicate_barcode_needs_confirmation".to_string(),
            existing_id: 15,
            existing_specimen: SpecimenShort {
                id: 15,
                barcode: Some("BC999".to_string()),
                call_number: None,
                borrowable: false,
                source_name: Some("Source1".to_string()),
                availability: None,
            },
            message: "test".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DuplicateSpecimenBarcodeRequired = serde_json::from_str(&json).unwrap();
        assert_eq!(back.existing_id, 15);
        assert_eq!(back.existing_specimen.barcode, Some("BC999".to_string()));
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
        assert!(!json.contains("\"existing_id\""));
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
        assert!(json.contains("\"action\":\"merged_bibliographic\""));
        assert!(json.contains("\"existing_id\":\"42\""));
        assert!(json.contains("\"message\":\"Merged into 42\""));
        assert!(!json.contains("\"warnings\""));
    }
}
