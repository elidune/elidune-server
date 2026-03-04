//! Equipment domain methods on Repository

use chrono::Utc;
use sqlx::{Pool, Postgres};

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::equipment::{CreateEquipment, Equipment, UpdateEquipment},
};

impl Repository {
    /// List all equipment
    pub async fn equipment_list(&self) -> AppResult<Vec<Equipment>> {
        let rows = sqlx::query_as::<_, Equipment>(
            "SELECT * FROM equipment ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get equipment by ID
    pub async fn equipment_get_by_id(&self, id: i64) -> AppResult<Equipment> {
        sqlx::query_as::<_, Equipment>("SELECT * FROM equipment WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Equipment {} not found", id)))
    }

    /// Create equipment
    pub async fn equipment_create(&self, data: &CreateEquipment) -> AppResult<Equipment> {
        let row = sqlx::query_as::<_, Equipment>(
            r#"
            INSERT INTO equipment (name, equipment_type, has_internet, is_public, quantity, notes)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(&data.name)
        .bind(data.equipment_type.unwrap_or(0))
        .bind(data.has_internet)
        .bind(data.is_public)
        .bind(data.quantity)
        .bind(&data.notes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Update equipment
    pub async fn equipment_update_equipment(&self, id: i64, data: &UpdateEquipment) -> AppResult<Equipment> {
        let now = Utc::now();
        let mut sets = vec!["modif_date = $1".to_string()];
        let mut idx = 2;

        macro_rules! add_field {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    sets.push(format!("{} = ${}", $name, idx));
                    idx += 1;
                }
            };
        }

        add_field!(data.name, "name");
        add_field!(data.equipment_type, "equipment_type");
        add_field!(data.has_internet, "has_internet");
        add_field!(data.is_public, "is_public");
        add_field!(data.quantity, "quantity");
        add_field!(data.status, "status");
        add_field!(data.notes, "notes");

        let query = format!("UPDATE equipment SET {} WHERE id = {} RETURNING *", sets.join(", "), id);

        let mut builder = sqlx::query_as::<_, Equipment>(&query).bind(now);

        macro_rules! bind_field {
            ($field:expr) => {
                if let Some(ref val) = $field {
                    builder = builder.bind(val);
                }
            };
        }

        bind_field!(data.name);
        bind_field!(data.equipment_type);
        bind_field!(data.has_internet);
        bind_field!(data.is_public);
        bind_field!(data.quantity);
        bind_field!(data.status);
        bind_field!(data.notes);

        builder
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Equipment {} not found", id)))
    }

    /// Delete equipment
    pub async fn equipment_delete(&self, id: i64) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM equipment WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Equipment {} not found", id)));
        }
        Ok(())
    }

    /// Count public equipment with internet access (for stats)
    pub async fn equipment_count_public_internet_stations(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(quantity), 0)::bigint FROM equipment
            WHERE is_public = TRUE AND has_internet = TRUE
              AND (status IS NULL OR status = 0)
            "#
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Count public tablets/ereaders (for stats)
    pub async fn equipment_count_public_devices(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(quantity), 0)::bigint FROM equipment
            WHERE is_public = TRUE
              AND equipment_type IN (1, 2)
              AND (status IS NULL OR status = 0)
            "#
        )
        .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}

