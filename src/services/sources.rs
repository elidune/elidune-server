//! Sources service

use crate::{
    error::{AppError, AppResult},
    models::source::{CreateSource, MergeSources, Source, UpdateSource},
    repository::Repository,
};

#[derive(Clone)]
pub struct SourcesService {
    repository: Repository,
}

impl SourcesService {
    pub fn new(repository: Repository) -> Self {
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

    /// Archive a source (fails if non-archived specimens are linked)
    pub async fn archive(&self, id: i64) -> AppResult<Source> {
        // Verify source exists
        let source = self.repository.sources_get_by_id(id).await?;

        // Check if already archived
        if source.is_archive == Some(1) {
            return Err(AppError::BusinessRule(
                "Source is already archived".to_string(),
            ));
        }

        // Check for non-archived specimens
        let active_count = self
            .repository
            .sources_count_active_specimens(id)
            .await?;
        if active_count > 0 {
            return Err(AppError::BusinessRule(format!(
                "Cannot archive source: {} non-archived specimen(s) still linked",
                active_count
            )));
        }

        self.repository.sources_archive(id).await
    }

    /// Merge multiple sources into a new one
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

        // Verify all source IDs exist
        for &id in &data.source_ids {
            self.repository.sources_get_by_id(id).await?;
        }

        // Create new source
        let new_source = self
            .repository
            .sources_create(data.name.trim(), None)
            .await?;

        // Reassign specimens and items to the new source
        self.repository
            .sources_reassign_specimens(&data.source_ids, new_source.id)
            .await?;
        self.repository
            .sources_reassign_items(&data.source_ids, new_source.id)
            .await?;

        // Archive old sources
        self.repository
            .sources_archive_many(&data.source_ids)
            .await?;

        Ok(new_source)
    }
}
