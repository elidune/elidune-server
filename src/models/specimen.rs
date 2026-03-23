//! Specimen (physical copy) model and related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;
use validator::Validate;

fn default_borrowable() -> bool {
    true
}

/// Full specimen model from database.
/// Soft delete is tracked solely via `archived_at` (NULL = active, set = archived).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema, Validate)]
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
    #[validate(length(max = 100, message = "Barcode must be at most 100 characters"))]
    pub barcode: Option<String>,
    #[validate(length(max = 200, message = "Call number must be at most 200 characters"))]
    pub call_number: Option<String>,
    #[validate(length(max = 100, message = "Volume designation must be at most 100 characters"))]
    pub volume_designation: Option<String>,
    pub place: Option<i16>,
    #[serde(default = "default_borrowable")]
    pub borrowable: bool,
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
    pub borrowable: bool,
    pub source_name: Option<String>,
    pub availability: Option<i64>,
}


impl From<Specimen> for SpecimenShort {
    fn from(specimen: Specimen) -> Self {
        Self {
            id: specimen.id.unwrap_or(0),
            barcode: specimen.barcode,
            call_number: specimen.call_number,
            borrowable: specimen.borrowable,
            source_name: specimen.source_name,
            availability: specimen.availability,
        }
    }
}