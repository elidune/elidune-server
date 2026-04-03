//! Events service

use std::path::Path;
use std::sync::Arc;

/// Maximum size for an event attachment (10 MiB).
pub const MAX_EVENT_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use crate::{
    dynamic_config::DynamicConfig,
    error::{AppError, AppResult},
    models::{
        event::{CreateEvent, Event, EventAttachmentInput, EventQuery, UpdateEvent},
        Language,
    },
    repository::{events::EventAnnualStats, EventsServiceRepository},
    services::{
        audit::{self, AuditService},
        email::EmailService,
        email_templates,
    },
};

/// Request body for sending an event announcement email.
/// All fields are optional: if omitted, the default template is used.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendAnnouncementRequest {
    /// Override email subject (uses template if absent)
    pub subject: Option<String>,
    /// Override plain-text body (uses template if absent)
    pub body_plain: Option<String>,
    /// Override HTML body (derived from body_plain or template if absent)
    pub body_html: Option<String>,
}

/// Per-recipient error collected during a bulk send
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementError {
    pub user_id: i64,
    pub email: String,
    pub error_message: String,
}

/// Summary returned after sending an event announcement
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementReport {
    pub event_id: i64,
    /// Number of emails successfully sent
    pub emails_sent: u32,
    /// Number of recipients skipped (no email address)
    pub skipped: u32,
    /// Per-recipient send errors
    pub errors: Vec<AnnouncementError>,
}

#[derive(Clone)]
pub struct EventsService {
    repository: Arc<dyn EventsServiceRepository>,
    email: EmailService,
    audit: AuditService,
    dynamic_config: Arc<DynamicConfig>,
}

impl EventsService {
    pub fn new(
        repository: Arc<dyn EventsServiceRepository>,
        email: EmailService,
        audit: AuditService,
        dynamic_config: Arc<DynamicConfig>,
    ) -> Self {
        Self { repository, email, audit, dynamic_config }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list(&self, query: &EventQuery) -> AppResult<(Vec<Event>, i64)> {
        self.repository.events_list(query).await
    }

    /// Load event metadata only (no `attachment_data_base64`). Used internally, e.g. announcement emails.
    pub async fn get_by_id(&self, id: i64) -> AppResult<Event> {
        self.repository.events_get_by_id(id).await
    }

    /// Load event including `attachment_data_base64` when an attachment exists (single-resource API).
    #[tracing::instrument(skip(self), err)]
    pub async fn get_by_id_with_attachment(&self, id: i64) -> AppResult<Event> {
        let event = self.repository.events_get_by_id(id).await?;
        self.enrich_with_attachment_base64(event).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn create(&self, data: &CreateEvent) -> AppResult<Event> {
        let attachment = match &data.attachment {
            Some(a) => Some(decode_event_attachment_input(a)?),
            None => None,
        };
        let event = self.repository.events_create(data, attachment).await?;
        self.enrich_with_attachment_base64(event).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn update(&self, id: i64, data: &UpdateEvent) -> AppResult<Event> {
        let remove = data.remove_attachment == Some(true);
        let new_attachment = if !remove {
            match &data.attachment {
                Some(a) => Some(decode_event_attachment_input(a)?),
                None => None,
            }
        } else {
            None
        };

        let mut event = self.repository.events_update(id, data).await?;

        event = if remove {
            self.repository.events_delete_attachment(id).await?
        } else if let Some((bytes, fname, mime)) = new_attachment {
            self.repository
                .events_put_attachment(id, &bytes, &fname, &mime)
                .await?
        } else {
            event
        };

        self.enrich_with_attachment_base64(event).await
    }

    async fn enrich_with_attachment_base64(&self, mut event: Event) -> AppResult<Event> {
        if event.attachment_size.unwrap_or(0) > 0 {
            if let Some((bytes, _, _)) = self.repository.events_get_attachment_blob(event.id).await? {
                event.attachment_data_base64 = Some(B64.encode(&bytes));
            }
        }
        Ok(event)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repository.events_delete(id).await
    }

    /// Get annual event statistics (for annual report)
    #[tracing::instrument(skip(self), err)]
    pub async fn annual_stats(&self, year: i32) -> AppResult<EventAnnualStats> {
        self.repository.events_annual_stats(year).await
    }

    /// Send an announcement email for an event to all users whose public_type
    /// matches the event's `target_public` (or everyone if `target_public` is NULL).
    ///
    /// If the request provides `subject`/`body_plain`/`body_html`, those are used
    /// directly instead of the template.
    #[tracing::instrument(skip(self), err)]
    pub async fn send_announcement(
        &self,
        event_id: i64,
        payload: &SendAnnouncementRequest,
        triggered_by: Option<i64>,
        client_ip: Option<String>,
    ) -> AppResult<AnnouncementReport> {
        let event = self.repository.events_get_by_id(event_id).await?;

        let templates_dir = self.dynamic_config.read_email().templates_dir.clone();

        let event_date = event.event_date.format("%d/%m/%Y").to_string();
        let event_type_label = match event.event_type {
            0 => "Animation",
            1 => "Visite scolaire / School visit",
            2 => "Exposition / Exhibition",
            3 => "Conférence / Conference",
            4 => "Atelier / Workshop",
            5 => "Spectacle / Show",
            _ => "Autre / Other",
        };

        let start_time_plain = event
            .start_time
            .map(|t| format!("\nHeure / Time: {}", t.format("%H:%M")))
            .unwrap_or_default();
        let start_time_row = event
            .start_time
            .map(|t| format!(
                "<tr><td style=\"padding:6px 12px;font-weight:bold;background:#f7f7f7;border:1px solid #e2e8f0\">Heure / Time</td>\
                 <td style=\"padding:6px 12px;border:1px solid #e2e8f0\">{}</td></tr>",
                t.format("%H:%M")
            ))
            .unwrap_or_default();

        let description_plain = event
            .description
            .as_deref()
            .map(|d| format!("\n{}", d))
            .unwrap_or_default();
        let description_block = event
            .description
            .as_deref()
            .map(|d| format!("<p>{}</p>", d.replace('\n', "<br>")))
            .unwrap_or_default();

        // Fetch target users
        let targets = self
            .repository
            .users_get_emails_by_public_type(event.target_public.map(|v| v as i64))
            .await?;

        let mut emails_sent: u32 = 0;
        let mut skipped: u32 = 0;
        let mut errors: Vec<AnnouncementError> = Vec::new();

        for user in &targets {
            let email_addr = match &user.email {
                Some(e) if !e.is_empty() => e.clone(),
                _ => {
                    skipped += 1;
                    continue;
                }
            };

            let firstname = user.firstname.as_deref().unwrap_or("");

            let (subject, body_plain, body_html) = if payload.subject.is_some()
                || payload.body_plain.is_some()
            {
                // Use caller-supplied content
                let subj = payload
                    .subject
                    .as_deref()
                    .unwrap_or(&event.name)
                    .to_string();
                let plain = payload
                    .body_plain
                    .as_deref()
                    .unwrap_or("")
                    .to_string();
                let html = payload.body_html.as_deref().map(|h| h.to_string()).unwrap_or_else(|| {
                    format!(
                        "<html><body><pre>{}</pre></body></html>",
                        plain.replace('\n', "<br>")
                    )
                });
                (subj, plain, html)
            } else {
                // Load template
                let lang = user.language.as_deref().map(Language::from);
                match email_templates::load_template(
                    Path::new(&templates_dir),
                    "event_announcement",
                    lang,
                ) {
                    Err(e) => {
                        errors.push(AnnouncementError {
                            user_id: user.id,
                            email: email_addr.clone(),
                            error_message: format!("Template load error: {}", e),
                        });
                        continue;
                    }
                    Ok(template) => {
                        let vars: Vec<(&str, &str)> = vec![
                            ("firstname", firstname),
                            ("event_name", &event.name),
                            ("event_date", &event_date),
                            ("event_type", event_type_label),
                            ("start_time_line", &start_time_plain),
                            ("start_time_row", &start_time_row),
                            ("description_line", &description_plain),
                            ("description_block", &description_block),
                        ];
                        let (s, p, h) = email_templates::substitute(&template, &vars);
                        (s, p, h)
                    }
                }
            };

            match self.email.send_email_with_html(&email_addr, &subject, &body_plain, &body_html).await {
                Ok(()) => {
                    emails_sent += 1;
                    self.audit.log(
                        audit::event::EVENT_ANNOUNCEMENT_SENT,
                        triggered_by,
                        Some("event"),
                        Some(event_id),
                        client_ip.clone(),
                        Some(serde_json::json!({
                            "user_id": user.id,
                            "email": email_addr,
                            "event_name": event.name,
                        })),
                    );
                }
                Err(e) => {
                    errors.push(AnnouncementError {
                        user_id: user.id,
                        email: email_addr,
                        error_message: e.to_string(),
                    });
                }
            }
        }

        if emails_sent > 0 {
            let _ = self.repository.events_set_announcement_sent_at(event_id).await;
        }

        Ok(AnnouncementReport { event_id, emails_sent, skipped, errors })
    }
}

fn decode_event_attachment_input(input: &EventAttachmentInput) -> AppResult<(Vec<u8>, String, String)> {
    let bytes = B64
        .decode(input.data_base64.trim())
        .map_err(|_| AppError::Validation("Invalid Base64 in attachment".to_string()))?;
    if bytes.is_empty() {
        return Err(AppError::Validation("Attachment payload is empty".to_string()));
    }
    if bytes.len() > MAX_EVENT_ATTACHMENT_BYTES {
        return Err(AppError::Validation(format!(
            "Attachment exceeds maximum size of {} bytes",
            MAX_EVENT_ATTACHMENT_BYTES
        )));
    }
    let fname = sanitize_attachment_filename(&input.file_name);
    let mime = normalize_mime_type(&input.mime_type);
    Ok((bytes, fname, mime))
}

fn sanitize_attachment_filename(name: &str) -> String {
    let base = Path::new(name.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment");
    let cleaned: String = base
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .take(200)
        .collect();
    let trimmed = cleaned.trim_matches('.');
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_mime_type(mime: &str) -> String {
    let s = mime.trim();
    if s.is_empty() {
        return "application/octet-stream".to_string();
    }
    s.chars().take(255).collect()
}
