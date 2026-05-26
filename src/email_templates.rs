//! Email template loading and variable substitution.
//!
//! Templates are stored in the `email_templates` DB table (editable via
//! `/settings/email-templates` API). The on-disk JSON files under
//! [`crate::config::EmailConfig::templates_dir`] act as a packaged fallback
//! and as the source for the one-time bootstrap performed at startup.

use std::path::{Path, PathBuf};

use sqlx::{Pool, Postgres};

use crate::{
    error::{AppError, AppResult},
    models::Language,
    repository::Repository,
};

#[derive(Debug, Clone)]
pub struct EmailTemplate {
    pub subject: String,
    pub body_plain: String,
    pub body_html: Option<String>,
}

/// Canonical list of template ids the server expects on disk / in DB.
pub const KNOWN_TEMPLATE_IDS: &[&str] = &[
    "2fa_code",
    "recovery_code",
    "password_reset",
    "hold_ready",
    "inventory_loan_closed",
    "overdue_reminder",
    "event_announcement",
];

/// Languages bootstrapped / accepted by the API.
pub const SUPPORTED_LANGUAGES: &[&str] = &["english", "french"];

/// Build the language fallback chain for resolution: requested → french → english.
fn language_chain(lang: Option<Language>) -> Vec<&'static str> {
    match lang {
        Some(l) => match l.as_db_str() {
            "french" => vec!["french", "english"],
            "english" => vec!["english", "french"],
            _ => vec!["french", "english"],
        },
        None => vec!["french", "english"],
    }
}

/// Load template for given id and language from the DB, with cascade `lang → french → english`.
/// Falls back to the on-disk JSON file (same cascade) if no DB row matches.
pub async fn load_template_async(
    pool: &Pool<Postgres>,
    templates_dir: &Path,
    template_id: &str,
    lang: Option<Language>,
) -> AppResult<EmailTemplate> {
    let candidates = language_chain(lang);

    for lang_key in &candidates {
        if let Some(tpl) = load_from_db(pool, template_id, lang_key).await? {
            return Ok(tpl);
        }
    }

    for lang_key in &candidates {
        if let Some(tpl) = load_from_file(templates_dir, template_id, lang_key)? {
            return Ok(tpl);
        }
    }

    Err(AppError::Internal(format!(
        "No template found for {} (tried languages: {})",
        template_id,
        candidates.join(", ")
    )))
}

/// Synchronous on-disk loader kept for callers that do not (yet) own a DB pool.
/// Newer code should prefer [`load_template_async`].
pub fn load_template(
    templates_dir: &Path,
    template_id: &str,
    lang: Option<Language>,
) -> AppResult<EmailTemplate> {
    let candidates = language_chain(lang);

    for lang_key in &candidates {
        if let Some(tpl) = load_from_file(templates_dir, template_id, lang_key)? {
            return Ok(tpl);
        }
    }

    Err(AppError::Internal(format!(
        "No template found for {} (tried languages: {})",
        template_id,
        candidates.join(", ")
    )))
}

async fn load_from_db(
    pool: &Pool<Postgres>,
    template_id: &str,
    language: &str,
) -> AppResult<Option<EmailTemplate>> {
    let repo = Repository::new(pool.clone(), None, None);
    let row = repo.email_templates_get(template_id, language).await?;
    Ok(row.map(|r| EmailTemplate {
        subject: r.subject,
        body_plain: r.body_plain,
        body_html: r.body_html,
    }))
}

fn load_from_file(
    templates_dir: &Path,
    template_id: &str,
    language: &str,
) -> AppResult<Option<EmailTemplate>> {
    let path: PathBuf = templates_dir.join(format!("{}.{}.json", template_id, language));
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Internal(format!("Failed to read template {:?}: {}", path, e)))?;
    let raw: RawTemplate = serde_json::from_str(&content)
        .map_err(|e| AppError::Internal(format!("Invalid template {:?}: {}", path, e)))?;
    Ok(Some(EmailTemplate {
        subject: raw.subject,
        body_plain: raw.body_plain,
        body_html: raw.body_html,
    }))
}

/// Substitute {{var}} placeholders in subject and body.
/// Returns (subject, body_plain, body_html). HTML is generated from plain if not in template.
pub fn substitute(template: &EmailTemplate, vars: &[(&str, &str)]) -> (String, String, String) {
    let subj = substitute_str(&template.subject, vars);
    let plain = substitute_str(&template.body_plain, vars);
    let html = template
        .body_html
        .as_ref()
        .map(|h| substitute_str(h, vars))
        .unwrap_or_else(|| format!("<html><body><pre>{}</pre></body></html>", plain.replace('\n', "<br>")));
    (subj, plain, html)
}

fn substitute_str(s: &str, vars: &[(&str, &str)]) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        let placeholder = format!("{{{{{}}}}}", k);
        out = out.replace(&placeholder, v);
    }
    out
}

/// Seed the `email_templates` table from the on-disk JSON files when it is empty.
///
/// This keeps `data/email_templates/*.json` as the authoritative initial source for new
/// installations while letting administrators edit the rows afterwards via the API.
/// Subsequent restarts are no-ops because the count check short-circuits.
pub async fn bootstrap_from_files(
    pool: &Pool<Postgres>,
    templates_dir: &Path,
) -> AppResult<usize> {
    let repo = Repository::new(pool.clone(), None, None);

    if repo.email_templates_count().await? > 0 {
        return Ok(0);
    }

    let mut inserted = 0usize;
    for template_id in KNOWN_TEMPLATE_IDS {
        for language in SUPPORTED_LANGUAGES {
            match load_from_file(templates_dir, template_id, language)? {
                Some(tpl) => {
                    repo.email_templates_upsert(
                        template_id,
                        language,
                        &tpl.subject,
                        &tpl.body_plain,
                        tpl.body_html.as_deref(),
                    )
                    .await?;
                    inserted += 1;
                }
                None => {
                    tracing::debug!(
                        "Email template bootstrap: no file for {}.{}.json — skipped",
                        template_id,
                        language
                    );
                }
            }
        }
    }

    if inserted == 0 {
        tracing::warn!(
            "Email template bootstrap: no JSON files found under {:?}; the table stays empty",
            templates_dir
        );
    } else {
        tracing::info!(
            "Email template bootstrap: seeded {} rows from {:?}",
            inserted,
            templates_dir
        );
    }

    Ok(inserted)
}

#[derive(serde::Deserialize)]
struct RawTemplate {
    subject: String,
    body_plain: String,
    #[serde(default)]
    body_html: Option<String>,
}
