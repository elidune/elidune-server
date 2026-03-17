//! Catalog management service

use crate::{
    error::{AppError, AppResult},
    models::{
        import_report::{ImportAction, ImportReport},
        item::{Item, ItemQuery, ItemShort},
        specimen::Specimen,
    },
    repository::Repository,
};

#[derive(Clone)]
pub struct CatalogService {
    repository: Repository,
}

impl CatalogService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    // =========================================================================
    // Shared policy helpers
    // =========================================================================

    /// Check ISBN uniqueness among active items.
    /// Returns structured 409 error with `ItemShort` if a duplicate is found.
    async fn ensure_isbn_unique(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<()> {
        if let Some(existing_id) = self.repository.items_find_active_by_isbn(isbn, exclude_id).await? {
            let existing_item = self.repository.items_get_short_by_id(existing_id).await?;
            return Err(AppError::DuplicateNeedsConfirmation {
                existing_id,
                existing_item,
                message: format!(
                    "An item with ISBN {} already exists (id={}). \
                     Resend with confirm_replace_existing_id={} to merge it.",
                    isbn, existing_id, existing_id
                ),
            });
        }
        Ok(())
    }

    /// Check specimen barcode uniqueness (active and archived).
    /// Returns structured 409 error with `SpecimenShort` if a duplicate is found.
    async fn ensure_barcode_unique(&self, barcode: &str, exclude_specimen_id: Option<i64>) -> AppResult<()> {
        if let Some(existing) = self.repository.items_find_specimen_short_by_barcode(barcode, exclude_specimen_id).await? {
            return Err(AppError::DuplicateBarcodeNeedsConfirmation {
                existing_id: existing.id,
                existing_specimen: existing,
                message: format!("A specimen with barcode {} already exists.", barcode),
            });
        }
        Ok(())
    }

    /// Process embedded specimens through barcode policy, then upsert each one.
    async fn process_embedded_specimens(&self, item_id: i64, mut specimens: Vec<Specimen>) -> AppResult<Vec<Specimen>> {
        for specimen in &mut specimens {
            if let Some(ref barcode) = specimen.barcode {
                self.ensure_barcode_unique(barcode, specimen.id).await?;
            }
            specimen.item_id = Some(item_id);
            self.repository.upsert_specimen(specimen).await?;
        }
        Ok(specimens)
    }

    // =========================================================================
    // Items
    // =========================================================================

    /// Search items with filters
    pub async fn search_items(&self, query: &ItemQuery) -> AppResult<(Vec<ItemShort>, i64)> {
        self.repository.items_search(query).await
    }

    /// Get item by ID with full details
    pub async fn get_item(&self, id: i64) -> AppResult<Item> {
        self.repository
            .items_get_by_id_or_isbn(&id.to_string())
            .await
    }

    /// Create a new item with ISBN deduplication.
    ///
    /// - No duplicate ISBN among active items → create OK.
    /// - Duplicate found + `allow_duplicate_isbn` → create a second item.
    /// - Duplicate found + `confirm_replace_existing_id` matches → merge bibliographic data.
    /// - Duplicate found + no flag → 409 with existing `ItemShort`.
    ///
    /// Embedded specimens are created through the barcode policy.
    pub async fn create_item(
        &self,
        mut item: Item,
        allow_duplicate_isbn: bool,
        confirm_replace_existing_id: Option<i64>,
    ) -> AppResult<(Item, ImportReport)> {
        if !allow_duplicate_isbn {
            if let Some(ref isbn) = item.isbn {
                if let Some(existing_id) = self.repository.items_find_active_by_isbn(isbn.as_str(), None).await? {
                    if confirm_replace_existing_id == Some(existing_id) {
                        tracing::info!("Catalog create: confirmed merge into item id={}", existing_id);
                        let pending = std::mem::take(&mut item.specimens);
                        self.repository.items_update(existing_id, &mut item).await?;
                        item.specimens = self.process_embedded_specimens(existing_id, pending).await?;
                        if !item.specimens.is_empty() {
                            self.repository.items_update_marc_record_for_item(&mut item).await?;
                        }
                        let report = ImportReport {
                            action: ImportAction::MergedBibliographic,
                            existing_id: Some(existing_id),
                            warnings: vec![],
                            message: Some(format!(
                                "Merged bibliographic data into item id={} after confirmation.",
                                existing_id
                            )),
                        };
                        return Ok((item, report));
                    }

                    let existing_item = self.repository.items_get_short_by_id(existing_id).await?;
                    return Err(AppError::DuplicateNeedsConfirmation {
                        existing_id,
                        existing_item,
                        message: format!(
                            "An item with ISBN {} already exists (id={}). \
                             Resend with confirm_replace_existing_id={} to merge it.",
                            isbn, existing_id, existing_id
                        ),
                    });
                }
            }
        }

        let mut warnings = Vec::new();
        if item.isbn.is_none() && !allow_duplicate_isbn {
            warnings.push("No ISBN — duplicate check skipped. This may create silent duplicates.".to_string());
        }

        let pending_specimens = std::mem::take(&mut item.specimens);
        self.repository.items_create(&mut item).await?;
        let item_id = item.id.unwrap();
        item.specimens = self.process_embedded_specimens(item_id, pending_specimens).await?;
        if !item.specimens.is_empty() {
            self.repository.items_update_marc_record_for_item(&mut item).await?;
        }

        let report = ImportReport {
            action: ImportAction::Created,
            existing_id: None,
            warnings,
            message: None,
        };
        Ok((item, report))
    }

    /// Update an existing item.
    /// ISBN uniqueness check returns the same structured 409 as create.
    /// Embedded specimens are processed through the barcode policy.
    pub async fn update_item(&self, id: i64, mut item: Item, allow_duplicate_isbn: bool) -> AppResult<Item> {
        self.repository
            .items_get_by_id_or_isbn(&id.to_string())
            .await?;

        if !allow_duplicate_isbn {
            if let Some(ref isbn) = item.isbn {
                self.ensure_isbn_unique(isbn.as_str(), Some(id)).await?;
            }
        }

        let pending_specimens = std::mem::take(&mut item.specimens);
        self.repository.items_update(id, &mut item).await?;
        item.specimens = self.process_embedded_specimens(id, pending_specimens).await?;
        if !item.specimens.is_empty() {
            self.repository.items_update_marc_record_for_item(&mut item).await?;
        }

        Ok(item)
    }

    /// Delete an item
    pub async fn delete_item(&self, id: i64, force: bool) -> AppResult<()> {
        self.repository.items_delete(id, force).await
    }

    // =========================================================================
    // Specimens
    // =========================================================================

    /// Get specimens for an item
    pub async fn get_specimens(&self, item_id: i64) -> AppResult<Vec<Specimen>> {
        self.repository
            .items_get_by_id_or_isbn(&item_id.to_string())
            .await?;
        self.repository.items_get_specimens(item_id).await
    }

    /// Create a specimen for an item.
    /// Barcode uniqueness is enforced through the shared policy.
    pub async fn create_specimen(&self, item_id: i64, specimen: Specimen) -> AppResult<Specimen> {
        self.repository
            .items_get_by_id_or_isbn(&item_id.to_string())
            .await?;

        if let Some(ref barcode) = specimen.barcode {
            self.ensure_barcode_unique(barcode, None).await?;
        }

        self.repository
            .items_create_specimen(item_id, &specimen)
            .await
    }

    /// Update a specimen.
    /// Barcode uniqueness is enforced through the shared policy.
    pub async fn update_specimen<'a>(&self, item_id: i64, specimen: &'a mut Specimen) -> AppResult<&'a mut Specimen> {
        let specimen_id = specimen.id.ok_or_else(|| {
            AppError::NotFound("Specimen id is required".to_string())
        })?;

        self.repository
            .items_get_by_id_or_isbn(&item_id.to_string())
            .await?;

        let specimens = self.repository.items_get_specimens(item_id).await?;
        if !specimens.iter().any(|s| s.id == Some(specimen_id)) {
            return Err(AppError::NotFound(
                format!("Specimen {} not found for item {}", specimen_id, item_id),
            ));
        }

        if let Some(ref barcode) = specimen.barcode {
            self.ensure_barcode_unique(barcode, Some(specimen_id)).await?;
        }

        self.repository.items_update_specimen(specimen).await
    }

    /// Delete a specimen
    pub async fn delete_specimen(&self, _item_id: i64, specimen_id: i64, force: bool) -> AppResult<()> {
        self.repository
            .items_delete_specimen(specimen_id, force)
            .await
    }

    /// List all items in a series (ordered by volume number)
    pub async fn get_items_by_series(&self, series_id: i64) -> AppResult<Vec<ItemShort>> {
        self.repository
            .items_get_by_series(series_id)
            .await
    }
}
