//! Inventory / stocktaking service

use std::sync::Arc;

use crate::{
    email::EmailService,
    error::{AppError, AppResult},
    inventory_email,
    models::inventory::{
        CreateInventorySessionResponse, InventoryConsolidationEmailError,
        InventoryConsolidationPreview, InventoryConsolidationResult, InventoryConsolidationSkipped,
        InventoryMissingRow, InventoryReport, InventoryScan, InventorySession, InventoryStatus,
    },
    repository::{InventoryRepository, SourcesRepository},
    services::{
        audit::AuditService,
        catalog::CatalogService,
        task_manager::TaskHandle,
    },
};

/// Maximum barcodes accepted per `POST .../scans/batch` request.
pub const INVENTORY_BATCH_MAX_BARCODES: usize = 500;

const WARN_EMPTY_SCOPE: &str =
    "No active copies match the selected scope (source and/or place); verify catalog data before scanning.";

#[derive(Clone)]
pub struct InventoryService {
    repository: Arc<dyn InventoryRepository>,
    sources: Arc<dyn SourcesRepository>,
    catalog: CatalogService,
    email: EmailService,
    audit: AuditService,
}

impl InventoryService {
    pub fn new(
        repository: Arc<dyn InventoryRepository>,
        sources: Arc<dyn SourcesRepository>,
        catalog: CatalogService,
        email: EmailService,
        audit: AuditService,
    ) -> Self {
        Self {
            repository,
            sources,
            catalog,
            email,
            audit,
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
        scope_source_id: Option<i64>,
        created_by: Option<i64>,
    ) -> AppResult<CreateInventorySessionResponse> {
        if let Some(source_id) = scope_source_id {
            let source = self.sources.sources_get_by_id(source_id).await?;
            if source.is_archive.unwrap_or(0) != 0 || source.archived_at.is_some() {
                return Err(AppError::Validation(
                    "Cannot open inventory for an archived source".to_string(),
                ));
            }
        }

        if self
            .repository
            .inventory_has_open_session_for_scope(scope_source_id, scope_place)
            .await?
        {
            return Err(AppError::Conflict(
                "An open inventory session already exists for this source and place scope".to_string(),
            ));
        }

        let expected_in_scope = self
            .repository
            .inventory_count_expected_in_scope(scope_source_id, scope_place)
            .await?;

        let mut warnings = Vec::new();
        if expected_in_scope == 0 {
            warnings.push(WARN_EMPTY_SCOPE.to_string());
        }

        let session = self
            .repository
            .inventory_create_session(
                name,
                location_filter,
                notes,
                scope_place,
                scope_source_id,
                created_by,
            )
            .await?;

        Ok(CreateInventorySessionResponse {
            session,
            warnings,
            expected_in_scope,
        })
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
    #[tracing::instrument(skip(self, task), err)]
    pub async fn consolidate_session(
        &self,
        session_id: i64,
        consolidated_by: Option<i64>,
        force: bool,
        task: Option<TaskHandle>,
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

        let total = missing_ids.len();
        let attempted = total as i64;
        let mut deleted = 0i64;
        let mut skipped = Vec::new();
        let mut archived_biblios = 0i64;

        if let Some(ref handle) = task {
            handle
                .set_progress(
                    0,
                    total,
                    Some(serde_json::json!({
                        "sessionId": session_id.to_string(),
                        "phase": "archiving",
                        "attempted": attempted,
                    })),
                )
                .await;
        }

        for (index, item_id) in missing_ids.into_iter().enumerate() {
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

            if let Some(ref handle) = task {
                handle
                    .set_progress(
                        index + 1,
                        total,
                        Some(serde_json::json!({
                            "sessionId": session_id.to_string(),
                            "phase": "archiving",
                            "deleted": deleted,
                            "skipped": skipped.len(),
                            "archivedBiblios": archived_biblios,
                        })),
                    )
                    .await;
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
                if let Some(ref handle) = task {
                    handle
                        .set_progress(
                            total,
                            total,
                            Some(serde_json::json!({
                                "sessionId": session_id.to_string(),
                                "phase": "notifying_readers",
                                "recipientCount": loan_closures.len(),
                            })),
                        )
                        .await;
                }
                let (sent, errors) = inventory_email::send_loan_closure_notifications(
                    &self.email,
                    &self.audit,
                    session_id,
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
