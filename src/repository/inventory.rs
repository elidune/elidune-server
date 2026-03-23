//! Inventory / stocktaking domain methods on Repository

use async_trait::async_trait;
use snowflaked::Generator;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::inventory::{InventoryReport, InventorySession, InventoryScan, InventoryStatus},
};

#[async_trait]
pub trait InventoryRepository: Send + Sync {
    async fn inventory_list_sessions(&self) -> AppResult<Vec<InventorySession>>;
    async fn inventory_get_session(&self, id: i64) -> AppResult<InventorySession>;
    async fn inventory_create_session(
        &self,
        name: &str,
        location_filter: Option<&str>,
        notes: Option<&str>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession>;
    async fn inventory_close_session(&self, id: i64) -> AppResult<InventorySession>;
    async fn inventory_scan_barcode(
        &self,
        session_id: i64,
        barcode: &str,
    ) -> AppResult<InventoryScan>;
    async fn inventory_list_scans(&self, session_id: i64) -> AppResult<Vec<InventoryScan>>;
    async fn inventory_report(&self, session_id: i64) -> AppResult<InventoryReport>;
}


#[async_trait::async_trait]
impl InventoryRepository for Repository {
    async fn inventory_list_sessions(&self) -> AppResult<Vec<InventorySession>> {
        Repository::inventory_list_sessions(self).await
    }
    async fn inventory_get_session(&self, id: i64) -> AppResult<InventorySession> {
        Repository::inventory_get_session(self, id).await
    }
    async fn inventory_create_session(
        &self, name: &str, location_filter: Option<&str>, notes: Option<&str>, created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        Repository::inventory_create_session(self, name, location_filter, notes, created_by).await
    }
    async fn inventory_close_session(&self, id: i64) -> AppResult<InventorySession> {
        Repository::inventory_close_session(self, id).await
    }
    async fn inventory_scan_barcode(
        &self, session_id: i64, barcode: &str,
    ) -> AppResult<InventoryScan> {
        Repository::inventory_scan_barcode(self, session_id, barcode).await
    }
    async fn inventory_list_scans(&self, session_id: i64) -> AppResult<Vec<InventoryScan>> {
        Repository::inventory_list_scans(self, session_id).await
    }
    async fn inventory_report(&self, session_id: i64) -> AppResult<InventoryReport> {
        Repository::inventory_report(self, session_id).await
    }
}


static SNOWFLAKE: std::sync::LazyLock<std::sync::Mutex<Generator>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Generator::new(3)));

fn next_id() -> i64 {
    SNOWFLAKE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .generate::<i64>()
}

impl Repository {
    /// List all inventory sessions
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_sessions(&self) -> AppResult<Vec<InventorySession>> {
        let rows = sqlx::query_as::<_, InventorySession>(
            "SELECT * FROM inventory_sessions ORDER BY started_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get session by ID
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_get_session(&self, id: i64) -> AppResult<InventorySession> {
        sqlx::query_as::<_, InventorySession>(
            "SELECT * FROM inventory_sessions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Inventory session {id} not found")))
    }

    /// Create a new inventory session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_create_session(
        &self,
        name: &str,
        location_filter: Option<&str>,
        notes: Option<&str>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        let id = next_id();
        let row = sqlx::query_as::<_, InventorySession>(
            r#"
            INSERT INTO inventory_sessions (id, name, location_filter, notes, created_by)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(location_filter)
        .bind(notes)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Close an inventory session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_close_session(&self, id: i64) -> AppResult<InventorySession> {
        sqlx::query_as::<_, InventorySession>(
            "UPDATE inventory_sessions SET status = 'closed', closed_at = NOW()
             WHERE id = $1 AND status = 'open' RETURNING *",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Open session {id} not found")))
    }

    /// Record a barcode scan in a session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_scan_barcode(
        &self,
        session_id: i64,
        barcode: &str,
    ) -> AppResult<InventoryScan> {
        // Lookup physical item by barcode
        let item_id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM items WHERE barcode = $1 LIMIT 1")
                .bind(barcode)
                .fetch_optional(&self.pool)
                .await?;

        let result = if item_id.is_some() { "found" } else { "unknown_barcode" };

        let row = sqlx::query_as::<_, InventoryScan>(
            r#"
            INSERT INTO inventory_scans (session_id, item_id, barcode, result)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(session_id)
        .bind(item_id)
        .bind(barcode)
        .bind(result)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Get scans for a session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_scans(
        &self,
        session_id: i64,
    ) -> AppResult<Vec<InventoryScan>> {
        let rows = sqlx::query_as::<_, InventoryScan>(
            "SELECT * FROM inventory_scans WHERE session_id = $1 ORDER BY scanned_at",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Generate a discrepancy report for a session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_report(&self, session_id: i64) -> AppResult<InventoryReport> {
        let total_scanned: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM inventory_scans WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let total_found: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM inventory_scans WHERE session_id = $1 AND result = 'found'",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let total_unknown = total_scanned - total_found;

        // Items (physical copies) not scanned = all active items minus scanned ones
        let missing_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM items s
            WHERE s.archived_at IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1 AND sc.item_id = s.id
              )
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(InventoryReport {
            session_id,
            total_scanned,
            total_found,
            total_unknown,
            missing_count,
        })
    }
}

