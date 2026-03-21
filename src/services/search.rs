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
pub use crate::models::item::MeiliItemDocument;

/// Optional filter parameters applied alongside the free-text query.
#[derive(Debug, Default)]
pub struct SearchFilters {
    pub media_type: Option<String>,
    pub lang: Option<String>,
    pub audience_type: Option<i16>,
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
    pub async fn index_document(&self, doc: &MeiliItemDocument) {
        let index = self.client.index(&self.index_name);
        if let Err(e) = index.add_or_replace(&[doc], Some("id")).await {
            warn!("Meilisearch index_document failed for id={}: {}", doc.id, e);
        }
    }

    /// Remove a document by item ID.
    pub async fn delete_document(&self, id: i64) {
        let index = self.client.index(&self.index_name);
        if let Err(e) = index.delete_document(&id).await {
            warn!("Meilisearch delete_document failed for id={}: {}", id, e);
        }
    }

    /// Replace all documents in the index with the provided batch.
    ///
    /// The documents are pushed in chunks to avoid hitting request-size limits.
    pub async fn reindex_all(&self, docs: Vec<MeiliItemDocument>) {
        let index = self.client.index(&self.index_name);

        if let Err(e) = index.delete_all_documents().await {
            warn!("Meilisearch reindex_all: failed to clear index: {}", e);
            return;
        }

        const CHUNK_SIZE: usize = 500;
        let total = docs.len();
        let mut indexed = 0usize;

        for chunk in docs.chunks(CHUNK_SIZE) {
            match index.add_documents(chunk, Some("id")).await {
                Ok(_) => {
                    indexed += chunk.len();
                    info!("Meilisearch reindex: {}/{} documents queued", indexed, total);
                }
                Err(e) => {
                    warn!("Meilisearch reindex chunk failed: {}", e);
                }
            }
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
    if let Some(at) = filters.audience_type {
        parts.push(format!("audience_type = {}", at));
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
