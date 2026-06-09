//! Biblios repository — items operations.

//! Biblios repository — read operations.

use std::collections::HashMap;

use chrono::Utc;
use sqlx::{FromRow, Row};
use sqlx::types::Json;

use super::super::Repository;
use crate::models::item::ItemShort;
use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        author::Author,
        author::Function,
        import_report::DuplicateCandidate,
        biblio::{Collection, Edition, Isbn, Biblio, BiblioQuery, BiblioShort, MeiliBiblioDocument, MediaType, Serie},
        item::Item,
    },
};
use super::BiblioShortRow;

use super::ItemShortRow;

impl Repository {
    /// Get items (physical copies) for a biblio (excludes archived items)
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_items(&self, biblio_id: i64) -> AppResult<Vec<Item>> {
        let items = sqlx::query_as::<_, Item>(
            r#"
            SELECT i.id, i.biblio_id, i.source_id, i.barcode, i.call_number, i.volume_designation,
                   i.place, i.borrowable, i.circulation_status, i.notes, i.price,
                   i.created_at, i.updated_at, i.archived_at,
                   so.name as source_name,
                   EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed,
                   (SELECT l.id FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL ORDER BY l.id DESC LIMIT 1) as loan_id
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.biblio_id = $1 AND i.archived_at IS NULL
            ORDER BY i.barcode
            "#,
        )
        .bind(biblio_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    /// Get one active item by id (same row shape as [`biblios_get_items`]).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_active_by_id(&self, item_id: i64) -> AppResult<Item> {
        sqlx::query_as::<_, Item>(
            r#"
            SELECT i.id, i.biblio_id, i.source_id, i.barcode, i.call_number, i.volume_designation,
                   i.place, i.borrowable, i.circulation_status, i.notes, i.price,
                   i.created_at, i.updated_at, i.archived_at,
                   so.name as source_name,
                   EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed,
                   (SELECT l.id FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL ORDER BY l.id DESC LIMIT 1) as loan_id
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.id = $1 AND i.archived_at IS NULL
            "#,
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Item {item_id} not found")))
    }

    /// Get one active item by barcode (same row shape as [`items_get_active_by_id`]).
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_active_by_barcode(&self, barcode: &str) -> AppResult<Item> {
        sqlx::query_as::<_, Item>(
            r#"
            SELECT i.id, i.biblio_id, i.source_id, i.barcode, i.call_number, i.volume_designation,
                   i.place, i.borrowable, i.circulation_status, i.notes, i.price,
                   i.created_at, i.updated_at, i.archived_at,
                   so.name as source_name,
                   EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed,
                   (SELECT l.id FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL ORDER BY l.id DESC LIMIT 1) as loan_id
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.barcode = $1 AND i.archived_at IS NULL
            "#,
        )
        .bind(barcode)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Item with barcode {barcode} not found")))
    }

    /// Get ItemShort for many biblios (excludes archived). Used to attach items to BiblioShort lists.
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_get_items_short_by_biblio_ids(
        &self,
        biblio_ids: &[i64],
    ) -> AppResult<HashMap<i64, Vec<ItemShort>>> {
        if biblio_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<ItemShortRow> = sqlx::query_as(
            r#"
            SELECT i.biblio_id, i.id, i.barcode, i.call_number, i.borrowable,
                   so.name as source_name,
                   EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.biblio_id = ANY($1) AND i.archived_at IS NULL
            ORDER BY i.biblio_id, i.barcode
            "#,
        )
        .bind(biblio_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut map: HashMap<i64, Vec<ItemShort>> = HashMap::new();
        for row in rows {
            map.entry(row.biblio_id)
                .or_default()
                .push(ItemShort::from(row));
        }
        Ok(map)
    }

    /// Create an item (physical copy) for a biblio
    #[tracing::instrument(skip(self), err)]
    pub async fn biblios_create_item(&self, biblio_id: i64, item: &Item) -> AppResult<Item> {
        let now = Utc::now();
        let mut new_item = item.clone();
        let source_id = if let Some(id) = item.source_id {
            Some(id)
        } else if let Some(ref name) = item.source_name {
            Some(self.sources_find_or_create_by_name(name).await?)
        } else if let Some(default) = self.sources_get_default().await? {
            Some(default.id)
        } else {
            None
        };
        new_item.source_id = source_id;

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO items (
                biblio_id, barcode, call_number, volume_designation, place, borrowable, notes, price, source_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
            RETURNING id
            "#,
        )
        .bind(biblio_id)
        .bind(&item.barcode)
        .bind(&item.call_number)
        .bind(&item.volume_designation)
        .bind(&item.place)
        .bind(item.borrowable)
        .bind(&item.notes)
        .bind(&item.price)
        .bind(source_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        new_item.id = Some(id);
        Ok(new_item)
    }

    /// Upsert an item (physical copy)
    #[tracing::instrument(skip(self), err)]
    pub async fn upsert_item<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let now = Utc::now();
        item.updated_at = Some(now);
        if let Some(id) = item.id {
            sqlx::query(
                r#"
                UPDATE items SET
                    biblio_id = $1,
                    barcode = $2,
                    call_number = $3,
                    volume_designation = $4,
                    place = $5,
                    borrowable = $6,
                    notes = $7,
                    price = $8,
                    source_id = $9,
                    updated_at = $10,
                    archived_at = $11
                WHERE id = $12
                "#,
            )
            .bind(&item.biblio_id)
            .bind(&item.barcode)
            .bind(&item.call_number)
            .bind(&item.volume_designation)
            .bind(&item.place)
            .bind(item.borrowable)
            .bind(&item.notes)
            .bind(&item.price)
            .bind(&item.source_id)
            .bind(&item.updated_at)
            .bind(&item.archived_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        } else {
            if let Some(ref barcode) = item.barcode {
                let existing_id = sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM items WHERE barcode = $1",
                )
                .bind(barcode)
                .fetch_optional(&self.pool)
                .await?;
                item.id = existing_id;
            }

            if let Some(id) = item.id {
                sqlx::query(
                    r#"
                    UPDATE items SET
                        biblio_id = $1,
                        barcode = $2,
                        call_number = $3,
                        volume_designation = $4,
                        place = $5,
                        borrowable = $6,
                        notes = $7,
                        price = $8,
                        source_id = $9,
                        updated_at = $10,
                        archived_at = $11
                    WHERE id = $12
                    "#,
                )
                .bind(&item.biblio_id)
                .bind(&item.barcode)
                .bind(&item.call_number)
                .bind(&item.volume_designation)
                .bind(&item.place)
                .bind(item.borrowable)
                .bind(&item.notes)
                .bind(&item.price)
                .bind(&item.source_id)
                .bind(&item.updated_at)
                .bind(&item.archived_at)
                .bind(id)
                .execute(&self.pool)
                .await?;
            } else {
                let id = sqlx::query_scalar::<_, i64>(
                    r#"
                    INSERT INTO items (
                        biblio_id, barcode, call_number, volume_designation,
                        place, borrowable, notes, price, source_id, created_at, updated_at, archived_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11)
                    RETURNING id
                    "#,
                )
                .bind(&item.biblio_id)
                .bind(&item.barcode)
                .bind(&item.call_number)
                .bind(&item.volume_designation)
                .bind(&item.place)
                .bind(item.borrowable)
                .bind(&item.notes)
                .bind(&item.price)
                .bind(&item.source_id)
                .bind(&item.updated_at)
                .bind(&item.archived_at)
                .fetch_one(&self.pool)
                .await?;

                item.id = Some(id);
            }
        }
       
        Ok(item)
    }

    /// Update an item (physical copy)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_update<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let now = Utc::now();
        item.updated_at = Some(now);
        sqlx::query(
            r#"
            UPDATE items SET
                barcode = COALESCE($1, barcode),
                call_number = COALESCE($2, call_number),
                volume_designation = COALESCE($3, volume_designation),
                place = COALESCE($4, place),
                borrowable = COALESCE($5, borrowable),
                notes = COALESCE($6, notes),
                price = COALESCE($7, price),
                source_id = COALESCE($8, source_id),
                updated_at = $9,
                archived_at = $10
            WHERE id = $11
            "#
        )
        .bind(&item.barcode)
        .bind(&item.call_number)
        .bind(&item.volume_designation)
        .bind(&item.place)
        .bind(item.borrowable)
        .bind(&item.notes)
        .bind(&item.price)
        .bind(&item.source_id)
        .bind(&item.updated_at)
        .bind(&item.archived_at)
        .bind(item.id.unwrap_or(0))
        .execute(&self.pool)
        .await?;

        Ok(item)
    }

    /// Delete an item (physical copy — soft delete, sets archived_at)
    #[tracing::instrument(skip(self), err)]
    pub async fn items_delete(&self, id: i64, force: bool) -> AppResult<()> {
        let now = Utc::now();

        let borrowed = self.loans_count_active_for_item(id).await?;

        if borrowed > 0 {
            if !force {
                return Err(AppError::Conflict(
                    "Item is currently borrowed. Use force=true to delete anyway.".to_string(),
                ));
            }
            let loan_ids = self.loans_get_active_ids_for_item(id).await?;
            for loan_id in loan_ids {
                self.loans_return(loan_id).await?;
            }
        }

        self.holds_cancel_active_for_item(id).await?;

        sqlx::query(
            "UPDATE items SET archived_at = $1, updated_at = $1, barcode = CONCAT('ARCH_', id::text, '_', COALESCE(barcode, '')) WHERE id = $2 AND archived_at IS NULL"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if item barcode already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn items_barcode_exists(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_item_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM items WHERE barcode = $1 AND id != $2)")
                .bind(barcode)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM items WHERE barcode = $1)")
                .bind(barcode)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(exists)
    }

    /// Get item id and archived_at by barcode
    #[tracing::instrument(skip(self), err)]
    pub async fn items_get_by_barcode(&self, barcode: &str) -> AppResult<Option<(i64, bool)>> {
        let row: Option<(i64, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, archived_at FROM items WHERE barcode = $1",
        )
        .bind(barcode)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, archived_at)| (id, archived_at.is_some())))
    }

    /// Reactivate an archived item and update its fields.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_reactivate(
        &self,
        item_id: i64,
        biblio_id: i64,
        item: &Item,
    ) -> AppResult<Item> {
        let now = Utc::now();
        let source_id = if let Some(id) = item.source_id {
            Some(id)
        } else if let Some(ref name) = item.source_name {
            Some(self.sources_find_or_create_by_name(name).await?)
        } else if let Some(default) = self.sources_get_default().await? {
            Some(default.id)
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE items SET
                biblio_id = $1, barcode = $2, call_number = $3, volume_designation = $4,
                place = $5, borrowable = $6,
                notes = $7, price = $8, source_id = $9,
                archived_at = NULL,
                updated_at = $10
            WHERE id = $11
            "#,
        )
        .bind(biblio_id)
        .bind(&item.barcode)
        .bind(&item.call_number)
        .bind(&item.volume_designation)
        .bind(&item.place)
        .bind(item.borrowable)
        .bind(&item.notes)
        .bind(&item.price)
        .bind(source_id)
        .bind(now)
        .bind(item_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_as::<_, Item>(
            r#"
            SELECT i.id, i.biblio_id, i.source_id, i.barcode, i.call_number, i.volume_designation,
                   i.place, i.borrowable, i.circulation_status, i.notes, i.price,
                   i.created_at, i.updated_at, i.archived_at,
                   so.name as source_name,
                   EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed,
                   (SELECT l.id FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL ORDER BY l.id DESC LIMIT 1) as loan_id
            FROM items i
            LEFT JOIN sources so ON i.source_id = so.id
            WHERE i.id = $1
            "#,
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
    /// Find an existing item by barcode and return its short representation.
    #[tracing::instrument(skip(self), err)]
    pub async fn items_find_short_by_barcode(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<Option<ItemShort>> {
        let row: Option<ItemShortRow> = if let Some(eid) = exclude_item_id {
            sqlx::query_as(
                r#"
                SELECT i.biblio_id, i.id, i.barcode, i.call_number, i.borrowable,
                       so.name as source_name,
                       EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed
                FROM items i
                LEFT JOIN sources so ON i.source_id = so.id
                WHERE i.barcode = $1 AND i.id != $2 AND i.archived_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(barcode)
            .bind(eid)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT i.biblio_id, i.id, i.barcode, i.call_number, i.borrowable,
                       so.name as source_name,
                       EXISTS(SELECT 1 FROM loans l WHERE l.item_id = i.id AND l.returned_at IS NULL) as borrowed
                FROM items i
                LEFT JOIN sources so ON i.source_id = so.id
                WHERE i.barcode = $1 AND i.archived_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(barcode)
            .fetch_optional(&self.pool)
            .await?
        };
        Ok(row.map(ItemShort::from))
    }
}
