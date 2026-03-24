//! Maintenance repository: data-quality operations that keep the catalog clean and consistent.
//!
//! Each operation runs inside a single transaction and returns a [`MaintenanceDetail`] map
//! with named counters (rows_fixed, rows_deleted, …) so callers can report exactly what changed.

use std::collections::BTreeMap;

use async_trait::async_trait;

use super::Repository;
use crate::error::AppResult;

/// Named counters returned by every maintenance operation (e.g. "rows_fixed" → 3).
pub type MaintenanceDetail = BTreeMap<&'static str, i64>;

#[async_trait]
pub trait MaintenanceRepository: Send + Sync {
    /// Strip surrounding ASCII double-quotes from series names and delete orphan series
    /// (not linked to any biblio via `biblio_series`).
    async fn maintenance_cleanup_series(&self) -> AppResult<MaintenanceDetail>;

    /// Strip surrounding ASCII double-quotes from collection names and delete orphan
    /// collections (not linked to any biblio via `biblio_collections`).
    async fn maintenance_cleanup_collections(&self) -> AppResult<MaintenanceDetail>;

    /// Delete authors that have no entry in `biblio_authors` (unreachable from any biblio).
    async fn maintenance_cleanup_orphan_authors(&self) -> AppResult<MaintenanceDetail>;

    /// Merge series whose names are identical after case-folding and trimming.
    /// The oldest record (lowest id) becomes the canonical one; all `biblio_series`
    /// references are re-pointed and duplicate series rows are deleted.
    async fn maintenance_merge_duplicate_series(&self) -> AppResult<MaintenanceDetail>;

    /// Same as above but for collections / `biblio_collections`.
    async fn maintenance_merge_duplicate_collections(&self) -> AppResult<MaintenanceDetail>;

    /// Delete `biblio_series` rows that reference a series_id that no longer exists
    /// (defensive check — should not happen with proper FK, but guards against manual edits).
    async fn maintenance_cleanup_dangling_biblio_series(&self) -> AppResult<MaintenanceDetail>;

    /// Same defensive check for `biblio_collections`.
    async fn maintenance_cleanup_dangling_biblio_collections(&self) -> AppResult<MaintenanceDetail>;
}

// ─── impl ────────────────────────────────────────────────────────────────────

#[async_trait]
impl MaintenanceRepository for Repository {
    async fn maintenance_cleanup_series(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let quoted_fixed = sqlx::query(
            r#"
            UPDATE series
            SET    name       = trim(both '"' from name),
                   updated_at = NOW()
            WHERE  name IS NOT NULL
              AND  length(name) >= 2
              AND  left(name, 1)  = '"'
              AND  right(name, 1) = '"'
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        let orphans_deleted = sqlx::query(
            r#"
            DELETE FROM series s
            WHERE NOT EXISTS (
                SELECT 1 FROM biblio_series bs WHERE bs.series_id = s.id
            )
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("quoted_names_fixed", quoted_fixed);
        detail.insert("orphans_deleted", orphans_deleted);
        Ok(detail)
    }

    async fn maintenance_cleanup_collections(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let quoted_fixed = sqlx::query(
            r#"
            UPDATE collections
            SET    name       = trim(both '"' from name),
                   updated_at = NOW()
            WHERE  name IS NOT NULL
              AND  length(name) >= 2
              AND  left(name, 1)  = '"'
              AND  right(name, 1) = '"'
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        let orphans_deleted = sqlx::query(
            r#"
            DELETE FROM collections c
            WHERE NOT EXISTS (
                SELECT 1 FROM biblio_collections bc WHERE bc.collection_id = c.id
            )
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("quoted_names_fixed", quoted_fixed);
        detail.insert("orphans_deleted", orphans_deleted);
        Ok(detail)
    }

    async fn maintenance_cleanup_orphan_authors(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let orphans_deleted = sqlx::query(
            r#"
            DELETE FROM authors a
            WHERE NOT EXISTS (
                SELECT 1 FROM biblio_authors ba WHERE ba.author_id = a.id
            )
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("orphans_deleted", orphans_deleted);
        Ok(detail)
    }

    async fn maintenance_merge_duplicate_series(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        // Load all series to detect duplicates in Rust (simpler than pure SQL for re-pointing).
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, name FROM series ORDER BY id")
                .fetch_all(&mut *tx)
                .await?;

        // Group ids by normalized name; first id in each group is the canonical one.
        let mut by_norm: std::collections::HashMap<String, Vec<i64>> =
            std::collections::HashMap::new();
        for (id, name) in rows {
            let key = name.trim().to_lowercase();
            by_norm.entry(key).or_default().push(id);
        }

        let mut refs_moved: i64 = 0;
        let mut duplicates_deleted: i64 = 0;

        for mut ids in by_norm.into_values() {
            if ids.len() <= 1 {
                continue;
            }
            ids.sort_unstable();
            let canonical_id = ids[0];

            for dup_id in &ids[1..] {
                // Re-point biblio_series rows; skip conflicts (biblio already linked to canonical).
                let moved = sqlx::query(
                    r#"
                    INSERT INTO biblio_series (biblio_id, series_id, position, volume_number)
                    SELECT biblio_id, $1, position, volume_number
                    FROM   biblio_series
                    WHERE  series_id = $2
                    ON CONFLICT (biblio_id, series_id) DO NOTHING
                    "#,
                )
                .bind(canonical_id)
                .bind(dup_id)
                .execute(&mut *tx)
                .await?
                .rows_affected() as i64;

                refs_moved += moved;

                // Remove old junction rows pointing to the duplicate.
                sqlx::query("DELETE FROM biblio_series WHERE series_id = $1")
                    .bind(dup_id)
                    .execute(&mut *tx)
                    .await?;

                // Delete the duplicate series record (now an orphan).
                let deleted = sqlx::query("DELETE FROM series WHERE id = $1")
                    .bind(dup_id)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected() as i64;

                duplicates_deleted += deleted;
            }
        }

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("duplicates_merged", duplicates_deleted);
        detail.insert("biblio_refs_moved", refs_moved);
        Ok(detail)
    }

    async fn maintenance_merge_duplicate_collections(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, name FROM collections ORDER BY id")
                .fetch_all(&mut *tx)
                .await?;

        let mut by_norm: std::collections::HashMap<String, Vec<i64>> =
            std::collections::HashMap::new();
        for (id, name) in rows {
            let key = name.trim().to_lowercase();
            by_norm.entry(key).or_default().push(id);
        }

        let mut refs_moved: i64 = 0;
        let mut duplicates_deleted: i64 = 0;

        for mut ids in by_norm.into_values() {
            if ids.len() <= 1 {
                continue;
            }
            ids.sort_unstable();
            let canonical_id = ids[0];

            for dup_id in &ids[1..] {
                let moved = sqlx::query(
                    r#"
                    INSERT INTO biblio_collections (biblio_id, collection_id, position, volume_number)
                    SELECT biblio_id, $1, position, volume_number
                    FROM   biblio_collections
                    WHERE  collection_id = $2
                    ON CONFLICT (biblio_id, collection_id) DO NOTHING
                    "#,
                )
                .bind(canonical_id)
                .bind(dup_id)
                .execute(&mut *tx)
                .await?
                .rows_affected() as i64;

                refs_moved += moved;

                sqlx::query("DELETE FROM biblio_collections WHERE collection_id = $1")
                    .bind(dup_id)
                    .execute(&mut *tx)
                    .await?;

                let deleted = sqlx::query("DELETE FROM collections WHERE id = $1")
                    .bind(dup_id)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected() as i64;

                duplicates_deleted += deleted;
            }
        }

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("duplicates_merged", duplicates_deleted);
        detail.insert("biblio_refs_moved", refs_moved);
        Ok(detail)
    }

    async fn maintenance_cleanup_dangling_biblio_series(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let deleted = sqlx::query(
            r#"
            DELETE FROM biblio_series bs
            WHERE NOT EXISTS (
                SELECT 1 FROM series s WHERE s.id = bs.series_id
            )
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("dangling_refs_deleted", deleted);
        Ok(detail)
    }

    async fn maintenance_cleanup_dangling_biblio_collections(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let deleted = sqlx::query(
            r#"
            DELETE FROM biblio_collections bc
            WHERE NOT EXISTS (
                SELECT 1 FROM collections c WHERE c.id = bc.collection_id
            )
            "#,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("dangling_refs_deleted", deleted);
        Ok(detail)
    }
}
