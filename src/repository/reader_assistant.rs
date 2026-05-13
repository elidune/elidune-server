use async_trait::async_trait;
use snowflaked::Generator;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::reader_assistant::{AiMessage, AiRecommendationRow, AiSession, NewRecommendation},
};

#[async_trait]
pub trait ReaderAssistantRepository: Send + Sync {
    async fn ai_session_create(&self, user_id: i64, title: Option<&str>) -> AppResult<AiSession>;
    async fn ai_sessions_list(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<AiSession>, i64)>;
    async fn ai_session_get(&self, user_id: i64, session_id: i64) -> AppResult<AiSession>;
    async fn ai_session_soft_delete(&self, user_id: i64, session_id: i64) -> AppResult<()>;
    async fn ai_message_insert(
        &self,
        session_id: i64,
        role: &str,
        content_redacted: &str,
        provider: Option<&str>,
        model: Option<&str>,
        token_usage: Option<i32>,
        latency_ms: Option<i32>,
    ) -> AppResult<AiMessage>;
    async fn ai_messages_list(&self, user_id: i64, session_id: i64) -> AppResult<Vec<AiMessage>>;
    async fn ai_recommendations_insert(
        &self,
        message_id: i64,
        recommendations: &[NewRecommendation],
    ) -> AppResult<()>;
    async fn ai_recommendations_for_session(
        &self,
        user_id: i64,
        session_id: i64,
    ) -> AppResult<Vec<AiRecommendationRow>>;
    async fn ai_user_message_count_today(&self, user_id: i64) -> AppResult<i64>;
}

#[async_trait::async_trait]
impl ReaderAssistantRepository for Repository {
    async fn ai_session_create(&self, user_id: i64, title: Option<&str>) -> AppResult<AiSession> {
        Repository::ai_session_create(self, user_id, title).await
    }
    async fn ai_sessions_list(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<AiSession>, i64)> {
        Repository::ai_sessions_list(self, user_id, page, per_page).await
    }
    async fn ai_session_get(&self, user_id: i64, session_id: i64) -> AppResult<AiSession> {
        Repository::ai_session_get(self, user_id, session_id).await
    }
    async fn ai_session_soft_delete(&self, user_id: i64, session_id: i64) -> AppResult<()> {
        Repository::ai_session_soft_delete(self, user_id, session_id).await
    }
    async fn ai_message_insert(
        &self,
        session_id: i64,
        role: &str,
        content_redacted: &str,
        provider: Option<&str>,
        model: Option<&str>,
        token_usage: Option<i32>,
        latency_ms: Option<i32>,
    ) -> AppResult<AiMessage> {
        Repository::ai_message_insert(
            self,
            session_id,
            role,
            content_redacted,
            provider,
            model,
            token_usage,
            latency_ms,
        )
        .await
    }
    async fn ai_messages_list(&self, user_id: i64, session_id: i64) -> AppResult<Vec<AiMessage>> {
        Repository::ai_messages_list(self, user_id, session_id).await
    }
    async fn ai_recommendations_insert(
        &self,
        message_id: i64,
        recommendations: &[NewRecommendation],
    ) -> AppResult<()> {
        Repository::ai_recommendations_insert(self, message_id, recommendations).await
    }
    async fn ai_recommendations_for_session(
        &self,
        user_id: i64,
        session_id: i64,
    ) -> AppResult<Vec<AiRecommendationRow>> {
        Repository::ai_recommendations_for_session(self, user_id, session_id).await
    }
    async fn ai_user_message_count_today(&self, user_id: i64) -> AppResult<i64> {
        Repository::ai_user_message_count_today(self, user_id).await
    }
}

static SNOWFLAKE: std::sync::LazyLock<std::sync::Mutex<Generator>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Generator::new(2)));

fn next_id() -> i64 {
    SNOWFLAKE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .generate::<i64>()
}

impl Repository {
    #[tracing::instrument(skip(self), err)]
    pub async fn ai_session_create(&self, user_id: i64, title: Option<&str>) -> AppResult<AiSession> {
        let id = next_id();
        let row = sqlx::query_as::<_, AiSession>(
            r#"
            INSERT INTO ai_sessions (id, user_id, title)
            VALUES ($1, $2, $3)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_sessions_list(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<AiSession>, i64)> {
        let safe_page = page.max(1);
        let safe_per_page = per_page.clamp(1, 100);
        let offset = (safe_page - 1) * safe_per_page;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM ai_sessions WHERE user_id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        let rows = sqlx::query_as::<_, AiSession>(
            r#"
            SELECT * FROM ai_sessions
            WHERE user_id = $1 AND deleted_at IS NULL
            ORDER BY updated_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(safe_per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok((rows, total))
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_session_get(&self, user_id: i64, session_id: i64) -> AppResult<AiSession> {
        sqlx::query_as::<_, AiSession>(
            r#"
            SELECT * FROM ai_sessions
            WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Assistant session {session_id} not found")))
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_session_soft_delete(&self, user_id: i64, session_id: i64) -> AppResult<()> {
        let affected = sqlx::query(
            r#"
            UPDATE ai_sessions
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(format!(
                "Assistant session {session_id} not found"
            )));
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_message_insert(
        &self,
        session_id: i64,
        role: &str,
        content_redacted: &str,
        provider: Option<&str>,
        model: Option<&str>,
        token_usage: Option<i32>,
        latency_ms: Option<i32>,
    ) -> AppResult<AiMessage> {
        let id = next_id();
        let row = sqlx::query_as::<_, AiMessage>(
            r#"
            INSERT INTO ai_messages (
                id, session_id, role, content_redacted,
                provider, model, token_usage, latency_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(session_id)
        .bind(role)
        .bind(content_redacted)
        .bind(provider)
        .bind(model)
        .bind(token_usage)
        .bind(latency_ms)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query("UPDATE ai_sessions SET updated_at = NOW() WHERE id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(row)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_messages_list(&self, user_id: i64, session_id: i64) -> AppResult<Vec<AiMessage>> {
        let _ = self.ai_session_get(user_id, session_id).await?;
        let rows = sqlx::query_as::<_, AiMessage>(
            r#"
            SELECT m.*
            FROM ai_messages m
            JOIN ai_sessions s ON s.id = m.session_id
            WHERE m.session_id = $1 AND s.user_id = $2 AND s.deleted_at IS NULL
            ORDER BY m.created_at ASC
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    #[tracing::instrument(skip(self, recommendations), err)]
    pub async fn ai_recommendations_insert(
        &self,
        message_id: i64,
        recommendations: &[NewRecommendation],
    ) -> AppResult<()> {
        for rec in recommendations {
            let id = next_id();
            sqlx::query(
                r#"
                INSERT INTO ai_recommendations (
                    id, message_id, kind, biblio_id, external_ref, score, rationale
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(id)
            .bind(message_id)
            .bind(rec.kind.as_db_str())
            .bind(rec.biblio_id)
            .bind(rec.external_ref.clone())
            .bind(rec.score)
            .bind(&rec.rationale)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_recommendations_for_session(
        &self,
        user_id: i64,
        session_id: i64,
    ) -> AppResult<Vec<AiRecommendationRow>> {
        let _ = self.ai_session_get(user_id, session_id).await?;
        let rows = sqlx::query_as::<_, AiRecommendationRow>(
            r#"
            SELECT r.*
            FROM ai_recommendations r
            JOIN ai_messages m ON m.id = r.message_id
            JOIN ai_sessions s ON s.id = m.session_id
            WHERE s.id = $1 AND s.user_id = $2 AND s.deleted_at IS NULL
            ORDER BY r.created_at ASC
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn ai_user_message_count_today(&self, user_id: i64) -> AppResult<i64> {
        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::bigint
            FROM ai_messages m
            JOIN ai_sessions s ON s.id = m.session_id
            WHERE s.user_id = $1
              AND m.role = 'user'
              AND m.created_at >= date_trunc('day', NOW())
              AND s.deleted_at IS NULL
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(total)
    }
}
