//! Inventory / stocktaking service

use std::sync::Arc;

use crate::{
    error::AppResult,
    models::inventory::{InventoryReport, InventorySession, InventoryScan},
    repository::InventoryRepository,
};

#[derive(Clone)]
pub struct InventoryService {
    repository: Arc<dyn InventoryRepository>,
}

impl InventoryService {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list_sessions(&self) -> AppResult<Vec<InventorySession>> {
        self.repository.inventory_list_sessions().await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_session(&self, id: i64) -> AppResult<InventorySession> {
        self.repository.inventory_get_session(id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn create_session(
        &self,
        name: &str,
        location_filter: Option<&str>,
        notes: Option<&str>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        self.repository
            .inventory_create_session(name, location_filter, notes, created_by)
            .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn close_session(&self, id: i64) -> AppResult<InventorySession> {
        self.repository.inventory_close_session(id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn scan_barcode(
        &self,
        session_id: i64,
        barcode: &str,
    ) -> AppResult<InventoryScan> {
        self.repository.inventory_scan_barcode(session_id, barcode).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list_scans(&self, session_id: i64) -> AppResult<Vec<InventoryScan>> {
        self.repository.inventory_list_scans(session_id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn report(&self, session_id: i64) -> AppResult<InventoryReport> {
        self.repository.inventory_report(session_id).await
    }
}
