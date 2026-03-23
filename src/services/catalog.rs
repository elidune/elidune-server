//! Catalog management service

use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    models::{
        import_report::{ImportAction, ImportReport},
        biblio::{Biblio, BiblioQuery, BiblioShort},
        item::Item,
    },
    repository::BibliosRepository,
    services::search::{MeilisearchService, SearchFilters},
};

#[derive(Clone)]
pub struct CatalogService {
    repository: Arc<dyn BibliosRepository>,
    search: Option<Arc<MeilisearchService>>,
}

impl CatalogService {
    pub fn new(repository: Arc<dyn BibliosRepository>) -> Self {
        Self { repository, search: None }
    }

    pub fn with_search(repository: Arc<dyn BibliosRepository>, search: Arc<MeilisearchService>) -> Self {
        Self { repository, search: Some(search) }
    }

    // =========================================================================
    // Shared policy helpers
    // =========================================================================

    /// Check ISBN uniqueness among active biblios.
    /// Returns structured 409 error with `BiblioShort` if a duplicate is found.
    async fn ensure_isbn_unique(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<()> {
        if let Some(existing_id) = self.repository.biblios_find_active_by_isbn(isbn, exclude_id).await? {
            let existing_biblio = self.repository.biblios_get_short_by_id(existing_id).await?;
            return Err(AppError::DuplicateNeedsConfirmation {
                existing_id,
                existing_item: existing_biblio,
                message: format!(
                    "A biblio with ISBN {} already exists (id={}). \
                     Resend with confirm_replace_existing_id={} to merge it.",
                    isbn, existing_id, existing_id
                ),
            });
        }
        Ok(())
    }

    /// Check item barcode uniqueness (active and archived).
    /// Returns structured 409 error with `ItemShort` if a duplicate is found.
    async fn ensure_barcode_unique(&self, barcode: &str, exclude_item_id: Option<i64>) -> AppResult<()> {
        if let Some(existing) = self.repository.items_find_short_by_barcode(barcode, exclude_item_id).await? {
            return Err(AppError::DuplicateBarcodeNeedsConfirmation {
                existing_id: existing.id,
                existing_item: existing,
                message: format!("An item with barcode {} already exists.", barcode),
            });
        }
        Ok(())
    }

    /// Process embedded items (physical copies) through barcode policy, then upsert each one.
    async fn process_embedded_items(&self, biblio_id: i64, mut items: Vec<Item>) -> AppResult<Vec<Item>> {
        for item in &mut items {
            if let Some(ref barcode) = item.barcode {
                self.ensure_barcode_unique(barcode, item.id).await?;
            }
            item.biblio_id = Some(biblio_id);
            self.repository.upsert_item(item).await?;
        }
        Ok(items)
    }

    /// Fire-and-forget: push a fresh Meilisearch document for the given biblio.
    async fn sync_index(&self, id: i64) {
        if let Some(ref svc) = self.search {
            match self.repository.biblios_get_meili_document(id).await {
                Ok(Some(doc)) => svc.index_document(&doc).await,
                Ok(None) => {}
                Err(e) => tracing::warn!("sync_index: failed to build doc for id={}: {}", id, e),
            }
        }
    }

    /// Fire-and-forget: remove a document from the Meilisearch index.
    async fn sync_delete(&self, id: i64) {
        if let Some(ref svc) = self.search {
            svc.delete_document(id).await;
        }
    }

    // =========================================================================
    // Biblios
    // =========================================================================

    /// Search biblios.
    ///
    /// When `freesearch` is present and Meilisearch is available, delegates to
    /// Meilisearch for full-text search (typo tolerance, ranking) and loads the
    /// ordered `BiblioShort` rows from PostgreSQL. Falls back to the PostgreSQL path
    /// if Meilisearch is unavailable or not configured.
    #[tracing::instrument(skip(self), err)]
    pub async fn search_biblios(&self, query: &BiblioQuery) -> AppResult<(Vec<BiblioShort>, i64)> {
        if let (Some(ref fs), Some(ref svc)) = (query.freesearch.as_deref(), &self.search) {
            if !fs.trim().is_empty() {
                let filters = SearchFilters {
                    media_type: query.media_type.clone(),
                    lang: query.lang.clone(),
                    audience_type: query.audience_type.clone(),
                    archive: query.archive,
                };
                let page = query.page.unwrap_or(1).max(1);
                let per_page = query.per_page.unwrap_or(20).clamp(1, 200);

                match svc.search(fs, &filters, page, per_page).await {
                    Ok((ids, total)) => {
                        let biblios = self.repository.biblios_get_short_by_ids_ordered(&ids).await?;
                        return Ok((biblios, total));
                    }
                    Err(e) => {
                        tracing::warn!("Meilisearch search failed, falling back to PostgreSQL: {}", e);
                    }
                }
            }
        }

        self.repository.biblios_search(query).await
    }

    /// Get biblio by ID with full details
    #[tracing::instrument(skip(self), err)]
    pub async fn get_biblio(&self, id: i64) -> AppResult<Biblio> {
        self.repository
            .biblios_get_by_id_or_isbn(&id.to_string())
            .await
    }

    /// Create a new biblio with ISBN deduplication.
    ///
    /// - No duplicate ISBN among active biblios → create OK.
    /// - Duplicate found + `allow_duplicate_isbn` → create a second biblio.
    /// - Duplicate found + `confirm_replace_existing_id` matches → merge bibliographic data.
    /// - Duplicate found + no flag → 409 with existing `BiblioShort`.
    ///
    /// Embedded items (physical copies) are created through the barcode policy.
    #[tracing::instrument(skip(self), err)]
    pub async fn create_biblio(
        &self,
        mut biblio: Biblio,
        allow_duplicate_isbn: bool,
        confirm_replace_existing_id: Option<i64>,
    ) -> AppResult<(Biblio, ImportReport)> {
        if !allow_duplicate_isbn {
            if let Some(ref isbn) = biblio.isbn {
                if let Some(existing_id) = self.repository.biblios_find_active_by_isbn(isbn.as_str(), None).await? {
                    if confirm_replace_existing_id == Some(existing_id) {
                        tracing::info!("Catalog create: confirmed merge into biblio id={}", existing_id);
                        let pending = std::mem::take(&mut biblio.items);
                        self.repository.biblios_update(existing_id, &mut biblio).await?;
                        biblio.items = self.process_embedded_items(existing_id, pending).await?;
                        if !biblio.items.is_empty() {
                            self.repository.biblios_update_marc_record(&mut biblio).await?;
                        }
                        self.sync_index(existing_id).await;
                        let report = ImportReport {
                            action: ImportAction::MergedBibliographic,
                            existing_id: Some(existing_id),
                            warnings: vec![],
                            message: Some(format!(
                                "Merged bibliographic data into biblio id={} after confirmation.",
                                existing_id
                            )),
                        };
                        return Ok((biblio, report));
                    }

                    let existing_biblio = self.repository.biblios_get_short_by_id(existing_id).await?;
                    return Err(AppError::DuplicateNeedsConfirmation {
                        existing_id,
                        existing_item: existing_biblio,
                        message: format!(
                            "A biblio with ISBN {} already exists (id={}). \
                             Resend with confirm_replace_existing_id={} to merge it.",
                            isbn, existing_id, existing_id
                        ),
                    });
                }
            }
        }

        let mut warnings = Vec::new();
        if biblio.isbn.is_none() && !allow_duplicate_isbn {
            warnings.push("No ISBN — duplicate check skipped. This may create silent duplicates.".to_string());
        }

        let pending_items = std::mem::take(&mut biblio.items);
        self.repository.biblios_create(&mut biblio).await?;
        let biblio_id = biblio.id.unwrap();
        biblio.items = self.process_embedded_items(biblio_id, pending_items).await?;
        if !biblio.items.is_empty() {
            self.repository.biblios_update_marc_record(&mut biblio).await?;
        }
        self.sync_index(biblio_id).await;

        let report = ImportReport {
            action: ImportAction::Created,
            existing_id: None,
            warnings,
            message: None,
        };
        Ok((biblio, report))
    }

    /// Update an existing biblio.
    #[tracing::instrument(skip(self), err)]
    pub async fn update_biblio(&self, id: i64, mut biblio: Biblio, allow_duplicate_isbn: bool) -> AppResult<Biblio> {
        self.repository
            .biblios_get_by_id_or_isbn(&id.to_string())
            .await?;

        if !allow_duplicate_isbn {
            if let Some(ref isbn) = biblio.isbn {
                self.ensure_isbn_unique(isbn.as_str(), Some(id)).await?;
            }
        }

        let pending_items = std::mem::take(&mut biblio.items);
        self.repository.biblios_update(id, &mut biblio).await?;
        biblio.items = self.process_embedded_items(id, pending_items).await?;
        if !biblio.items.is_empty() {
            self.repository.biblios_update_marc_record(&mut biblio).await?;
        }
        self.sync_index(id).await;

        Ok(biblio)
    }

    /// Delete a biblio (soft delete)
    #[tracing::instrument(skip(self), err)]
    pub async fn delete_biblio(&self, id: i64, force: bool) -> AppResult<()> {
        self.repository.biblios_delete(id, force).await?;
        self.sync_delete(id).await;
        Ok(())
    }

    // =========================================================================
    // Items (physical copies)
    // =========================================================================

    /// Get items (physical copies) for a biblio
    #[tracing::instrument(skip(self), err)]
    pub async fn get_items(&self, biblio_id: i64) -> AppResult<Vec<Item>> {
        self.repository
            .biblios_get_by_id_or_isbn(&biblio_id.to_string())
            .await?;
        self.repository.biblios_get_items(biblio_id).await
    }

    /// Create an item (physical copy) for a biblio.
    /// Barcode uniqueness is enforced through the shared policy.
    #[tracing::instrument(skip(self), err)]
    pub async fn create_item(&self, biblio_id: i64, item: Item) -> AppResult<Item> {
        self.repository
            .biblios_get_by_id_or_isbn(&biblio_id.to_string())
            .await?;

        if let Some(ref barcode) = item.barcode {
            self.ensure_barcode_unique(barcode, None).await?;
        }

        let result = self.repository.biblios_create_item(biblio_id, &item).await?;
        self.sync_index(biblio_id).await;
        Ok(result)
    }

    /// Update an item (physical copy).
    #[tracing::instrument(skip(self), err)]
    pub async fn update_item<'a>(&self, biblio_id: i64, item: &'a mut Item) -> AppResult<&'a mut Item> {
        let item_id = item.id.ok_or_else(|| {
            AppError::NotFound("Item id is required".to_string())
        })?;

        self.repository
            .biblios_get_by_id_or_isbn(&biblio_id.to_string())
            .await?;

        let items = self.repository.biblios_get_items(biblio_id).await?;
        if !items.iter().any(|i| i.id == Some(item_id)) {
            return Err(AppError::NotFound(
                format!("Item {} not found for biblio {}", item_id, biblio_id),
            ));
        }

        if let Some(ref barcode) = item.barcode {
            self.ensure_barcode_unique(barcode, Some(item_id)).await?;
        }

        let result = self.repository.items_update(item).await?;
        self.sync_index(biblio_id).await;
        Ok(result)
    }

    /// Delete an item (physical copy)
    #[tracing::instrument(skip(self), err)]
    pub async fn delete_item(&self, biblio_id: i64, item_id: i64, force: bool) -> AppResult<()> {
        self.repository
            .items_delete(item_id, force)
            .await?;
        self.sync_index(biblio_id).await;
        Ok(())
    }

    /// List all biblios in a series (ordered by volume number)
    #[tracing::instrument(skip(self), err)]
    pub async fn get_biblios_by_series(&self, series_id: i64) -> AppResult<Vec<BiblioShort>> {
        self.repository.biblios_get_by_series(series_id).await
    }

    // =========================================================================
    // Admin / reindex
    // =========================================================================

    /// Trigger a full reindex of all catalog biblios in Meilisearch.
    /// Returns `(total_biblios_queued, bool_meilisearch_available)`.
    #[tracing::instrument(skip(self), err)]
    pub async fn reindex_search(&self) -> AppResult<(usize, bool)> {
        let Some(ref svc) = self.search else {
            return Ok((0, false));
        };
        let docs = self.repository.biblios_get_all_meili_documents().await?;
        let count = docs.len();
        svc.reindex_all(docs).await;
        Ok((count, true))
    }
}
