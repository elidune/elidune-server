//! Runtime application settings persisted in the `settings` table (JSON overrides for dynamic config).

use async_trait::async_trait;

use super::Repository;
use crate::error::AppResult;

/// DB access for the `settings` table (runtime overrides). Implemented by [`Repository`].
#[async_trait]
pub trait RuntimeSettingsRepository: Send + Sync {
    async fn settings_load_overrides(&self) -> AppResult<Vec<(String, serde_json::Value)>>;
    async fn settings_count(&self) -> AppResult<i64>;
    async fn settings_list_keys(&self) -> AppResult<Vec<String>>;
    async fn settings_upsert_section(
        &self,
        key: &str,
        value: &serde_json::Value,
    ) -> AppResult<()>;
    async fn settings_delete_key(&self, key: &str) -> AppResult<()>;
}

#[async_trait]
impl RuntimeSettingsRepository for Repository {
    async fn settings_load_overrides(&self) -> AppResult<Vec<(String, serde_json::Value)>> {
        Repository::settings_load_overrides(self).await
    }

    async fn settings_count(&self) -> AppResult<i64> {
        Repository::settings_count(self).await
    }

    async fn settings_list_keys(&self) -> AppResult<Vec<String>> {
        Repository::settings_list_keys(self).await
    }

    async fn settings_upsert_section(
        &self,
        key: &str,
        value: &serde_json::Value,
    ) -> AppResult<()> {
        Repository::settings_upsert_section(self, key, value).await
    }

    async fn settings_delete_key(&self, key: &str) -> AppResult<()> {
        Repository::settings_delete_key(self, key).await
    }
}

impl Repository {
    /// Load all key/value rows from `settings` for merging into file config at startup.
    pub async fn settings_load_overrides(&self) -> AppResult<Vec<(String, serde_json::Value)>> {
        let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT key, value FROM settings",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Number of rows in `settings` (runtime config overrides).
    pub async fn settings_count(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Keys currently present in `settings` (overridden sections).
    pub async fn settings_list_keys(&self) -> AppResult<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>("SELECT key FROM settings")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Upsert a JSON section in `settings`.
    pub async fn settings_upsert_section(
        &self,
        key: &str,
        value: &serde_json::Value,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Remove a section override from `settings`.
    pub async fn settings_delete_key(&self, key: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM settings WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
