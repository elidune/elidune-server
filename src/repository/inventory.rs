//! Inventory / stocktaking domain methods on Repository

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use snowflaked::Generator;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::inventory::{
        InventoryMissingRow, InventoryReport, InventoryScan, InventoryScanResult, InventorySession,
        InventoryStatus,
    },
};

#[async_trait]
pub trait InventoryRepository: Send + Sync {
    async fn inventory_list_sessions_page(
        &self,
        page: i64,
        per_page: i64,
        status: Option<InventoryStatus>,
    ) -> AppResult<(Vec<InventorySession>, i64)>;
    async fn inventory_get_session(&self, id: i64) -> AppResult<InventorySession>;
    async fn inventory_create_session(
        &self,
        name: &str,
        location_filter: Option<&str>,
        notes: Option<&str>,
        scope_place: Option<i16>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession>;
    async fn inventory_close_session(&self, id: i64) -> AppResult<InventorySession>;
    async fn inventory_scan_barcode(
        &self,
        session_id: i64,
        barcode: &str,
        scanned_by: Option<i64>,
    ) -> AppResult<InventoryScan>;
    async fn inventory_list_scans_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryScan>, i64)>;
    async fn inventory_list_missing_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryMissingRow>, i64)>;
    async fn inventory_report(&self, session_id: i64) -> AppResult<InventoryReport>;
}

#[async_trait]
impl InventoryRepository for Repository {
    async fn inventory_list_sessions_page(
        &self,
        page: i64,
        per_page: i64,
        status: Option<InventoryStatus>,
    ) -> AppResult<(Vec<InventorySession>, i64)> {
        Repository::inventory_list_sessions_page(self, page, per_page, status).await
    }
    async fn inventory_get_session(&self, id: i64) -> AppResult<InventorySession> {
        Repository::inventory_get_session(self, id).await
    }
    async fn inventory_create_session(
        &self,
        name: &str,
        location_filter: Option<&str>,
        notes: Option<&str>,
        scope_place: Option<i16>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        Repository::inventory_create_session(self, name, location_filter, notes, scope_place, created_by)
            .await
    }
    async fn inventory_close_session(&self, id: i64) -> AppResult<InventorySession> {
        Repository::inventory_close_session(self, id).await
    }
    async fn inventory_scan_barcode(
        &self,
        session_id: i64,
        barcode: &str,
        scanned_by: Option<i64>,
    ) -> AppResult<InventoryScan> {
        Repository::inventory_scan_barcode(self, session_id, barcode, scanned_by).await
    }
    async fn inventory_list_scans_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryScan>, i64)> {
        Repository::inventory_list_scans_page(self, session_id, page, per_page).await
    }
    async fn inventory_list_missing_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryMissingRow>, i64)> {
        Repository::inventory_list_missing_page(self, session_id, page, per_page).await
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
    /// List inventory sessions (paginated, newest first).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_sessions_page(
        &self,
        page: i64,
        per_page: i64,
        status: Option<InventoryStatus>,
    ) -> AppResult<(Vec<InventorySession>, i64)> {
        let offset = (page - 1).max(0) * per_page;
        let total: i64 = if let Some(ref st) = status {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM inventory_sessions WHERE status = $1",
            )
            .bind(st.as_str())
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM inventory_sessions")
                .fetch_one(&self.pool)
                .await?
        };

        let rows = if let Some(st) = status {
            sqlx::query_as::<_, InventorySession>(
                "SELECT * FROM inventory_sessions WHERE status = $1
                 ORDER BY started_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(st.as_str())
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, InventorySession>(
                "SELECT * FROM inventory_sessions ORDER BY started_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };
        Ok((rows, total))
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
        scope_place: Option<i16>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        let id = next_id();
        let row = sqlx::query_as::<_, InventorySession>(
            r#"
            INSERT INTO inventory_sessions (id, name, location_filter, notes, scope_place, created_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(location_filter)
        .bind(notes)
        .bind(scope_place)
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
        scanned_by: Option<i64>,
    ) -> AppResult<InventoryScan> {
        let row: Option<(i64, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, archived_at FROM items WHERE barcode = $1 LIMIT 1",
        )
        .bind(barcode)
        .fetch_optional(&self.pool)
        .await?;

        let (item_id, result) = match row {
            None => (None, InventoryScanResult::UnknownBarcode),
            Some((id, archived_at)) => {
                if archived_at.is_some() {
                    (Some(id), InventoryScanResult::FoundArchived)
                } else {
                    (Some(id), InventoryScanResult::Found)
                }
            }
        };

        let row = sqlx::query_as::<_, InventoryScan>(
            r#"
            INSERT INTO inventory_scans (session_id, item_id, barcode, result, scanned_by)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(session_id)
        .bind(item_id)
        .bind(barcode)
        .bind(result)
        .bind(scanned_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Paginated scans for a session (oldest first).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_scans_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryScan>, i64)> {
        let offset = (page - 1).max(0) * per_page;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM inventory_scans WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query_as::<_, InventoryScan>(
            "SELECT * FROM inventory_scans WHERE session_id = $1
             ORDER BY scanned_at ASC, id ASC LIMIT $2 OFFSET $3",
        )
        .bind(session_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok((rows, total))
    }

    /// Active items in session scope never seen as `item_id` on a scan (paginated).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_missing_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryMissingRow>, i64)> {
        let offset = (page - 1).max(0) * per_page;

        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1 AND sc.item_id = i.id
              )
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query_as::<_, InventoryMissingRow>(
            r#"
            SELECT i.id AS item_id, i.barcode, i.call_number, i.place, b.title AS biblio_title
            FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            LEFT JOIN biblios b ON b.id = i.biblio_id
            WHERE i.archived_at IS NULL
              AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1 AND sc.item_id = i.id
              )
            ORDER BY i.id
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(session_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok((rows, total))
    }

    /// Enriched discrepancy report (respects session `scope_place`).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_report(&self, session_id: i64) -> AppResult<InventoryReport> {
        let expected_in_scope: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let missing_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1 AND sc.item_id = i.id
              )
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let missing_scannable: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
              AND i.barcode IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1 AND sc.item_id = i.id
              )
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let missing_without_barcode: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
              AND i.barcode IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1 AND sc.item_id = i.id
              )
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let total_scanned: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM inventory_scans WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let total_found: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM inventory_scans WHERE session_id = $1 AND result = 'found'",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let total_found_archived: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM inventory_scans WHERE session_id = $1 AND result = 'found_archived'",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let total_unknown: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM inventory_scans WHERE session_id = $1 AND result = 'unknown_barcode'",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let distinct_items_scanned: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT item_id)::bigint FROM inventory_scans
            WHERE session_id = $1 AND item_id IS NOT NULL
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let scans_with_item: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM inventory_scans
            WHERE session_id = $1 AND item_id IS NOT NULL
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let duplicate_scan_count = scans_with_item - distinct_items_scanned;

        Ok(InventoryReport {
            session_id,
            expected_in_scope,
            total_scanned,
            total_found,
            total_found_archived,
            total_unknown,
            distinct_items_scanned,
            duplicate_scan_count,
            missing_count,
            missing_scannable,
            missing_without_barcode,
        })
    }
}
