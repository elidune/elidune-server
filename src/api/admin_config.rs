//! Admin configuration API.
//!
//! Allows admins to read, update, and reset overridable config sections at runtime.
//! Changes are persisted to the `settings` DB table and applied immediately in memory.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    services::audit,
    AppState,
};

use super::{AuthenticatedUser, ClientIp};

/// A single config section with its current value and override status
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ConfigSectionInfo {
    /// Section key (e.g. "email", "logging", "reminders", "audit")
    pub key: String,
    /// Current effective value (merged file + DB override)
    pub value: Value,
    /// Whether this section is currently overridden in the DB
    pub overridden: bool,
    /// Whether this section is allowed to be overridden (from file config)
    pub overridable: bool,
}

/// Response for GET /admin/config
#[derive(Serialize, ToSchema)]
pub struct ConfigResponse {
    pub sections: Vec<ConfigSectionInfo>,
}

/// Request body for PUT /admin/config/:section
#[derive(Deserialize, ToSchema)]
pub struct UpdateConfigSectionRequest {
    /// The new JSON value for the section
    pub value: Value,
}

/// Request body for POST /admin/config/email/test
#[derive(Deserialize, ToSchema)]
pub struct TestEmailRequest {
    /// Recipient email address for the test
    pub to: String,
}

/// Get all overridable config sections (admin only)
#[utoipa::path(
    get,
    path = "/admin/config",
    tag = "admin",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "All config sections", body = ConfigResponse),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn get_config(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<ConfigResponse>> {
    claims.require_write_settings()?;

    let pool = state.services.repository_pool();

    // Get which sections are currently overridden in DB
    let overridden_keys: Vec<String> = sqlx::query_scalar("SELECT key FROM settings")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let dynamic = &state.dynamic_config;
    let mut sections = Vec::new();

    for key in dynamic.overridable_sections() {
        let value = dynamic.get_section_value(key)?;
        // Mask sensitive fields for output
        let value = audit::mask_sensitive_fields(value);
        sections.push(ConfigSectionInfo {
            key: key.to_string(),
            value,
            overridden: overridden_keys.contains(&key.to_string()),
            overridable: true,
        });
    }

    Ok(Json(ConfigResponse { sections }))
}

/// Update a config section (admin only). Validates, persists to DB, applies immediately.
#[utoipa::path(
    put,
    path = "/admin/config/{section}",
    tag = "admin",
    security(("bearer_auth" = [])),
    request_body = UpdateConfigSectionRequest,
    params(
        ("section" = String, Path, description = "Config section key: email | logging | reminders | audit")
    ),
    responses(
        (status = 200, description = "Updated config section", body = ConfigSectionInfo),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Unknown section")
    )
)]
pub async fn update_config_section(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(section): Path<String>,
    Json(body): Json<UpdateConfigSectionRequest>,
) -> AppResult<Json<ConfigSectionInfo>> {
    claims.require_write_settings()?;

    let dynamic = &state.dynamic_config;

    // Capture old value before update (masked)
    let old_value = dynamic
        .get_section_value(&section)
        .ok()
        .map(audit::mask_sensitive_fields);

    // Validate and apply in memory
    dynamic.update_section(&section, body.value.clone())?;

    // Persist to DB
    let pool = state.services.repository_pool();
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(&section)
    .bind(&body.value)
    .execute(pool)
    .await?;

    // Audit
    let new_value_masked = dynamic
        .get_section_value(&section)
        .ok()
        .map(audit::mask_sensitive_fields);

    state.services.audit.log(
        audit::event::CONFIG_SECTION_UPDATED,
        Some(claims.user_id),
        Some("config"),
        None,
        ip,
        Some(serde_json::json!({
            "section": section,
            "old_value": old_value,
            "new_value": new_value_masked,
        })),
    );

    // Wake the reminder scheduler if the reminders config changed
    if section == "reminders" {
        state.scheduler_notify.notify_one();
    }

    let value = dynamic.get_section_value(&section)?;
    let value = audit::mask_sensitive_fields(value);

    Ok(Json(ConfigSectionInfo {
        key: section,
        value,
        overridden: true,
        overridable: true,
    }))
}

/// Reset a config section to the file default (admin only). Removes DB override.
#[utoipa::path(
    delete,
    path = "/admin/config/{section}",
    tag = "admin",
    security(("bearer_auth" = [])),
    params(
        ("section" = String, Path, description = "Config section key to reset")
    ),
    responses(
        (status = 200, description = "Section reset to file default", body = ConfigSectionInfo),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Unknown section")
    )
)]
pub async fn reset_config_section(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(section): Path<String>,
) -> AppResult<Json<ConfigSectionInfo>> {
    claims.require_write_settings()?;

    let dynamic = &state.dynamic_config;
    dynamic.reset_section(&section)?;

    // Remove from DB
    let pool = state.services.repository_pool();
    sqlx::query("DELETE FROM settings WHERE key = $1")
        .bind(&section)
        .execute(pool)
        .await?;

    state.services.audit.log(
        audit::event::CONFIG_SECTION_RESET,
        Some(claims.user_id),
        Some("config"),
        None,
        ip,
        Some(serde_json::json!({ "section": section })),
    );

    // Wake the reminder scheduler if the reminders config was reset
    if section == "reminders" {
        state.scheduler_notify.notify_one();
    }

    let value = dynamic.get_section_value(&section)?;
    let value = audit::mask_sensitive_fields(value);

    Ok(Json(ConfigSectionInfo {
        key: section,
        value,
        overridden: false,
        overridable: true,
    }))
}

/// Send a test email using the current live SMTP config (admin only)
#[utoipa::path(
    post,
    path = "/admin/config/email/test",
    tag = "admin",
    security(("bearer_auth" = [])),
    request_body = TestEmailRequest,
    responses(
        (status = 200, description = "Test email sent"),
        (status = 400, description = "Invalid request or SMTP error"),
        (status = 403, description = "Insufficient permissions")
    )
)]
pub async fn test_email(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(body): Json<TestEmailRequest>,
) -> AppResult<StatusCode> {
    claims.require_write_settings()?;

    state.services.email.send_test_email(&body.to).await?;

    state.services.audit.log(
        audit::event::EMAIL_TEST_SENT,
        Some(claims.user_id),
        None,
        None,
        ip,
        Some(serde_json::json!({ "to": body.to })),
    );

    Ok(StatusCode::OK)
}

/// Trigger a full reindex of the catalog into Meilisearch.
///
/// Fetches all items from PostgreSQL and pushes them to Meilisearch in batches.
/// Useful after initial setup, bulk imports, or if the index gets out of sync.
/// Returns immediately with the number of items queued; Meilisearch processes them asynchronously.
#[utoipa::path(
    post,
    path = "/api/v1/admin/reindex-search",
    tag = "admin",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Reindex queued", body = ReindexSearchResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 503, description = "Meilisearch not configured")
    )
)]
pub async fn reindex_search(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
) -> AppResult<Json<ReindexSearchResponse>> {
    claims.require_write_settings()?;

    let (count, available) = state.services.catalog.reindex_search().await?;

    state.services.audit.log(
        "admin.reindex_search",
        Some(claims.user_id),
        None,
        None,
        ip,
        Some(serde_json::json!({ "items_queued": count, "meilisearch_available": available })),
    );

    Ok(Json(ReindexSearchResponse { items_queued: count, meilisearch_available: available }))
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ReindexSearchResponse {
    pub items_queued: usize,
    pub meilisearch_available: bool,
}
