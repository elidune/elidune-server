//! Background task state models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

/// Kind of long-running background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    MarcBatchImport,
    Maintenance,
}

/// Lifecycle status of a background task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Step-level progress within a running task.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    /// Number of items already processed.
    pub current: usize,
    /// Total items to process (0 if unknown).
    pub total: usize,
    /// Arbitrary serialisable payload describing the current step.
    /// Can be a plain string, an object with structured fields, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<serde_json::Value>,
}

/// A background task tracked by the server.
///
/// Active tasks live in memory.  Completed/failed tasks are persisted to Redis
/// (TTL 24 h) so the frontend can retrieve them after a page refresh or
/// reconnect.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    /// Unique task identifier (Snowflake, serialised as a string to preserve
    /// JS number precision).
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,

    pub kind: TaskKind,
    pub status: TaskStatus,

    /// Present only while the task is `Running`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,

    /// Typed result payload; present only when `status == Completed`.
    ///
    /// Shape depends on `kind`:
    /// - `marcBatchImport` → `MarcBatchImportReport`
    /// - `maintenance`     → `MaintenanceResponse` (per-action `details` may include Z39.50 summaries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Error description; present only when `status == Failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// User who created the task.
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user_id: i64,
}
