//! Source model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Source record
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub key: Option<String>,
    pub name: Option<String>,
    pub is_archive: Option<i16>,
    pub archived_at: Option<DateTime<Utc>>,
    pub default: Option<bool>,
}

/// Create source request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateSource {
    /// Source name
    pub name: String,
    /// Set as default source
    pub default: Option<bool>,
}

/// Update source request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateSource {
    /// New name for the source
    pub name: Option<String>,
    /// Set as default source
    pub default: Option<bool>,
}

/// Merge sources request
#[serde_as]
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MergeSources {
    /// IDs of sources to merge
    #[serde_as(as = "Vec<DisplayFromStr>")]
    #[schema(value_type = Vec<String>)]
    pub source_ids: Vec<i64>,
    /// Name for the new merged source
    pub name: String,
}
