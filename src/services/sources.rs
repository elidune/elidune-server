//! Sources service

use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::source::{CreateSource, MergeSources, Source, UpdateSource},
    repository::SourcesRepository,
};

#[derive(Clone)]
pub struct SourcesService {
    repository: Arc<dyn SourcesRepository>,
}

impl SourcesService {
    pub fn new(repository: Arc<dyn SourcesRepository>) -> Self {
        Self { repository }
    }

    /// List sources
    pub async fn list(&self, include_archived: bool) -> AppResult<Vec<Source>> {
        self.repository.sources_list(include_archived).await
    }

    /// Get source by ID
    pub async fn get_by_id(&self, id: i64) -> AppResult<Source> {
        self.repository.sources_get_by_id(id).await
    }

    /// Create a source
    pub async fn create(&self, data: &CreateSource) -> AppResult<Source> {
        let name = data.name.trim();
        if name.is_empty() {
            return Err(AppError::Validation("Source name cannot be empty".to_string()));
        }
        self.repository.sources_create(name, data.default).await
    }

    /// Rename a source
    pub async fn rename(&self, id: i64, name: &str) -> AppResult<Source> {
        if name.trim().is_empty() {
            return Err(AppError::Validation("Source name cannot be empty".to_string()));
        }
        self.repository
            .sources_rename(id, name.trim())
            .await
    }

    /// Update a source (name and/or default status)
    pub async fn update(&self, id: i64, data: &UpdateSource) -> AppResult<Source> {
        // Validate name if provided
        if let Some(ref name) = data.name {
            if name.trim().is_empty() {
                return Err(AppError::Validation("Source name cannot be empty".to_string()));
            }
        }

        self.repository
            .sources_update(id, data.name.as_deref(), data.default)
            .await
    }

    /// Archive a source (fails if non-archived items are linked)
    pub async fn archive(&self, id: i64) -> AppResult<Source> {
        // Verify source exists
        let source = self.repository.sources_get_by_id(id).await?;

        // Check if already archived
        if source.is_archive == Some(1) {
            return Err(AppError::BusinessRule(
                "Source is already archived".to_string(),
            ));
        }

        // Check for non-archived items
        let active_count = self
            .repository
            .sources_count_active_items(id)
            .await?;
        if active_count > 0 {
            return Err(AppError::BusinessRule(format!(
                "Cannot archive source: {} non-archived item(s) still linked",
                active_count
            )));
        }

        self.repository.sources_archive(id).await
    }

    /// Merge multiple sources into a new one.
    ///
    /// All three writes (create, reassign, archive) run inside a single transaction so
    /// a failure cannot leave items pointing at a non-existent or wrong source.
    pub async fn merge(&self, data: &MergeSources) -> AppResult<Source> {
        if data.name.trim().is_empty() {
            return Err(AppError::Validation(
                "Merged source name cannot be empty".to_string(),
            ));
        }
        if data.source_ids.len() < 2 {
            return Err(AppError::Validation(
                "At least 2 source IDs are required for merge".to_string(),
            ));
        }

        // Verify all source IDs exist before opening the transaction.
        for &id in &data.source_ids {
            self.repository.sources_get_by_id(id).await?;
        }

        let name = data.name.trim();
        let old_ids = &data.source_ids;

        self.repository.merge_sources_tx(name, old_ids).await
    }
}
