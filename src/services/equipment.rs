//! Equipment service

use crate::{
    error::AppResult,
    models::equipment::{CreateEquipment, Equipment, UpdateEquipment},
    repository::Repository,
};

#[derive(Clone)]
pub struct EquipmentService {
    repository: Repository,
}

impl EquipmentService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    pub async fn list(&self) -> AppResult<Vec<Equipment>> {
        self.repository.equipment_list().await
    }

    pub async fn get_by_id(&self, id: i64) -> AppResult<Equipment> {
        self.repository.equipment_get_by_id(id).await
    }

    pub async fn create(&self, data: &CreateEquipment) -> AppResult<Equipment> {
        self.repository.equipment_create(data).await
    }

    pub async fn update(&self, id: i64, data: &UpdateEquipment) -> AppResult<Equipment> {
        self.repository
            .equipment_update_equipment(id, data)
            .await
    }

    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repository.equipment_delete(id).await
    }

    /// Count public internet stations (for stats)
    pub async fn count_public_internet_stations(&self) -> AppResult<i64> {
        self.repository
            .equipment_count_public_internet_stations()
            .await
    }

    /// Count public devices - tablets and ereaders (for stats)
    pub async fn count_public_devices(&self) -> AppResult<i64> {
        self.repository
            .equipment_count_public_devices()
            .await
    }
}
