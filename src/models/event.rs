//! Event model (cultural actions, school visits, animations)

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};

/// Event record
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
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
}

/// Create event request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
}

/// Update event request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
}

/// Query parameters for events
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
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
