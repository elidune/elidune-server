//! Inventory / stocktaking domain methods on Repository

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use snowflaked::Generator;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::inventory::{
        InventoryConsolidationPreviewLoan, InventoryConsolidationPreviewRow,
        InventoryConsolidationPreviewSummary, InventoryMissingRow, InventoryReport, InventoryScan,
        InventoryScanResult, InventorySession, InventoryStatus,
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
        scope_source_id: Option<i64>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession>;
    async fn inventory_count_expected_in_scope(
        &self,
        scope_source_id: Option<i64>,
        scope_place: Option<i16>,
    ) -> AppResult<i64>;
    async fn inventory_has_open_session_for_scope(
        &self,
        scope_source_id: Option<i64>,
        scope_place: Option<i16>,
    ) -> AppResult<bool>;
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
    async fn inventory_list_missing_item_ids(&self, session_id: i64) -> AppResult<Vec<i64>>;
    async fn inventory_mark_consolidated(
        &self,
        session_id: i64,
        consolidated_by: Option<i64>,
    ) -> AppResult<InventorySession>;
    async fn inventory_consolidation_preview_summary(
        &self,
        session_id: i64,
    ) -> AppResult<InventoryConsolidationPreviewSummary>;
    async fn inventory_consolidation_preview_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryConsolidationPreviewRow>, i64)>;
    async fn inventory_list_loan_closures_for_missing(
        &self,
        session_id: i64,
    ) -> AppResult<Vec<InventoryLoanClosureRow>>;
}

/// Active loan on a missing copy — used before forced consolidation emails.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InventoryLoanClosureRow {
    pub item_id: i64,
    pub barcode: Option<String>,
    pub biblio_title: Option<String>,
    pub loan_id: i64,
    pub user_id: i64,
    pub user_email: Option<String>,
    pub user_firstname: Option<String>,
    pub user_lastname: Option<String>,
    pub user_language: Option<String>,
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
        scope_source_id: Option<i64>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        Repository::inventory_create_session(
            self,
            name,
            location_filter,
            notes,
            scope_place,
            scope_source_id,
            created_by,
        )
        .await
    }
    async fn inventory_count_expected_in_scope(
        &self,
        scope_source_id: Option<i64>,
        scope_place: Option<i16>,
    ) -> AppResult<i64> {
        Repository::inventory_count_expected_in_scope(self, scope_source_id, scope_place).await
    }
    async fn inventory_has_open_session_for_scope(
        &self,
        scope_source_id: Option<i64>,
        scope_place: Option<i16>,
    ) -> AppResult<bool> {
        Repository::inventory_has_open_session_for_scope(self, scope_source_id, scope_place).await
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
    async fn inventory_list_missing_item_ids(&self, session_id: i64) -> AppResult<Vec<i64>> {
        Repository::inventory_list_missing_item_ids(self, session_id).await
    }
    async fn inventory_mark_consolidated(
        &self,
        session_id: i64,
        consolidated_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        Repository::inventory_mark_consolidated(self, session_id, consolidated_by).await
    }
    async fn inventory_consolidation_preview_summary(
        &self,
        session_id: i64,
    ) -> AppResult<InventoryConsolidationPreviewSummary> {
        Repository::inventory_consolidation_preview_summary(self, session_id).await
    }
    async fn inventory_consolidation_preview_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryConsolidationPreviewRow>, i64)> {
        Repository::inventory_consolidation_preview_page(self, session_id, page, per_page).await
    }
    async fn inventory_list_loan_closures_for_missing(
        &self,
        session_id: i64,
    ) -> AppResult<Vec<InventoryLoanClosureRow>> {
        Repository::inventory_list_loan_closures_for_missing(self, session_id).await
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

const INVENTORY_SESSION_FROM: &str = r#"
    FROM inventory_sessions inv
    LEFT JOIN sources so ON so.id = inv.scope_source_id
"#;

const INVENTORY_SCOPE_PREDICATE: &str = r#"
    (inv.scope_source_id IS NULL OR i.source_id = inv.scope_source_id)
    AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
"#;

const MISSING_ITEMS_CTE: &str = r#"
    WITH missing_items AS (
        SELECT i.*
        FROM items i
        INNER JOIN inventory_sessions inv ON inv.id = $1
        WHERE i.archived_at IS NULL
          AND (inv.scope_source_id IS NULL OR i.source_id = inv.scope_source_id)
          AND (inv.scope_place IS NULL OR i.place = inv.scope_place)
          AND NOT EXISTS (
              SELECT 1 FROM inventory_scans sc
              WHERE sc.session_id = $1
                AND sc.item_id = i.id
                AND sc.result = 'found'
          )
    )
"#;

fn item_matches_session_scope(
    scope_source_id: Option<i64>,
    scope_place: Option<i16>,
    item_source_id: Option<i64>,
    item_place: Option<i16>,
) -> bool {
    if let Some(source_id) = scope_source_id {
        if item_source_id != Some(source_id) {
            return false;
        }
    }
    if let Some(place) = scope_place {
        if item_place != Some(place) {
            return false;
        }
    }
    true
}

impl Repository {
    async fn inventory_fetch_session(&self, id: i64) -> AppResult<InventorySession> {
        let sql = format!(
            "SELECT inv.*, so.name AS scope_source_name {INVENTORY_SESSION_FROM} WHERE inv.id = $1"
        );
        sqlx::query_as::<_, InventorySession>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Inventory session {id} not found")))
    }
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
                &format!(
                    "SELECT inv.*, so.name AS scope_source_name \
                     {INVENTORY_SESSION_FROM} WHERE inv.status = $1 \
                     ORDER BY inv.started_at DESC LIMIT $2 OFFSET $3"
                ),
            )
            .bind(st.as_str())
            .bind(per_page)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, InventorySession>(
                &format!(
                    "SELECT inv.*, so.name AS scope_source_name \
                     {INVENTORY_SESSION_FROM} ORDER BY inv.started_at DESC LIMIT $1 OFFSET $2"
                ),
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
        self.inventory_fetch_session(id).await
    }

    /// Count active items matching optional source and place scope (before session exists).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_count_expected_in_scope(
        &self,
        scope_source_id: Option<i64>,
        scope_place: Option<i16>,
    ) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM items i
            WHERE i.archived_at IS NULL
              AND ($1::bigint IS NULL OR i.source_id = $1)
              AND ($2::smallint IS NULL OR i.place = $2)
            "#,
        )
        .bind(scope_source_id)
        .bind(scope_place)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Whether an open session already exists for the same scope dimensions.
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_has_open_session_for_scope(
        &self,
        scope_source_id: Option<i64>,
        scope_place: Option<i16>,
    ) -> AppResult<bool> {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM inventory_sessions
                WHERE status = 'open'
                  AND scope_source_id IS NOT DISTINCT FROM $1
                  AND scope_place IS NOT DISTINCT FROM $2
            )
            "#,
        )
        .bind(scope_source_id)
        .bind(scope_place)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// Create a new inventory session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_create_session(
        &self,
        name: &str,
        location_filter: Option<&str>,
        notes: Option<&str>,
        scope_place: Option<i16>,
        scope_source_id: Option<i64>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        let id = next_id();
        sqlx::query(
            r#"
            INSERT INTO inventory_sessions (
                id, name, location_filter, notes, scope_place, scope_source_id, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(location_filter)
        .bind(notes)
        .bind(scope_place)
        .bind(scope_source_id)
        .bind(created_by)
        .execute(&self.pool)
        .await?;
        self.inventory_fetch_session(id).await
    }

    /// Close an inventory session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_close_session(&self, id: i64) -> AppResult<InventorySession> {
        let updated: Option<i64> = sqlx::query_scalar(
            "UPDATE inventory_sessions SET status = 'closed', closed_at = NOW()
             WHERE id = $1 AND status = 'open' RETURNING id",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        updated.ok_or_else(|| AppError::NotFound(format!("Open session {id} not found")))?;
        self.inventory_fetch_session(id).await
    }

    /// Record a barcode scan in a session
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_scan_barcode(
        &self,
        session_id: i64,
        barcode: &str,
        scanned_by: Option<i64>,
    ) -> AppResult<InventoryScan> {
        let session = self.inventory_fetch_session(session_id).await?;

        let row: Option<(i64, Option<DateTime<Utc>>, Option<i64>, Option<i16>)> = sqlx::query_as(
            "SELECT id, archived_at, source_id, place FROM items WHERE barcode = $1 LIMIT 1",
        )
        .bind(barcode)
        .fetch_optional(&self.pool)
        .await?;

        let (item_id, result) = match row {
            None => (None, InventoryScanResult::UnknownBarcode),
            Some((id, archived_at, source_id, place)) => {
                if archived_at.is_some() {
                    (Some(id), InventoryScanResult::FoundArchived)
                } else if item_matches_session_scope(
                    session.scope_source_id,
                    session.scope_place,
                    source_id,
                    place,
                ) {
                    (Some(id), InventoryScanResult::Found)
                } else {
                    (Some(id), InventoryScanResult::FoundOutOfScope)
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
            &format!(
                r#"
            SELECT COUNT(*) FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND {INVENTORY_SCOPE_PREDICATE}
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1
                    AND sc.item_id = i.id
                    AND sc.result = 'found'
              )
            "#
            ),
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query_as::<_, InventoryMissingRow>(
            &format!(
                r#"
            SELECT
                i.id AS item_id,
                i.barcode,
                i.call_number,
                i.place,
                b.title AS biblio_title,
                i.source_id,
                so.name AS source_name
            FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            LEFT JOIN biblios b ON b.id = i.biblio_id
            LEFT JOIN sources so ON so.id = i.source_id
            WHERE i.archived_at IS NULL
              AND {INVENTORY_SCOPE_PREDICATE}
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1
                    AND sc.item_id = i.id
                    AND sc.result = 'found'
              )
            ORDER BY i.id
            LIMIT $2 OFFSET $3
            "#
            ),
        )
        .bind(session_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok((rows, total))
    }

    /// Enriched discrepancy report (respects session scope on source and place).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_report(&self, session_id: i64) -> AppResult<InventoryReport> {
        let scope_sql = format!(
            r#"
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND {INVENTORY_SCOPE_PREDICATE}
            "#
        );

        let expected_in_scope: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*)::bigint FROM items i {scope_sql}"
        ))
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let missing_predicate = format!(
            r#"
            {scope_sql}
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1
                    AND sc.item_id = i.id
                    AND sc.result = 'found'
              )
            "#
        );

        let missing_count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*)::bigint FROM items i {missing_predicate}"
        ))
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let missing_scannable: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*)::bigint FROM items i {missing_predicate} AND i.barcode IS NOT NULL"
        ))
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let missing_without_barcode: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*)::bigint FROM items i {missing_predicate} AND i.barcode IS NULL"
        ))
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

        let total_found_out_of_scope: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM inventory_scans WHERE session_id = $1 AND result = 'found_out_of_scope'",
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
            WHERE session_id = $1 AND result = 'found' AND item_id IS NOT NULL
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        let scans_with_item: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint FROM inventory_scans
            WHERE session_id = $1 AND result = 'found' AND item_id IS NOT NULL
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
            total_found_out_of_scope,
            total_unknown,
            distinct_items_scanned,
            duplicate_scan_count,
            missing_count,
            missing_scannable,
            missing_without_barcode,
        })
    }

    /// Active in-scope item ids never linked by any scan in the session.
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_missing_item_ids(&self, session_id: i64) -> AppResult<Vec<i64>> {
        let rows: Vec<(i64,)> = sqlx::query_as(
            &format!(
                r#"
            SELECT i.id
            FROM items i
            INNER JOIN inventory_sessions inv ON inv.id = $1
            WHERE i.archived_at IS NULL
              AND {INVENTORY_SCOPE_PREDICATE}
              AND NOT EXISTS (
                  SELECT 1 FROM inventory_scans sc
                  WHERE sc.session_id = $1
                    AND sc.item_id = i.id
                    AND sc.result = 'found'
              )
            ORDER BY i.id
            "#
            ),
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Mark a closed session as consolidated (idempotent guard via `consolidated_at IS NULL`).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_mark_consolidated(
        &self,
        session_id: i64,
        consolidated_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        let updated: Option<i64> = sqlx::query_scalar(
            r#"
            UPDATE inventory_sessions
            SET consolidated_at = NOW(), consolidated_by = $2
            WHERE id = $1
              AND status = 'closed'
              AND consolidated_at IS NULL
            RETURNING id
            "#,
        )
        .bind(session_id)
        .bind(consolidated_by)
        .fetch_optional(&self.pool)
        .await?;
        updated.ok_or_else(|| {
            AppError::Conflict(format!(
                "Session {session_id} is not eligible for consolidation (open, unknown, or already consolidated)"
            ))
        })?;
        self.inventory_fetch_session(session_id).await
    }

    /// Summary for consolidation preview.
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_consolidation_preview_summary(
        &self,
        session_id: i64,
    ) -> AppResult<InventoryConsolidationPreviewSummary> {
        let row: (i64, i64, i64, i64) = sqlx::query_as(&format!(
            r#"
            {MISSING_ITEMS_CTE}
            SELECT
                COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE l.id IS NOT NULL)::bigint,
                COUNT(DISTINCT l.user_id) FILTER (WHERE l.id IS NOT NULL)::bigint,
                COUNT(DISTINCT mi.biblio_id) FILTER (
                    WHERE (
                        SELECT COUNT(*)::bigint FROM items i2
                        WHERE i2.biblio_id = mi.biblio_id AND i2.archived_at IS NULL
                    ) = 1
                )::bigint
            FROM missing_items mi
            LEFT JOIN loans l ON l.item_id = mi.id AND l.returned_at IS NULL
            "#
        ))
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(InventoryConsolidationPreviewSummary {
            total_missing: row.0,
            on_loan_count: row.1,
            deletable_without_force: row.0 - row.1,
            affected_readers_count: row.2,
            orphan_biblios_count: row.3,
        })
    }

    /// Paginated consolidation preview rows (missing copies + loan / orphan hints).
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_consolidation_preview_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryConsolidationPreviewRow>, i64)> {
        let offset = (page - 1).max(0) * per_page;

        let total: i64 = sqlx::query_scalar(&format!(
            "{MISSING_ITEMS_CTE} SELECT COUNT(*)::bigint FROM missing_items"
        ))
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;

        #[derive(sqlx::FromRow)]
        struct PreviewDbRow {
            item_id: i64,
            barcode: Option<String>,
            call_number: Option<String>,
            place: Option<i16>,
            source_id: Option<i64>,
            source_name: Option<String>,
            biblio_id: Option<i64>,
            biblio_title: Option<String>,
            loan_id: Option<i64>,
            loan_user_id: Option<i64>,
            loan_user_email: Option<String>,
            loan_user_firstname: Option<String>,
            loan_user_lastname: Option<String>,
            loan_expiry_at: Option<chrono::DateTime<chrono::Utc>>,
            biblio_active_item_count: i64,
        }

        let rows = sqlx::query_as::<_, PreviewDbRow>(&format!(
            r#"
            {MISSING_ITEMS_CTE}
            SELECT
                mi.id AS item_id,
                mi.barcode,
                mi.call_number,
                mi.place,
                mi.source_id,
                so.name AS source_name,
                mi.biblio_id,
                b.title AS biblio_title,
                l.id AS loan_id,
                l.user_id AS loan_user_id,
                u.email AS loan_user_email,
                u.firstname AS loan_user_firstname,
                u.lastname AS loan_user_lastname,
                l.expiry_at AS loan_expiry_at,
                (
                    SELECT COUNT(*)::bigint FROM items i2
                    WHERE i2.biblio_id = mi.biblio_id AND i2.archived_at IS NULL
                ) AS biblio_active_item_count
            FROM missing_items mi
            LEFT JOIN biblios b ON b.id = mi.biblio_id
            LEFT JOIN sources so ON so.id = mi.source_id
            LEFT JOIN loans l ON l.item_id = mi.id AND l.returned_at IS NULL
            LEFT JOIN users u ON u.id = l.user_id
            ORDER BY mi.id
            LIMIT $2 OFFSET $3
            "#
        ))
        .bind(session_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let on_loan = r.loan_id.is_some();
                let active_loan = r.loan_id.map(|loan_id| InventoryConsolidationPreviewLoan {
                    loan_id,
                    user_id: r.loan_user_id.unwrap_or(0),
                    user_email: r.loan_user_email,
                    user_firstname: r.loan_user_firstname,
                    user_lastname: r.loan_user_lastname,
                    expiry_at: r.loan_expiry_at,
                });
                InventoryConsolidationPreviewRow {
                    item_id: r.item_id,
                    barcode: r.barcode,
                    call_number: r.call_number,
                    place: r.place,
                    source_id: r.source_id,
                    source_name: r.source_name,
                    biblio_id: r.biblio_id,
                    biblio_title: r.biblio_title,
                    on_loan,
                    would_skip_without_force: on_loan,
                    biblio_would_be_orphaned: r.biblio_active_item_count == 1,
                    active_loan,
                }
            })
            .collect();

        Ok((items, total))
    }

    /// Active loans on missing copies — for pre-consolidation reader notifications.
    #[tracing::instrument(skip(self), err)]
    pub async fn inventory_list_loan_closures_for_missing(
        &self,
        session_id: i64,
    ) -> AppResult<Vec<InventoryLoanClosureRow>> {
        let rows = sqlx::query_as::<_, InventoryLoanClosureRow>(&format!(
            r#"
            {MISSING_ITEMS_CTE}
            SELECT
                mi.id AS item_id,
                mi.barcode,
                b.title AS biblio_title,
                l.id AS loan_id,
                l.user_id,
                u.email AS user_email,
                u.firstname AS user_firstname,
                u.lastname AS user_lastname,
                u.language AS user_language
            FROM missing_items mi
            INNER JOIN loans l ON l.item_id = mi.id AND l.returned_at IS NULL
            INNER JOIN users u ON u.id = l.user_id
            LEFT JOIN biblios b ON b.id = mi.biblio_id
            ORDER BY l.user_id, mi.id
            "#
        ))
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
