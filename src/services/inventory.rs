//! Inventory / stocktaking service

use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::inventory::{
        InventoryMissingRow, InventoryReport, InventoryScan, InventorySession, InventoryStatus,
    },
    repository::InventoryRepository,
};

/// Maximum barcodes accepted per `POST .../scans/batch` request.
pub const INVENTORY_BATCH_MAX_BARCODES: usize = 500;

#[derive(Clone)]
pub struct InventoryService {
    repository: Arc<dyn InventoryRepository>,
}

impl InventoryService {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list_sessions_page(
        &self,
        page: i64,
        per_page: i64,
        status: Option<InventoryStatus>,
    ) -> AppResult<(Vec<InventorySession>, i64)> {
        self.repository
            .inventory_list_sessions_page(page, per_page, status)
            .await
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
        scope_place: Option<i16>,
        created_by: Option<i64>,
    ) -> AppResult<InventorySession> {
        self.repository
            .inventory_create_session(name, location_filter, notes, scope_place, created_by)
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
        scanned_by: Option<i64>,
    ) -> AppResult<InventoryScan> {
        self.repository
            .inventory_scan_barcode(session_id, barcode, scanned_by)
            .await
    }

    /// Record many barcodes (open session only — validate at API layer).
    #[tracing::instrument(skip(self, barcodes), err)]
    pub async fn scan_barcodes_batch(
        &self,
        session_id: i64,
        barcodes: &[String],
        scanned_by: Option<i64>,
    ) -> AppResult<Vec<InventoryScan>> {
        if barcodes.len() > INVENTORY_BATCH_MAX_BARCODES {
            return Err(AppError::Validation(format!(
                "At most {} barcodes per batch",
                INVENTORY_BATCH_MAX_BARCODES
            )));
        }
        let mut out = Vec::with_capacity(barcodes.len());
        for b in barcodes {
            let scan = self
                .repository
                .inventory_scan_barcode(session_id, b, scanned_by)
                .await?;
            out.push(scan);
        }
        Ok(out)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list_scans_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryScan>, i64)> {
        self.repository.inventory_get_session(session_id).await?;
        self.repository
            .inventory_list_scans_page(session_id, page, per_page)
            .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list_missing_page(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<InventoryMissingRow>, i64)> {
        self.repository.inventory_get_session(session_id).await?;
        self.repository
            .inventory_list_missing_page(session_id, page, per_page)
            .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn report(&self, session_id: i64) -> AppResult<InventoryReport> {
        self.repository.inventory_get_session(session_id).await?;
        self.repository.inventory_report(session_id).await
    }
}
