//! Settings → Email templates: list / get / upsert handlers.
//!
//! Templates are stored in the `email_templates` DB table. Read access requires the
//! "settings:read" right, write access requires "settings:write" — same model as the
//! library info and loan-rules screens.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    email_templates::{KNOWN_TEMPLATE_IDS, SUPPORTED_LANGUAGES},
    error::{AppError, AppResult},
    repository::{EmailTemplateRow, EmailTemplatesRepository},
    services::audit,
    AppState,
};

use super::{AuthenticatedUser, ClientIp};

/// Build the `/settings/email-templates*` routes (staff only).
pub fn router() -> axum::Router<AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/settings/email-templates", get(list_email_templates))
        .route(
            "/settings/email-templates/:template_id/:language",
            get(get_email_template).put(update_email_template),
        )
}

/// Persisted email template (one (templateId, language) pair).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailTemplate {
    pub template_id: String,
    pub language: String,
    pub subject: String,
    pub body_plain: String,
    pub body_html: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl From<EmailTemplateRow> for EmailTemplate {
    fn from(r: EmailTemplateRow) -> Self {
        Self {
            template_id: r.template_id,
            language: r.language,
            subject: r.subject,
            body_plain: r.body_plain,
            body_html: r.body_html,
            updated_at: r.updated_at,
        }
    }
}

/// Body for `PUT /settings/email-templates/{templateId}/{language}`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEmailTemplateRequest {
    pub subject: String,
    pub body_plain: String,
    /// Optional HTML body. When absent or empty, a plain-text-derived fallback is used at send time.
    pub body_html: Option<String>,
}

/// List all email templates (all (templateId, language) pairs).
#[utoipa::path(
    get,
    path = "/settings/email-templates",
    tag = "email_templates",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "All email templates", body = Vec<EmailTemplate>),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn list_email_templates(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<Vec<EmailTemplate>>> {
    claims.require_read_settings()?;
    let rows = state
        .services
        .minimal_repository()
        .email_templates_list()
        .await?;
    Ok(Json(rows.into_iter().map(EmailTemplate::from).collect()))
}

/// Get a single email template by id and language.
#[utoipa::path(
    get,
    path = "/settings/email-templates/{templateId}/{language}",
    tag = "email_templates",
    security(("bearer_auth" = [])),
    params(
        ("templateId" = String, Path, description = "Template id (e.g. password_reset)"),
        ("language" = String, Path, description = "Language: english | french")
    ),
    responses(
        (status = 200, description = "Email template", body = EmailTemplate),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Not found")
    )
)]
pub async fn get_email_template(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path((template_id, language)): Path<(String, String)>,
) -> AppResult<Json<EmailTemplate>> {
    claims.require_read_settings()?;
    validate_identifiers(&template_id, &language)?;

    let row = state
        .services
        .minimal_repository()
        .email_templates_get(&template_id, &language)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Email template not found: {}/{}",
                template_id, language
            ))
        })?;

    Ok(Json(row.into()))
}

/// Insert or update a template (`UPSERT` on `(templateId, language)`).
#[utoipa::path(
    put,
    path = "/settings/email-templates/{templateId}/{language}",
    tag = "email_templates",
    security(("bearer_auth" = [])),
    params(
        ("templateId" = String, Path, description = "Template id"),
        ("language" = String, Path, description = "Language: english | french")
    ),
    request_body = UpdateEmailTemplateRequest,
    responses(
        (status = 200, description = "Updated email template", body = EmailTemplate),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn update_email_template(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path((template_id, language)): Path<(String, String)>,
    Json(body): Json<UpdateEmailTemplateRequest>,
) -> AppResult<Json<EmailTemplate>> {
    claims.require_write_settings()?;
    validate_identifiers(&template_id, &language)?;

    let subject = body.subject.trim();
    if subject.is_empty() {
        return Err(AppError::Validation("subject must not be empty".into()));
    }
    if body.body_plain.trim().is_empty() {
        return Err(AppError::Validation("bodyPlain must not be empty".into()));
    }
    let body_html = body
        .body_html
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let row = state
        .services
        .minimal_repository()
        .email_templates_upsert(&template_id, &language, subject, &body.body_plain, body_html)
        .await?;

    state.services.audit.log(
        audit::event::EMAIL_TEMPLATE_UPDATED,
        Some(claims.user_id),
        Some("email_template"),
        None,
        ip,
        Some(serde_json::json!({
            "templateId": row.template_id,
            "language": row.language,
        })),
    );

    Ok(Json(row.into()))
}

fn validate_identifiers(template_id: &str, language: &str) -> AppResult<()> {
    if !KNOWN_TEMPLATE_IDS.contains(&template_id) {
        return Err(AppError::Validation(format!(
            "Unknown templateId '{}'. Allowed: {}",
            template_id,
            KNOWN_TEMPLATE_IDS.join(", ")
        )));
    }
    if !SUPPORTED_LANGUAGES.contains(&language) {
        return Err(AppError::Validation(format!(
            "Unsupported language '{}'. Allowed: {}",
            language,
            SUPPORTED_LANGUAGES.join(", ")
        )));
    }
    Ok(())
}
