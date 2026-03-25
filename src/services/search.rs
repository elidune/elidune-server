//! Meilisearch integration for catalog full-text search.
//!
//! [`MeilisearchService`] is a thin wrapper around the Meilisearch client that:
//! - Configures the index on startup (`ensure_index`)
//! - Indexes / deletes individual documents on catalog mutations
//! - Executes full-text searches and returns ordered item IDs
//! - Supports a full reindex for recovery or initial population

use meilisearch_sdk::{client::Client, settings::Settings};
use serde::Deserialize;
use tracing::{info, warn};

use crate::config::MeilisearchConfig;
pub use crate::models::biblio::MeiliBiblioDocument;

/// Optional filter parameters applied alongside the free-text query.
#[derive(Debug, Default)]
pub struct SearchFilters {
    pub media_type: Option<String>,
    pub lang: Option<String>,
    pub audience_type: Option<String>,
    pub archive: Option<bool>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// Provides catalog search via Meilisearch.
///
/// Obtain an instance via [`MeilisearchService::new`]; call [`ensure_index`]
/// once at startup to set searchable/filterable attributes.
#[derive(Clone)]
pub struct MeilisearchService {
    client: Client,
    index_name: String,
}

impl MeilisearchService {
    /// Create a service from the given configuration.
    pub fn new(config: &MeilisearchConfig) -> Self {
        let client = Client::new(&config.url, config.api_key.as_deref()).unwrap();
        Self {
            client,
            index_name: config.index_name.clone(),
        }
    }

    /// Configure the Meilisearch index (idempotent — safe to call at every startup).
    ///
    /// Sets searchable attributes (ranked by relevance), filterable attributes
    /// (for sidebar filters / query-time filters), and ranking rules.
    #[tracing::instrument(skip(self))]
    pub async fn ensure_index(&self) {
        let index = self.client.index(&self.index_name);

        let settings = Settings::new()
            .with_searchable_attributes([
                "title",
                "author_names",
                "subject",
                "keywords",
                "isbn",
                "publisher_name",
                "series_name",
                "collection_name",
                "barcodes",
                "call_numbers",
                "abstract_text",
                "notes",
                "table_of_contents",
            ])
            .with_sortable_attributes(["title"])
            .with_ranking_rules([
                "words",
                "typo",
                "proximity",
                "attribute",
                "sort",
                "exactness",
            ]);

        match index.set_settings(&settings).await {
            Ok(_) => info!("Meilisearch index '{}' configured", self.index_name),
            Err(e) => warn!("Failed to configure Meilisearch index: {}", e),
        }

        // Filterable attributes are set separately because the 0.32 SDK
        // uses FilterableAttribute instead of plain &str in the Settings builder.
        let filterable: Vec<&str> = vec!["media_type", "lang", "audience_type", "is_archived"];
        match index.set_filterable_attributes(&filterable).await {
            Ok(_) => {}
            Err(e) => warn!("Failed to set filterable attributes: {}", e),
        }
    }

    /// Full-text search. Returns `(ordered_item_ids, total_hits)`.
    ///
    /// `page` and `per_page` are 1-based / count-based, matching the API convention.
    #[tracing::instrument(skip(self), err)]
    pub async fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        page: i64,
        per_page: i64,
    ) -> Result<(Vec<i64>, i64), meilisearch_sdk::errors::Error> {
        let index = self.client.index(&self.index_name);
        let offset = ((page - 1) * per_page) as usize;
        let limit = per_page as usize;

        let filter_expr = build_filter_expr(filters);

        #[derive(Deserialize)]
        struct IdOnly {
            id: i64,
        }

        let mut sq = index.search();
        sq.with_query(query)
            .with_offset(offset)
            .with_limit(limit);

        if let Some(ref f) = filter_expr {
            sq.with_filter(f.as_str());
        }

        let results = sq.execute::<IdOnly>().await?;

        let ids: Vec<i64> = results.hits.into_iter().map(|h| h.result.id).collect();
        let total = results.estimated_total_hits.unwrap_or(ids.len()) as i64;

        Ok((ids, total))
    }

    /// Index (create or replace) a single document.
    #[tracing::instrument(skip(self))]
    pub async fn index_document(&self, doc: &MeiliBiblioDocument) {
        let index = self.client.index(&self.index_name);
        if let Err(e) = index.add_or_replace(&[doc], Some("id")).await {
            warn!("Meilisearch index_document failed for id={}: {}", doc.id, e);
        }
    }

    /// Remove a document by item ID.
    #[tracing::instrument(skip(self))]
    pub async fn delete_document(&self, id: i64) {
        let index = self.client.index(&self.index_name);
        if let Err(e) = index.delete_document(&id).await {
            warn!("Meilisearch delete_document failed for id={}: {}", id, e);
        }
    }

    /// Delete all documents from the index (first step of a full reindex).
    /// Returns `false` if the operation failed (logged as a warning).
    #[tracing::instrument(skip(self))]
    pub async fn clear_index(&self) -> bool {
        let index = self.client.index(&self.index_name);
        match index.delete_all_documents().await {
            Ok(_) => true,
            Err(e) => {
                warn!("Meilisearch clear_index failed: {}", e);
                false
            }
        }
    }

    /// Push a batch of documents to the index (`add_or_replace` semantics).
    ///
    /// Called repeatedly during a cursor-based reindex; each call corresponds
    /// to one page fetched from the database.
    #[tracing::instrument(skip(self, docs), fields(batch_len = docs.len()))]
    pub async fn index_batch(&self, docs: &[MeiliBiblioDocument]) {
        if docs.is_empty() {
            return;
        }
        let index = self.client.index(&self.index_name);
        match index.add_documents(docs, Some("id")).await {
            Ok(_) => info!("Meilisearch index_batch: {} documents queued", docs.len()),
            Err(e) => warn!("Meilisearch index_batch failed: {}", e),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a Meilisearch filter expression from structured filter params.
/// Returns `None` if there are no active filters.
fn build_filter_expr(filters: &SearchFilters) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(ref mt) = filters.media_type {
        parts.push(format!("media_type = \"{}\"", mt.replace('"', "\\\"")));
    }
    if let Some(ref lang) = filters.lang {
        parts.push(format!("lang = \"{}\"", lang.replace('"', "\\\"")));
    }
    if let Some(ref at) = filters.audience_type {
        parts.push(format!("audience_type = \"{}\"", at.replace('"', "\\\"")));
    }
    match filters.archive {
        Some(true) => parts.push("is_archived = true".to_string()),
        Some(false) | None => parts.push("is_archived = false".to_string()),
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" AND "))
    }
}
