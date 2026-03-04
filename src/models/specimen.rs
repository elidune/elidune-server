//! Specimen (physical copy) model and related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Specimen borrow status (can it be borrowed?)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i16)]
pub enum SpecimenBorrowStatus {
    Borrowable = 98,
    NotBorrowable = 110,
}

impl From<i16> for SpecimenBorrowStatus {
    fn from(v: i16) -> Self {
        match v {
            98 => SpecimenBorrowStatus::Borrowable,
            _ => SpecimenBorrowStatus::NotBorrowable,
        }
    }
}

/// Full specimen model from database.
/// Soft delete is tracked solely via `archived_at` (NULL = active, set = archived).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Specimen {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    pub id: Option<i64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub item_id: Option<i64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub source_id: Option<i64>,
    pub barcode: Option<String>,
    pub call_number: Option<String>,
    pub volume_designation: Option<String>,
    pub place: Option<i16>,
    pub borrow_status: Option<i16>,
    pub circulation_status: Option<i16>,
    pub notes: Option<String>,
    pub price: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub source_name: Option<String>,
    #[serde(default)]
    pub availability: Option<i64>,

}

impl Specimen {
    pub fn is_available(&self) -> bool {
        self.archived_at.is_none() && self.availability.unwrap_or(0) > 0
    }

    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct SpecimenShort {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub barcode: Option<String>,
    pub call_number: Option<String>,
    pub borrow_status: Option<i16>,
    pub source_name: Option<String>,
    pub availability: Option<i64>,
}


impl From<Specimen> for SpecimenShort {
    fn from(specimen: Specimen) -> Self {
        Self {
            id: specimen.id.unwrap_or(0),
            barcode: specimen.barcode,
            call_number: specimen.call_number,
            borrow_status: specimen.borrow_status,
            source_name: specimen.source_name,
            availability: specimen.availability,
        }
    }
}