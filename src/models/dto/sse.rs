//! Server-Sent Events payload types.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SsePayload {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_id: Option<String>,
}
