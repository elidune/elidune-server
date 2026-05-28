//! Persisted, runtime-editable email templates (`email_templates` table).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

use super::Repository;
use crate::error::AppResult;

/// One row of the `email_templates` table.
#[derive(Debug, Clone)]
pub struct EmailTemplateRow {
    pub template_id: String,
    pub language: String,
    pub name: String,
    pub subject: String,
    pub body_plain: String,
    pub body_html: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// DB access for editable email templates. Implemented by [`Repository`].
#[async_trait]
pub trait EmailTemplatesRepository: Send + Sync {
    async fn email_templates_count(&self) -> AppResult<i64>;
    async fn email_templates_list(&self) -> AppResult<Vec<EmailTemplateRow>>;
    async fn email_templates_get(
        &self,
        template_id: &str,
        language: &str,
    ) -> AppResult<Option<EmailTemplateRow>>;
    async fn email_templates_upsert(
        &self,
        template_id: &str,
        language: &str,
        name: &str,
        subject: &str,
        body_plain: &str,
        body_html: Option<&str>,
    ) -> AppResult<EmailTemplateRow>;
    async fn email_templates_update_content(
        &self,
        template_id: &str,
        language: &str,
        subject: &str,
        body_plain: &str,
        body_html: Option<&str>,
    ) -> AppResult<EmailTemplateRow>;
}

#[async_trait]
impl EmailTemplatesRepository for Repository {
    async fn email_templates_count(&self) -> AppResult<i64> {
        Repository::email_templates_count(self).await
    }

    async fn email_templates_list(&self) -> AppResult<Vec<EmailTemplateRow>> {
        Repository::email_templates_list(self).await
    }

    async fn email_templates_get(
        &self,
        template_id: &str,
        language: &str,
    ) -> AppResult<Option<EmailTemplateRow>> {
        Repository::email_templates_get(self, template_id, language).await
    }

    async fn email_templates_upsert(
        &self,
        template_id: &str,
        language: &str,
        name: &str,
        subject: &str,
        body_plain: &str,
        body_html: Option<&str>,
    ) -> AppResult<EmailTemplateRow> {
        Repository::email_templates_upsert(
            self,
            template_id,
            language,
            name,
            subject,
            body_plain,
            body_html,
        )
        .await
    }

    async fn email_templates_update_content(
        &self,
        template_id: &str,
        language: &str,
        subject: &str,
        body_plain: &str,
        body_html: Option<&str>,
    ) -> AppResult<EmailTemplateRow> {
        Repository::email_templates_update_content(
            self,
            template_id,
            language,
            subject,
            body_plain,
            body_html,
        )
        .await
    }
}

impl Repository {
    /// Number of rows in `email_templates`. Used at startup to decide whether to bootstrap from files.
    pub async fn email_templates_count(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_templates")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// All rows ordered by `(template_id, language)`.
    pub async fn email_templates_list(&self) -> AppResult<Vec<EmailTemplateRow>> {
        let rows = sqlx::query(
            r#"
            SELECT template_id, language, name, subject, body_plain, body_html, updated_at
            FROM email_templates
            ORDER BY template_id, language
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EmailTemplateRow {
                template_id: r.get("template_id"),
                language: r.get("language"),
                name: r.get("name"),
                subject: r.get("subject"),
                body_plain: r.get("body_plain"),
                body_html: r.get("body_html"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    /// Single template by `(template_id, language)`.
    pub async fn email_templates_get(
        &self,
        template_id: &str,
        language: &str,
    ) -> AppResult<Option<EmailTemplateRow>> {
        let row = sqlx::query(
            r#"
            SELECT template_id, language, name, subject, body_plain, body_html, updated_at
            FROM email_templates
            WHERE template_id = $1 AND language = $2
            "#,
        )
        .bind(template_id)
        .bind(language)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| EmailTemplateRow {
            template_id: r.get("template_id"),
            language: r.get("language"),
            name: r.get("name"),
            subject: r.get("subject"),
            body_plain: r.get("body_plain"),
            body_html: r.get("body_html"),
            updated_at: r.get("updated_at"),
        }))
    }

    /// Insert a template row from packaged JSON (bootstrap). Existing rows are left unchanged.
    pub async fn email_templates_upsert(
        &self,
        template_id: &str,
        language: &str,
        name: &str,
        subject: &str,
        body_plain: &str,
        body_html: Option<&str>,
    ) -> AppResult<EmailTemplateRow> {
        let row = sqlx::query(
            r#"
            INSERT INTO email_templates (template_id, language, name, subject, body_plain, body_html, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            ON CONFLICT (template_id, language) DO UPDATE SET
                subject    = EXCLUDED.subject,
                body_plain = EXCLUDED.body_plain,
                body_html  = EXCLUDED.body_html,
                updated_at = NOW()
            RETURNING template_id, language, name, subject, body_plain, body_html, updated_at
            "#,
        )
        .bind(template_id)
        .bind(language)
        .bind(name)
        .bind(subject)
        .bind(body_plain)
        .bind(body_html)
        .fetch_one(&self.pool)
        .await?;

        Ok(EmailTemplateRow {
            template_id: row.get("template_id"),
            language: row.get("language"),
            name: row.get("name"),
            subject: row.get("subject"),
            body_plain: row.get("body_plain"),
            body_html: row.get("body_html"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Update editable content only; display `name` is preserved.
    pub async fn email_templates_update_content(
        &self,
        template_id: &str,
        language: &str,
        subject: &str,
        body_plain: &str,
        body_html: Option<&str>,
    ) -> AppResult<EmailTemplateRow> {
        let row = sqlx::query(
            r#"
            UPDATE email_templates
            SET subject = $3, body_plain = $4, body_html = $5, updated_at = NOW()
            WHERE template_id = $1 AND language = $2
            RETURNING template_id, language, name, subject, body_plain, body_html, updated_at
            "#,
        )
        .bind(template_id)
        .bind(language)
        .bind(subject)
        .bind(body_plain)
        .bind(body_html)
        .fetch_one(&self.pool)
        .await?;

        Ok(EmailTemplateRow {
            template_id: row.get("template_id"),
            language: row.get("language"),
            name: row.get("name"),
            subject: row.get("subject"),
            body_plain: row.get("body_plain"),
            body_html: row.get("body_html"),
            updated_at: row.get("updated_at"),
        })
    }
}
