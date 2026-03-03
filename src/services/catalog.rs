//! Catalog management service

use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
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

    /// Search items with filters
    pub async fn search_items(&self, query: &ItemQuery) -> AppResult<(Vec<ItemShort>, i64)> {
        self.repository.items_search(query).await
    }

    /// Get item by ID with full details
    pub async fn get_item(&self, id: i64) -> AppResult<Item> {
        self.repository.items_get_by_id_or_isbn(&id.to_string()).await
    }



    /// Create a new item with ISBN deduplication.
    /// If `allow_duplicate_isbn` is true, creation is allowed even when another item has the same ISBN.
    /// If `confirm_replace_existing_id` matches a duplicate, replaces it.
    pub async fn create_item(
        &self,
        mut item: Item,
        source_id: Option<i64>,
        allow_duplicate_isbn: bool,
        confirm_replace_existing_id: Option<i64>,
    ) -> AppResult<(Item, ImportReport)> {
        if !allow_duplicate_isbn {
            if let Some(ref isbn) = item.isbn {
                if let Some(dup) = self.repository.items_find_by_isbn_for_import(isbn).await? {
                    if dup.specimen_count > 0 {
                        tracing::info!(
                            "Catalog create: merging bibliographic data into existing item id={} ({} specimens)",
                            dup.item_id, dup.specimen_count
                        );
                        self.repository.items_update(dup.item_id, source_id, &mut item).await?;
                        let report = ImportReport {
                            action: ImportAction::MergedBibliographic,
                            existing_id: Some(dup.item_id),
                            warnings: vec![],
                            message: Some(format!(
                                "Bibliographic data merged into existing item id={}. {} specimen(s) preserved.",
                                dup.item_id, dup.specimen_count
                            )),
                        };
                        return Ok((item, report));
                    }

                    if dup.archived_at.is_some() {
                        tracing::info!("Catalog create: replacing archived item id={}", dup.item_id);
                        self.repository.items_update(dup.item_id, source_id, &mut item).await?;
                        let report = ImportReport {
                            action: ImportAction::ReplacedArchived,
                            existing_id: Some(dup.item_id),
                            warnings: vec![],
                            message: Some(format!("Replaced archived item id={}.", dup.item_id)),
                        };
                        return Ok((item, report));
                    }

                    if confirm_replace_existing_id == Some(dup.item_id) {
                        tracing::info!("Catalog create: confirmed replacement of item id={}", dup.item_id);
                        self.repository.items_update(dup.item_id,  source_id, &mut item).await?;
                        let report = ImportReport {
                            action: ImportAction::ReplacedConfirmed,
                            existing_id: Some(dup.item_id),
                            warnings: vec![],
                            message: Some(format!("Replaced item id={} after confirmation.", dup.item_id)),
                        };
                        return Ok((item, report));
                    }

                    return Err(AppError::DuplicateNeedsConfirmation {
                        existing_id: dup.item_id,
                        message: format!(
                            "An item with ISBN {} already exists (id={}). \
                             Resend with confirm_replace_existing_id={} to replace it.",
                            isbn, dup.item_id, dup.item_id
                        ),
                    });
                }
            }
        }

        let mut warnings = Vec::new();
        if item.isbn.is_none() && !allow_duplicate_isbn {
            warnings.push("No ISBN — duplicate check skipped. This may create silent duplicates.".to_string());
        }

        let record = MarcRecord::from(&item);
        item.marc_record = serde_json::to_value(&record).ok();
        self.repository.items_create(source_id, &mut item).await?;
        let report = ImportReport {
            action: ImportAction::Created,
            existing_id: None,
            warnings,
            message: None,
        };
        Ok((item, report))
    }

    /// Update an existing item
    pub async fn update_item(&self, id: i64, mut item: Item) -> AppResult<Item> {
        // Check if item exists
        self.repository.items_get_by_id_or_isbn(&id.to_string()).await?;

        // Check for duplicate ISBN
        if let Some(ref isbn) = item.isbn {
            if self
                .repository
                .items_isbn_exists(isbn, Some(id))
                .await?
            {
                return Err(crate::error::AppError::Conflict(
                    "Item with this ISBN already exists".to_string(),
                ));
            }
        }

        let record = MarcRecord::from(&item);
        item.marc_record = serde_json::to_value(&record).ok();
        self.repository.items_update(id, None, &mut item).await?;
        Ok(item)
    }

    /// Delete an item
    pub async fn delete_item(&self, id: i64, force: bool) -> AppResult<()> {
        self.repository.items_delete(id, force).await
    }

    /// Get specimens for an item
    pub async fn get_specimens(&self, item_id: i64) -> AppResult<Vec<Specimen>> {
        // Verify item exists
        self.repository.items_get_by_id_or_isbn(&item_id.to_string()).await?;
        self.repository.items_get_specimens(item_id).await
    }

    /// Create a specimen for an item.
    /// Barcode must be unique among active specimens.
    /// If barcode exists on an archived specimen, it is reactivated and updated.
    pub async fn create_specimen(&self, item_id: i64, specimen: Specimen) -> AppResult<Specimen> {
        self.repository.items_get_by_id_or_isbn(&item_id.to_string()).await?;
        if let Some(ref barcode) = specimen.barcode {
            if let Some((existing_id, is_archived)) = self
                .repository
                .items_get_specimen_by_barcode(barcode)
                .await?
            {
                if is_archived {
                    return self
                        .repository
                        .items_reactivate_specimen(existing_id, item_id, &specimen)
                        .await;
                }
                return Err(crate::error::AppError::Conflict(
                    "A specimen with this barcode already exists".to_string(),
                ));
            }
        }
        self.repository.items_create_specimen(item_id, &specimen).await
    }

    /// Update a specimen
    pub async fn update_specimen(&self, item_id: i64, specimen_id: i64, specimen: Specimen) -> AppResult<Specimen> {
        // Verify item exists
        self.repository.items_get_by_id_or_isbn(&item_id.to_string()).await?;
        // Verify specimen belongs to item
        let specimens = self.repository.items_get_specimens(item_id).await?;
        if !specimens.iter().any(|s| s.id == Some(specimen_id)) {
            return Err(crate::error::AppError::NotFound(
                format!("Specimen {} not found for item {}", specimen_id, item_id)
            ));
        }
        // Enforce barcode uniqueness when changing barcode
        if let Some(ref barcode) = specimen.barcode {
            if self
                .repository
                .items_specimen_barcode_exists(barcode, Some(specimen_id))
                .await?
            {
                return Err(crate::error::AppError::Conflict(
                    "A specimen with this barcode already exists".to_string(),
                ));
            }
        }
        self.repository.items_update_specimen(specimen_id, &specimen).await
    }

    /// Delete a specimen
    pub async fn delete_specimen(&self, _item_id: i64, specimen_id: i64, force: bool) -> AppResult<()> {
        self.repository.items_delete_specimen(specimen_id, force).await
    }

    /// List all items in a series (ordered by volume number)
    pub async fn get_items_by_series(&self, series_id: i64) -> AppResult<Vec<ItemShort>> {
        self.repository.items_get_by_series(series_id).await
    }
}


