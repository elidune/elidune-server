//! Z39.50 DTOs shared by API and services.

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::{IntoParams, ToSchema};

fn default_z3950_encoding() -> String {
    "utf-8".to_string()
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Z3950ServerConfig {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub name: String,
    pub address: String,
    pub port: i32,
    pub database: Option<String>,
    pub format: Option<String>,
    pub login: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_z3950_encoding")]
    pub encoding: String,
    pub is_active: bool,
}

#[serde_as]
#[derive(Deserialize, IntoParams, ToSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Z3950SearchQuery {
    pub query: String,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub server_id: Option<i64>,
    pub max_results: Option<i32>,
}

#[serde_as]
#[derive(Debug, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub barcode: Option<String>,
    pub call_number: Option<String>,
    pub status: Option<String>,
    pub place: Option<i16>,
    pub notes: Option<String>,
    pub price: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub source_id: Option<i64>,
}
