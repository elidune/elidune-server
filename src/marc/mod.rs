//! MARC record parsing and translation
//!
//! This module provides functionality to parse MARC21 and UNIMARC records
//! and translate them into the internal Item structure.

pub mod translator;

pub use translator::{biblio_items_to_marc_items, marc_record_for_loan_export};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use utoipa::ToSchema;
use z3950_rs::marc_rs::RecordValidationIssue;
use serde_with::DisplayFromStr;
pub use z3950_rs::marc_rs::{Record as MarcRecord, MarcFormat};

use crate::models::{Author, BiblioShort, ItemShort, MediaType, biblio::Isbn};




#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarcImportPreview {
    #[serde(flatten)]
    pub biblio: BiblioShort,
    pub validation_issues: Vec<RecordValidationIssue>,
}
