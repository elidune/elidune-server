//! Biblios domain methods on [`Repository`].
//!
//! Split across `read`, `search`, `write`, `items`, and `meili` submodules.

mod items;
mod meili;
mod read;
mod search;
mod write;

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, Row};
use sqlx::types::Json;

use super::Repository;
use crate::models::item::ItemShort;
use crate::{
    error::{AppError, AppResult},
    marc::MarcRecord,
    models::{
        author::Author,
        author::Function,
        import_report::DuplicateCandidate,
        biblio::{Collection, Edition, Isbn, Biblio, BiblioQuery, BiblioShort, MeiliBiblioDocument, MediaType, Serie},
        item::Item,
    },
};

/// Deduplicate N:M junction rows by entity id (series or collection).
///
/// Keeps first-occurrence order. When a duplicate carries a volume number and the first
/// occurrence had none, the volume is merged in (common when MARC repeats the same 490).
pub(crate) fn dedupe_junction_links(ids: &[i64], volumes: &[Option<i16>]) -> (Vec<i64>, Vec<Option<i16>>) {
    let mut deduped_ids = Vec::with_capacity(ids.len());
    let mut deduped_vols: Vec<Option<i16>> = Vec::with_capacity(ids.len());
    let mut index_by_id: HashMap<i64, usize> = HashMap::new();

    for (pos, &id) in ids.iter().enumerate() {
        let vol = volumes.get(pos).copied().flatten();
        if let Some(&idx) = index_by_id.get(&id) {
            if deduped_vols[idx].is_none() {
                deduped_vols[idx] = vol;
            }
        } else {
            index_by_id.insert(id, deduped_ids.len());
            deduped_ids.push(id);
            deduped_vols.push(vol);
        }
    }

    (deduped_ids, deduped_vols)
}
/// Contract for [`Repository`] biblio/item persistence. Implemented below; services may use
/// `Arc<dyn BibliosRepository>` for substitution in tests.
#[async_trait]
pub trait BibliosRepository: Send + Sync {
    async fn biblios_get_by_id(&self, id: i64) -> AppResult<Biblio>;
    async fn biblios_get_short_by_id(&self, id: i64) -> AppResult<BiblioShort>;
    async fn biblios_search(&self, query: &BiblioQuery) -> AppResult<(Vec<BiblioShort>, i64)>;
    async fn biblios_get_by_series(&self, series_id: i64) -> AppResult<Vec<BiblioShort>>;
    async fn biblios_get_by_collection(&self, collection_id: i64) -> AppResult<Vec<BiblioShort>>;
    async fn biblios_get_meili_document(&self, id: i64) -> AppResult<Option<MeiliBiblioDocument>>;
    /// Fetch a page of Meilisearch documents using a keyset cursor.
    /// Returns biblios with `id > after_id`, up to `limit` rows, ordered by id.
    async fn biblios_get_meili_documents_batch(
        &self,
        after_id: i64,
        limit: i64,
    ) -> AppResult<Vec<MeiliBiblioDocument>>;
    async fn biblios_get_short_by_ids_ordered(&self, ids: &[i64]) -> AppResult<Vec<BiblioShort>>;
    async fn biblios_create<'a>(&self, biblio: &'a mut Biblio) -> AppResult<&'a mut Biblio>;
    async fn biblios_update<'a>(&self, id: i64, biblio: &'a mut Biblio) -> AppResult<&'a mut Biblio>;
    async fn biblios_delete(&self, id: i64, force: bool) -> AppResult<()>;
    /// Archive a biblio when it has no active copies left. Returns `true` if archived.
    async fn biblios_archive_if_orphan(&self, biblio_id: i64) -> AppResult<bool>;
    async fn biblios_get_items(&self, biblio_id: i64) -> AppResult<Vec<Item>>;
    /// Active (non-archived) item by primary key.
    async fn items_get_active_by_id(&self, item_id: i64) -> AppResult<Item>;
    /// Active (non-archived) item by barcode (exact match).
    async fn items_get_active_by_barcode(&self, barcode: &str) -> AppResult<Item>;
    async fn biblios_get_items_short_by_biblio_ids(
        &self,
        biblio_ids: &[i64],
    ) -> AppResult<HashMap<i64, Vec<ItemShort>>>;
    async fn biblios_create_item(&self, biblio_id: i64, item: &Item) -> AppResult<Item>;
    async fn upsert_item<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item>;
    async fn items_update<'a>(&self, item: &'a mut Item) -> AppResult<&'a mut Item>;
    async fn items_delete(&self, id: i64, force: bool) -> AppResult<()>;
    async fn items_barcode_exists(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<bool>;
    async fn items_get_by_barcode(&self, barcode: &str) -> AppResult<Option<(i64, bool)>>;
    async fn items_reactivate(
        &self,
        item_id: i64,
        biblio_id: i64,
        item: &Item,
    ) -> AppResult<Item>;
    async fn biblios_find_active_by_isbn(
        &self,
        isbn: &str,
        exclude_id: Option<i64>,
    ) -> AppResult<Option<i64>>;
    async fn items_find_short_by_barcode(
        &self,
        barcode: &str,
        exclude_item_id: Option<i64>,
    ) -> AppResult<Option<ItemShort>>;
    async fn biblios_find_by_isbn_for_import(&self, isbn: &str) -> AppResult<Option<DuplicateCandidate>>;
    async fn biblios_update_marc_record(&self, biblio: &mut Biblio) -> AppResult<()>;
    async fn biblios_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> AppResult<bool>;
    async fn biblios_count_items_for_source(&self, source_id: i64) -> AppResult<i64>;
    async fn biblios_reassign_items_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64>;
    async fn biblios_reassign_biblios_source(
        &self,
        old_source_ids: &[i64],
        new_source_id: i64,
    ) -> AppResult<i64>;
    /// Stored MARC JSON from `biblios.marc_record`, if non-null (notice without local items for export).
    async fn biblios_get_marc_record_optional(&self, biblio_id: i64) -> AppResult<Option<crate::marc::MarcRecord>>;
    /// Active biblios with a non-empty ISBN, optionally restricted to `marc_record IS NULL` when `force_rebuild` is false.
    async fn biblios_list_ids_for_z3950_refresh(&self, force_rebuild: bool) -> AppResult<Vec<i64>>;
    /// Replace bibliographic columns and `marc_record` (items are taken from `biblio.items` — caller must set copies to keep).
    async fn biblios_full_bibliographic_replace<'a>(
        &self,
        id: i64,
        biblio: &'a mut crate::models::biblio::Biblio,
    ) -> AppResult<&'a mut crate::models::biblio::Biblio>;
}

#[async_trait::async_trait]
impl BibliosRepository for Repository {
    async fn biblios_get_by_id(&self, id: i64) -> crate::error::AppResult<crate::models::biblio::Biblio> {
        Repository::biblios_get_by_id(self, id).await
    }
    async fn biblios_get_short_by_id(&self, id: i64) -> crate::error::AppResult<crate::models::biblio::BiblioShort> {
        Repository::biblios_get_short_by_id(self, id).await
    }
    async fn biblios_search(&self, query: &crate::models::biblio::BiblioQuery) -> crate::error::AppResult<(Vec<crate::models::biblio::BiblioShort>, i64)> {
        Repository::biblios_search(self, query).await
    }
    async fn biblios_get_by_series(&self, series_id: i64) -> crate::error::AppResult<Vec<crate::models::biblio::BiblioShort>> {
        Repository::biblios_get_by_series(self, series_id).await
    }
    async fn biblios_get_by_collection(&self, collection_id: i64) -> crate::error::AppResult<Vec<crate::models::biblio::BiblioShort>> {
        Repository::biblios_get_by_collection(self, collection_id).await
    }
    async fn biblios_get_meili_document(&self, id: i64) -> crate::error::AppResult<Option<crate::models::biblio::MeiliBiblioDocument>> {
        Repository::biblios_get_meili_document(self, id).await
    }
    async fn biblios_get_meili_documents_batch(&self, after_id: i64, limit: i64) -> crate::error::AppResult<Vec<crate::models::biblio::MeiliBiblioDocument>> {
        Repository::biblios_get_meili_documents_batch(self, after_id, limit).await
    }
    async fn biblios_get_short_by_ids_ordered(&self, ids: &[i64]) -> crate::error::AppResult<Vec<crate::models::biblio::BiblioShort>> {
        Repository::biblios_get_short_by_ids_ordered(self, ids).await
    }
    async fn biblios_create<'a>(&self, biblio: &'a mut crate::models::biblio::Biblio) -> crate::error::AppResult<&'a mut crate::models::biblio::Biblio> {
        Repository::biblios_create(self, biblio).await
    }
    async fn biblios_update<'a>(&self, id: i64, biblio: &'a mut crate::models::biblio::Biblio) -> crate::error::AppResult<&'a mut crate::models::biblio::Biblio> {
        Repository::biblios_update(self, id, biblio).await
    }
    async fn biblios_delete(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::biblios_delete(self, id, force).await
    }
    async fn biblios_archive_if_orphan(&self, biblio_id: i64) -> crate::error::AppResult<bool> {
        Repository::biblios_archive_if_orphan(self, biblio_id).await
    }
    async fn biblios_get_items(&self, biblio_id: i64) -> crate::error::AppResult<Vec<crate::models::item::Item>> {
        Repository::biblios_get_items(self, biblio_id).await
    }
    async fn items_get_active_by_id(&self, item_id: i64) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::items_get_active_by_id(self, item_id).await
    }
    async fn items_get_active_by_barcode(&self, barcode: &str) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::items_get_active_by_barcode(self, barcode).await
    }
    async fn biblios_get_items_short_by_biblio_ids(&self, biblio_ids: &[i64]) -> crate::error::AppResult<std::collections::HashMap<i64, Vec<crate::models::item::ItemShort>>> {
        Repository::biblios_get_items_short_by_biblio_ids(self, biblio_ids).await
    }
    async fn biblios_create_item(&self, biblio_id: i64, item: &crate::models::item::Item) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::biblios_create_item(self, biblio_id, item).await
    }
    async fn upsert_item<'a>(&self, item: &'a mut crate::models::item::Item) -> crate::error::AppResult<&'a mut crate::models::item::Item> {
        Repository::upsert_item(self, item).await
    }
    async fn items_update<'a>(&self, item: &'a mut crate::models::item::Item) -> crate::error::AppResult<&'a mut crate::models::item::Item> {
        Repository::items_update(self, item).await
    }
    async fn items_delete(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::items_delete(self, id, force).await
    }
    async fn items_barcode_exists(&self, barcode: &str, exclude_item_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::items_barcode_exists(self, barcode, exclude_item_id).await
    }
    async fn items_get_by_barcode(&self, barcode: &str) -> crate::error::AppResult<Option<(i64, bool)>> {
        Repository::items_get_by_barcode(self, barcode).await
    }
    async fn items_reactivate(&self, item_id: i64, biblio_id: i64, item: &crate::models::item::Item) -> crate::error::AppResult<crate::models::item::Item> {
        Repository::items_reactivate(self, item_id, biblio_id, item).await
    }
    async fn biblios_find_active_by_isbn(&self, isbn: &str, exclude_id: Option<i64>) -> crate::error::AppResult<Option<i64>> {
        Repository::biblios_find_active_by_isbn(self, isbn, exclude_id).await
    }
    async fn items_find_short_by_barcode(&self, barcode: &str, exclude_item_id: Option<i64>) -> crate::error::AppResult<Option<crate::models::item::ItemShort>> {
        Repository::items_find_short_by_barcode(self, barcode, exclude_item_id).await
    }
    async fn biblios_find_by_isbn_for_import(&self, isbn: &str) -> crate::error::AppResult<Option<crate::models::import_report::DuplicateCandidate>> {
        Repository::biblios_find_by_isbn_for_import(self, isbn).await
    }
    async fn biblios_update_marc_record(&self, biblio: &mut crate::models::biblio::Biblio) -> crate::error::AppResult<()> {
        Repository::biblios_update_marc_record(self, biblio).await
    }
    async fn biblios_isbn_exists(&self, isbn: &str, exclude_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::biblios_isbn_exists(self, isbn, exclude_id).await
    }
    async fn biblios_count_items_for_source(&self, source_id: i64) -> crate::error::AppResult<i64> {
        Repository::biblios_count_items_for_source(self, source_id).await
    }
    async fn biblios_reassign_items_source(&self, old_source_ids: &[i64], new_source_id: i64) -> crate::error::AppResult<i64> {
        Repository::biblios_reassign_items_source(self, old_source_ids, new_source_id).await
    }
    async fn biblios_reassign_biblios_source(&self, old_source_ids: &[i64], new_source_id: i64) -> crate::error::AppResult<i64> {
        Repository::biblios_reassign_biblios_source(self, old_source_ids, new_source_id).await
    }
    async fn biblios_get_marc_record_optional(&self, biblio_id: i64) -> crate::error::AppResult<Option<crate::marc::MarcRecord>> {
        Repository::biblios_get_marc_record_optional(self, biblio_id).await
    }
    async fn biblios_list_ids_for_z3950_refresh(&self, force_rebuild: bool) -> crate::error::AppResult<Vec<i64>> {
        Repository::biblios_list_ids_for_z3950_refresh(self, force_rebuild).await
    }
    async fn biblios_full_bibliographic_replace<'a>(
        &self,
        id: i64,
        biblio: &'a mut crate::models::biblio::Biblio,
    ) -> crate::error::AppResult<&'a mut crate::models::biblio::Biblio> {
        Repository::biblios_full_bibliographic_replace(self, id, biblio).await
    }
}


/// Internal row type for decoding BiblioShort with JSONB author (items loaded separately).
#[derive(FromRow)]
pub(crate) struct BiblioShortRow {
    id: i64,
    media_type: MediaType,
    isbn: Option<Isbn>,
    title: Option<String>,
    date: Option<String>,
    status: i16,
    #[allow(dead_code)]
    is_local: i16,
    is_valid: Option<bool>,
    archived_at: Option<chrono::DateTime<Utc>>,
    author: Option<Json<Author>>,
}

/// Row type for item (physical copy) short data from SQL (build ItemShort in Rust).
#[derive(FromRow)]
pub(crate) struct ItemShortRow {
    biblio_id: i64,
    id: i64,
    barcode: Option<String>,
    call_number: Option<String>,
    borrowable: bool,
    source_name: Option<String>,
    borrowed: bool,
}

impl From<ItemShortRow> for ItemShort {
    fn from(r: ItemShortRow) -> Self {
        Self {
            id: r.id,
            barcode: r.barcode,
            call_number: r.call_number,
            borrowable: r.borrowable,
            source_name: r.source_name,
            borrowed: r.borrowed,
        }
    }
}

impl From<BiblioShortRow> for BiblioShort {
    fn from(r: BiblioShortRow) -> Self {
        Self {
            id: r.id,
            media_type: r.media_type,
            isbn: r.isbn,
            title: r.title,
            date: r.date,
            status: r.status,
            is_valid: r.is_valid,
            archived_at: r.archived_at,
            author: r.author.map(|j| j.0),
            items: Vec::new(),
        }
    }
}

/// Escape a string for use as a LIKE pattern (ESCAPE '\').
pub(crate) fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub(crate) fn normalize_key(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c if c.is_alphanumeric() => c,
            _ => '_',
        })
        .collect::<String>()
        .replace("__", "_")
        .trim_matches('_')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::dedupe_junction_links;

    #[test]
    fn dedupe_junction_links_removes_duplicate_series_ids() {
        let ids = [10_i64, 20, 20, 30];
        let vols = [None, None, None, Some(5)];
        let (deduped_ids, deduped_vols) = dedupe_junction_links(&ids, &vols);
        assert_eq!(deduped_ids, vec![10, 20, 30]);
        assert_eq!(deduped_vols, vec![None, None, Some(5)]);
    }

    #[test]
    fn dedupe_junction_links_merges_volume_from_later_duplicate() {
        let ids = [10_i64, 20, 20];
        let vols = [None, None, Some(3)];
        let (deduped_ids, deduped_vols) = dedupe_junction_links(&ids, &vols);
        assert_eq!(deduped_ids, vec![10, 20]);
        assert_eq!(deduped_vols, vec![None, Some(3)]);
    }

    #[test]
    fn dedupe_junction_links_keeps_first_volume_when_both_set() {
        let ids = [10_i64, 10];
        let vols = [Some(1), Some(2)];
        let (_, deduped_vols) = dedupe_junction_links(&ids, &vols);
        assert_eq!(deduped_vols, vec![Some(1)]);
    }
}
