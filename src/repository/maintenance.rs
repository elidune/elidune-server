//! Maintenance repository: data-quality operations that keep the catalog clean and consistent.
//!
//! Each operation runs inside a single transaction and returns a [`MaintenanceDetail`] map
//! with named counters (rows_fixed, rows_deleted, …) so callers can report exactly what changed.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{Datelike, NaiveDate, Utc};
use unicode_normalization::UnicodeNormalization;

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
    async fn maintenance_cleanup_authors(&self) -> AppResult<MaintenanceDetail>;

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

    /// Delete soft-deleted user rows, normalize `addr_city`, and set `public_type` from `birthdate`
    /// using each `public_types` row's `age_min` / `age_max` (when not null).
    async fn maintenance_cleanup_users(&self) -> AppResult<MaintenanceDetail>;
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

    // delete from series where name is empty or null
    let deleted_series = sqlx::query("DELETE FROM series WHERE name IS NULL OR name = ''")
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;


    // force collection key to be normalized
    let series = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM series")
    .fetch_all(&mut *tx)
    .await?;
    let mut series_merged = 0;
    let mut series_updated = 0;
    for (id, name) in series {
    let normalized_key = Repository::normalize_key(&name);

    // check if another collection with the same key exists
    let another_series = sqlx::query_scalar::<_, i64>("SELECT id  FROM series WHERE key = $1 AND id != $2")
        .bind(&normalized_key)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    if another_series.is_some() {
        //update biblio_series to use the new series id
        sqlx::query("UPDATE biblio_series SET series_id = $1 WHERE series_id = $2")
            .bind(another_series.unwrap())
            .bind(id)
            .execute(&mut *tx)
            .await?;

        //delete the old series
        sqlx::query("DELETE FROM series WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        series_merged += 1;
    } else {
        //update the collection key
        sqlx::query("UPDATE series SET key = $1 WHERE id = $2")
            .bind(&normalized_key)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        series_updated += 1;
    }
    }


        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("quoted_names_fixed", quoted_fixed);
        detail.insert("orphans_deleted", orphans_deleted);
        detail.insert("series_deleted", deleted_series);
        detail.insert("series_merged", series_merged);
        detail.insert("series_updated", series_updated);
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

        // delete from collections where name is empty or null
        let deleted_collections = sqlx::query("DELETE FROM collections WHERE name IS NULL OR name = ''")
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;



        // force collection key to be normalized
        let collections = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM collections")
            .fetch_all(&mut *tx)
            .await?;
        let mut collections_merged = 0;
        let mut collections_updated = 0;
        for (id, name) in collections {
            let normalized_key = Repository::normalize_key(&name);

        // check if another collection with the same key exists
            let another_collection = sqlx::query_scalar::<_, i64>("SELECT id  FROM collections WHERE key = $1 AND id != $2")
                .bind(&normalized_key)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;

            if another_collection.is_some() {
                //update biblio_collections to use the new collection id
                sqlx::query("UPDATE biblio_collections SET collection_id = $1 WHERE collection_id = $2")
                    .bind(another_collection.unwrap())
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;

                //delete the old collection
                sqlx::query("DELETE FROM collections WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                collections_merged += 1;
            } else {
                //update the collection key
                sqlx::query("UPDATE collections SET key = $1 WHERE id = $2")
                    .bind(&normalized_key)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                collections_updated += 1;
            }
        }

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("quoted_names_fixed", quoted_fixed);
        detail.insert("orphans_deleted", orphans_deleted);
        detail.insert("collections_deleted", deleted_collections);
        detail.insert("collections_merged", collections_merged);
        detail.insert("collections_updated", collections_updated);
        Ok(detail)
    }

    async fn maintenance_cleanup_authors(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        // delete from authors where name is empty or null, cascade to biblio_authors automatically
        let deleted_authors = sqlx::query("DELETE FROM authors WHERE (lastname IS NULL OR lastname = '') AND (firstname IS NULL OR firstname = '')")
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;


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
        detail.insert("authors_deleted", deleted_authors);
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


    async fn maintenance_cleanup_users(&self) -> AppResult<MaintenanceDetail> {
        let mut tx = self.pool.begin().await?;

        let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
            r#"SELECT id, addr_city FROM users WHERE addr_city IS NOT NULL"#,
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut cities_sanitized: i64 = 0;
        for (id, old_city) in rows {
            let Some(ref old) = old_city else {
                continue;
            };
            let new_city = sanitize_user_city(old);
            let should_update = match &new_city {
                None => true,
                Some(n) => n != old,
            };
            if !should_update {
                continue;
            }
            sqlx::query("UPDATE users SET addr_city = $1 WHERE id = $2")
                .bind(new_city)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            cities_sanitized += 1;
        }

        let age_rules: Vec<(i64, Option<i16>, Option<i16>)> = sqlx::query_as(
            r#"SELECT id, age_min, age_max FROM public_types ORDER BY id"#,
        )
        .fetch_all(&mut *tx)
        .await?;

        let rules: Vec<PublicTypeAgeRule> = age_rules
            .into_iter()
            .map(|(id, age_min, age_max)| PublicTypeAgeRule {
                id,
                age_min,
                age_max,
            })
            .collect();

        let today = Utc::now().date_naive();

        let user_rows: Vec<(i64, NaiveDate, Option<i64>)> = sqlx::query_as(
            r#"SELECT id, birthdate, public_type FROM users WHERE birthdate IS NOT NULL"#,
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut public_types_updated: i64 = 0;
        for (user_id, birthdate, current_pt) in user_rows {
            let age = age_years_on_date(birthdate, today);
            if age < 0 {
                continue;
            }
            let Some(resolved) = resolve_public_type_for_age(&rules, age) else {
                continue;
            };
            if current_pt == Some(resolved) {
                continue;
            }
            sqlx::query("UPDATE users SET public_type = $1 WHERE id = $2")
                .bind(resolved)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            public_types_updated += 1;
        }

     

        tx.commit().await?;

        let mut detail = MaintenanceDetail::new();
        detail.insert("cities_sanitized", cities_sanitized);
        detail.insert("public_types_updated", public_types_updated);
        Ok(detail)
    }
}

#[derive(Debug, Clone)]
struct PublicTypeAgeRule {
    id: i64,
    age_min: Option<i16>,
    age_max: Option<i16>,
}

/// Inclusive age check: `age_min` / `age_max` apply only when set.
fn age_matches_public_type_bounds(age: i32, age_min: Option<i16>, age_max: Option<i16>) -> bool {
    if let Some(m) = age_min {
        if age < i32::from(m) {
            return false;
        }
    }
    if let Some(m) = age_max {
        if age > i32::from(m) {
            return false;
        }
    }
    true
}

/// Smallest inclusive span wins (most specific band); open-ended / unbounded rules sort last.
fn public_type_age_rule_rank(r: &PublicTypeAgeRule) -> (i32, i64) {
    let width = match (r.age_min, r.age_max) {
        (Some(min), Some(max)) => i32::from(max - min),
        _ => i32::MAX / 2,
    };
    (width, r.id)
}

fn resolve_public_type_for_age(rules: &[PublicTypeAgeRule], age: i32) -> Option<i64> {
    rules
        .iter()
        .filter(|r| age_matches_public_type_bounds(age, r.age_min, r.age_max))
        .min_by_key(|r| public_type_age_rule_rank(r))
        .map(|r| r.id)
}

fn age_years_on_date(birth: NaiveDate, on: NaiveDate) -> i32 {
    let mut years = on.year() - birth.year();
    if on.month() < birth.month()
        || (on.month() == birth.month() && on.day() < birth.day())
    {
        years -= 1;
    }
    years
}

/// Normalizes free-text city values so reports and deduplication see a consistent shape:
/// trims, NFC, collapses internal whitespace, maps common typographic punctuation to ASCII,
/// then applies title case after word/hyphen/apostrophe boundaries (French-friendly).
fn sanitize_user_city(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mapped: String = trimmed
        .chars()
        .map(|c| match c {
            '\u{2019}' | '\u{2018}' | '\u{02BC}' => '\'',
            '\u{2013}' | '\u{2014}' => '-',
            '\u{00A0}' => ' ',
            _ => c,
        })
        .collect();
    let nfc: String = mapped.nfc().collect();
    let collapsed = nfc.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(title_case_city_word_boundaries(&collapsed))
}

fn title_case_city_word_boundaries(lowered_full: &str) -> String {
    let lower = lowered_full.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut after_boundary = true;
    for c in lower.chars() {
        if c == ' ' || c == '-' || c == '\'' {
            out.push(c);
            after_boundary = true;
        } else if after_boundary && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            after_boundary = false;
        } else {
            out.push(c);
            if c.is_alphabetic() {
                after_boundary = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod city_sanitize_tests {
    use super::{sanitize_user_city, title_case_city_word_boundaries};

    #[test]
    fn trim_and_collapse_spaces() {
        assert_eq!(
            sanitize_user_city("  saint   étienne  ").as_deref(),
            Some("Saint Étienne")
        );
        assert_eq!(
            sanitize_user_city("saint-étienne").as_deref(),
            Some("Saint-Étienne")
        );
    }

    #[test]
    fn nfc_and_punctuation() {
        assert_eq!(
            sanitize_user_city("L\u{2019}ISLE").as_deref(),
            Some("L'Isle")
        );
    }

    #[test]
    fn apostrophe_hyphen_segments() {
        let s = title_case_city_word_boundaries("l'isle-adam");
        assert_eq!(s, "L'Isle-Adam");
    }

    #[test]
    fn empty_becomes_none() {
        assert_eq!(sanitize_user_city("   "), None);
    }
}

#[cfg(test)]
mod public_type_age_tests {
    use chrono::NaiveDate;

    use super::{
        PublicTypeAgeRule, age_matches_public_type_bounds, age_years_on_date,
        resolve_public_type_for_age,
    };

    fn rules_sample() -> Vec<PublicTypeAgeRule> {
        vec![
            PublicTypeAgeRule {
                id: 1,
                age_min: Some(0),
                age_max: Some(12),
            },
            PublicTypeAgeRule {
                id: 2,
                age_min: Some(0),
                age_max: Some(17),
            },
            PublicTypeAgeRule {
                id: 3,
                age_min: Some(18),
                age_max: None,
            },
        ]
    }

    #[test]
    fn picks_narrowest_matching_band() {
        let rules = rules_sample();
        assert_eq!(resolve_public_type_for_age(&rules, 10), Some(1));
        assert_eq!(resolve_public_type_for_age(&rules, 15), Some(2));
        assert_eq!(resolve_public_type_for_age(&rules, 30), Some(3));
    }

    #[test]
    fn bounds_respect_min_max() {
        assert!(age_matches_public_type_bounds(18, Some(18), Some(64)));
        assert!(!age_matches_public_type_bounds(17, Some(18), Some(64)));
        assert!(!age_matches_public_type_bounds(65, Some(18), Some(64)));
    }

    #[test]
    fn birthday_before_reference_yields_expected_age() {
        let birth = NaiveDate::from_ymd_opt(2010, 6, 15).unwrap();
        let on = NaiveDate::from_ymd_opt(2025, 6, 14).unwrap();
        assert_eq!(age_years_on_date(birth, on), 14);
        let on2 = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        assert_eq!(age_years_on_date(birth, on2), 15);
    }
}
