//! Declarative stats query builder types (flexible `/stats/query` API).

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

/// JSON body for `POST /stats/query` and stored `query_json` for saved queries.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsBuilderBody {
    pub entity: String,
    #[serde(default)]
    pub joins: Vec<String>,
    #[serde(default)]
    pub select: Vec<SelectField>,
    #[serde(default)]
    pub filters: Vec<StatsFilter>,
    #[serde(default)]
    pub aggregations: Vec<StatsAggregation>,
    #[serde(default)]
    pub group_by: Vec<GroupByField>,
    #[serde(default)]
    pub having: Vec<HavingFilter>,
    pub time_bucket: Option<TimeBucket>,
    #[serde(default)]
    pub order_by: Vec<StatsOrderBy>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelectField {
    pub field: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupByField {
    pub field: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsFilter {
    pub field: String,
    pub op: FilterOperator,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HavingFilter {
    pub field: String,
    pub op: FilterOperator,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "notIn")]
    NotIn,
    #[serde(rename = "isNull")]
    IsNull,
    #[serde(rename = "isNotNull")]
    IsNotNull,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsAggregation {
    #[serde(rename = "fn")]
    pub function: AggregateFunction,
    pub field: String,
    pub alias: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum AggregateFunction {
    Count,
    CountDistinct,
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeBucket {
    pub field: String,
    pub granularity: TimeGranularity,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TimeGranularity {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsOrderBy {
    pub field: String,
    pub dir: Option<SortDirection>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Tabular result for builder queries.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsTableResponse {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub total_rows: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub label: String,
    pub data_type: String,
}

/// Saved query row for `GET /stats/saved`.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SavedStatsQuery {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub query: StatsBuilderBody,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
    pub is_shared: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create or update saved query.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SavedStatsQueryWrite {
    pub name: String,
    pub description: Option<String>,
    pub query: StatsBuilderBody,
    #[serde(default)]
    pub is_shared: bool,
}
