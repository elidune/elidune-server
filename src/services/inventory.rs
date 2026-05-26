//! Inventory / stocktaking service

use std::sync::Arc;

use crate::{
    email::EmailService,
    error::{AppError, AppResult},
    inventory_email,
    models::inventory::{
        InventoryConsolidationEmailError, InventoryConsolidationPreview,
        InventoryConsolidationResult, InventoryConsolidationSkipped, InventoryMissingRow,
        InventoryReport, InventoryScan, InventorySession, InventoryStatus,
    },
    repository::InventoryRepository,
    services::catalog::CatalogService,
};

/// Maximum barcodes accepted per `POST .../scans/batch` request.
pub const INVENTORY_BATCH_MAX_BARCODES: usize = 500;

#[derive(Clone)]
pub struct InventoryService {
    repository: Arc<dyn InventoryRepository>,
    catalog: CatalogService,
    email: EmailService,
}

impl InventoryService {
    pub fn new(
        repository: Arc<dyn InventoryRepository>,
        catalog: CatalogService,
        email: EmailService,
    ) -> Self {
        Self {
            repository,
            catalog,
            email,
        }
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

    fn ensure_consolidation_eligible(&self, session: &InventorySession) -> AppResult<()> {
        if session.status != InventoryStatus::Closed {
            return Err(AppError::BadRequest(
                "Session must be closed before consolidation".to_string(),
            ));
        }
        if session.consolidated_at.is_some() {
            return Err(AppError::Conflict(
                "Session has already been consolidated".to_string(),
            ));
        }
        Ok(())
    }

    /// Preview copies that would be archived and side effects (loans, orphan biblios).
    #[tracing::instrument(skip(self), err)]
    pub async fn consolidation_preview(
        &self,
        session_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<InventoryConsolidationPreview> {
        let session = self.repository.inventory_get_session(session_id).await?;
        self.ensure_consolidation_eligible(&session)?;

        let page = page.max(1);
        let per_page = per_page.clamp(1, 200);
        let summary = self
            .repository
            .inventory_consolidation_preview_summary(session_id)
            .await?;
        let (items, total) = self
            .repository
            .inventory_consolidation_preview_page(session_id, page, per_page)
            .await?;
        let page_count = if total == 0 {
            0
        } else {
            (total + per_page - 1) / per_page
        };

        Ok(InventoryConsolidationPreview {
            session_id,
            summary,
            items,
            total,
            page,
            per_page,
            page_count,
        })
    }

    /// Archive all in-scope copies missing from scans in a closed session.
    ///
    /// When `force` is false, copies with active loans are skipped and the session
    /// is **not** marked consolidated so staff can retry with `force: true`.
    /// Orphan bibliographic records (no active copies left) are archived automatically.
    /// When `force` is true, readers with closed loans receive an email notification.
    #[tracing::instrument(skip(self), err)]
    pub async fn consolidate_session(
        &self,
        session_id: i64,
        consolidated_by: Option<i64>,
        force: bool,
    ) -> AppResult<InventoryConsolidationResult> {
        let session = self.repository.inventory_get_session(session_id).await?;
        self.ensure_consolidation_eligible(&session)?;

        let loan_closures = if force {
            self.repository
                .inventory_list_loan_closures_for_missing(session_id)
                .await?
        } else {
            Vec::new()
        };

        let missing_ids = self
            .repository
            .inventory_list_missing_item_ids(session_id)
            .await?;

        let attempted = missing_ids.len() as i64;
        let mut deleted = 0i64;
        let mut skipped = Vec::new();
        let mut archived_biblios = 0i64;

        for item_id in missing_ids {
            match self.catalog.delete_item(item_id, force).await {
                Ok(biblio_id) => {
                    deleted += 1;
                    if self.catalog.archive_biblio_if_orphan(biblio_id).await? {
                        archived_biblios += 1;
                    }
                }
                Err(AppError::Conflict(reason)) if !force => {
                    skipped.push(InventoryConsolidationSkipped {
                        item_id,
                        reason,
                    });
                }
                Err(e) => return Err(e),
            }
        }

        let consolidated = if skipped.is_empty() {
            self.repository
                .inventory_mark_consolidated(session_id, consolidated_by)
                .await?;
            true
        } else {
            false
        };

        let (loan_closure_emails_sent, loan_closure_email_errors) =
            if force && consolidated && !loan_closures.is_empty() {
                let (sent, errors) = inventory_email::send_loan_closure_notifications(
                    &self.email,
                    &session.name,
                    &loan_closures,
                )
                .await;
                let mapped = errors
                    .into_iter()
                    .map(|(user_id, email, error_message)| InventoryConsolidationEmailError {
                        user_id,
                        email,
                        error_message,
                    })
                    .collect();
                (sent, mapped)
            } else {
                (0, Vec::new())
            };

        Ok(InventoryConsolidationResult {
            session_id,
            attempted,
            deleted,
            skipped,
            consolidated,
            archived_biblios,
            loan_closure_emails_sent,
            loan_closure_email_errors,
        })
    }
}
