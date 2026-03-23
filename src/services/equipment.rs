//! Equipment service

use std::sync::Arc;

use crate::{
    error::AppResult,
    models::equipment::{CreateEquipment, Equipment, UpdateEquipment},
    repository::EquipmentRepository,
};

#[derive(Clone)]
pub struct EquipmentService {
    repository: Arc<dyn EquipmentRepository>,
}

impl EquipmentService {
    pub fn new(repository: Arc<dyn EquipmentRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list(&self) -> AppResult<Vec<Equipment>> {
        self.repository.equipment_list().await
    }

    pub async fn get_by_id(&self, id: i64) -> AppResult<Equipment> {
        self.repository.equipment_get_by_id(id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn create(&self, data: &CreateEquipment) -> AppResult<Equipment> {
        self.repository.equipment_create(data).await
    }

    pub async fn update(&self, id: i64, data: &UpdateEquipment) -> AppResult<Equipment> {
        self.repository
            .equipment_update_equipment(id, data)
            .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repository.equipment_delete(id).await
    }

    /// Count public internet stations (for stats)
    #[tracing::instrument(skip(self), err)]
    pub async fn count_public_internet_stations(&self) -> AppResult<i64> {
        self.repository
            .equipment_count_public_internet_stations()
            .await
    }

    /// Count public devices - tablets and ereaders (for stats)
    #[tracing::instrument(skip(self), err)]
    pub async fn count_public_devices(&self) -> AppResult<i64> {
        self.repository
            .equipment_count_public_devices()
            .await
    }
}
