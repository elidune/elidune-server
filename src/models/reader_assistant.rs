use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::biblio::BiblioShort;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSession {
    pub id: i64,
    pub user_id: i64,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content_redacted: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token_usage: Option<i32>,
    pub latency_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationKind {
    InCatalog,
    External,
}

impl RecommendationKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::InCatalog => "in_catalog",
            Self::External => "external",
        }
    }
}

impl From<&str> for RecommendationKind {
    fn from(value: &str) -> Self {
        match value {
            "external" => Self::External,
            _ => Self::InCatalog,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AiRecommendationRow {
    pub id: i64,
    pub message_id: i64,
    pub kind: String,
    pub biblio_id: Option<i64>,
    pub external_ref: Option<serde_json::Value>,
    pub score: f64,
    pub rationale: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationRef {
    pub title: String,
    pub author: Option<String>,
    pub publication_year: Option<i32>,
    pub source_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationItem {
    pub id: i64,
    pub kind: RecommendationKind,
    pub biblio_id: Option<i64>,
    pub biblio: Option<BiblioShort>,
    pub external_ref: Option<RecommendationRef>,
    pub score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateAssistantSessionRequest {
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendAssistantMessageRequest {
    pub content: String,
    pub include_external: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskRequest {
    pub session_id: Option<i64>,
    pub content: String,
    pub include_external: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token_usage: Option<i32>,
    pub latency_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub recommendations: Vec<RecommendationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: AiSession,
    pub messages: Vec<SessionMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistantReply {
    pub session_id: i64,
    pub assistant_message_id: i64,
    pub answer: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub fallback_used: bool,
    pub recommendations: Vec<RecommendationItem>,
}

#[derive(Debug, Clone)]
pub struct NewRecommendation {
    pub kind: RecommendationKind,
    pub biblio_id: Option<i64>,
    pub external_ref: Option<serde_json::Value>,
    pub score: f64,
    pub rationale: String,
}
