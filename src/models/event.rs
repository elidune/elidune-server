//! Event model (cultural actions, school visits, animations)

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};

/// Optional attachment supplied when creating an event (Base64-encoded payload).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventAttachmentInput {
    /// Display file name (path segments are stripped server-side).
    pub file_name: String,
    /// MIME type (e.g. `application/pdf`, `image/png`).
    pub mime_type: String,
    /// File content encoded as standard Base64.
    pub data_base64: String,
}

/// Event record
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    /// Event name
    pub name: String,
    /// Type (0=animation, 1=school_visit, 2=exhibition, 3=conference, 4=workshop, 5=show, 6=other)
    pub event_type: i16,
    /// Event date
    pub event_date: NaiveDate,
    /// Start time
    pub start_time: Option<NaiveTime>,
    /// End time
    pub end_time: Option<NaiveTime>,
    /// Number of attendees
    pub attendees_count: Option<i32>,
    /// Target audience (97=adult, 106=children, NULL=all)
    pub target_public: Option<i16>,
    /// School name (for school visits)
    pub school_name: Option<String>,
    /// Class name (for school visits)
    pub class_name: Option<String>,
    /// Number of students (for school visits)
    pub students_count: Option<i32>,
    /// Partner organization name
    pub partner_name: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub update_at: Option<DateTime<Utc>>,
    /// Date the announcement email was last sent
    pub announcement_sent_at: Option<DateTime<Utc>>,
    /// Original attachment file name when present
    pub attachment_filename: Option<String>,
    /// Attachment MIME type when present
    pub attachment_mime_type: Option<String>,
    /// Attachment size in bytes when present.
    pub attachment_size: Option<i32>,
    /// Full attachment as standard Base64. Included only in single-event responses (`GET` / `POST` / `PUT`), not in list responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub attachment_data_base64: Option<String>,
}

/// Create event request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateEvent {
    pub name: String,
    /// Type (0=animation, 1=school_visit, 2=exhibition, 3=conference, 4=workshop, 5=show, 6=other)
    pub event_type: Option<i16>,
    /// Event date (YYYY-MM-DD)
    pub event_date: String,
    /// Start time (HH:MM)
    pub start_time: Option<String>,
    /// End time (HH:MM)
    pub end_time: Option<String>,
    pub attendees_count: Option<i32>,
    /// Target audience (97=adult, 106=children)
    pub target_public: Option<i16>,
    pub school_name: Option<String>,
    pub class_name: Option<String>,
    pub students_count: Option<i32>,
    pub partner_name: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    /// Optional attachment (stored in-database; max size enforced server-side).
    pub attachment: Option<EventAttachmentInput>,
}

/// Update event request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEvent {
    pub name: Option<String>,
    pub event_type: Option<i16>,
    pub event_date: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub attendees_count: Option<i32>,
    pub target_public: Option<i16>,
    pub school_name: Option<String>,
    pub class_name: Option<String>,
    pub students_count: Option<i32>,
    pub partner_name: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    /// When `true`, removes the attachment. Takes precedence over `attachment`.
    pub remove_attachment: Option<bool>,
    /// Replaces the attachment (same shape as in [`CreateEvent`]).
    pub attachment: Option<EventAttachmentInput>,
}

/// Query parameters for events
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventQuery {
    /// Filter by start date (YYYY-MM-DD)
    pub start_date: Option<String>,
    /// Filter by end date (YYYY-MM-DD)
    pub end_date: Option<String>,
    /// Filter by event type
    pub event_type: Option<i16>,
    /// Page number (1-based)
    pub page: Option<i64>,
    /// Items per page
    pub per_page: Option<i64>,
}
