//! Library info DTOs shared by API and services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub name: Option<String>,
    pub addr_line1: Option<String>,
    pub addr_line2: Option<String>,
    pub addr_postcode: Option<String>,
    pub addr_city: Option<String>,
    pub addr_country: Option<String>,
    pub phones: Vec<String>,
    pub email: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLibraryInfoRequest {
    pub name: Option<String>,
    pub addr_line1: Option<String>,
    pub addr_line2: Option<String>,
    pub addr_postcode: Option<String>,
    pub addr_city: Option<String>,
    pub addr_country: Option<String>,
    pub phones: Option<Vec<String>>,
    pub email: Option<String>,
}
