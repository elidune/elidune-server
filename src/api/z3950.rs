//! Z39.50 catalog search endpoints

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::AppResult,
    models::{
        biblio::Biblio,
        import_report::ImportReport,
        item::Item,
    },
};

use super::AuthenticatedUser;

/// Z39.50 search query parameters
#[serde_as]
#[derive(Deserialize, IntoParams, ToSchema, Debug)]
pub struct Z3950SearchQuery {
    pub query: String,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub server_id: Option<i64>,
    pub max_results: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct Z3950SearchResponse {
    /// Total results found
    pub total: i32,
    /// List of found bibliographic records
    pub biblios: Vec<Biblio>,
    /// Source server name
    pub source: String,
}

/// Z39.50 import request
#[serde_as]
#[derive(Deserialize, ToSchema)]
pub struct Z3950ImportRequest {
    /// Remote biblio ID to import
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub biblio_id: i64,
    /// Physical items (copies) to create for the imported biblio
    pub items: Option<Vec<ImportItem>>,
    /// Set to the existing biblio ID to confirm replacement of a duplicate
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub confirm_replace_existing_id: Option<i64>,
}

#[serde_as]
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportItem {
    /// Item barcode (must be unique when provided)
    pub barcode: Option<String>,
    /// Shelf location / call number
    pub call_number: Option<String>,
    /// Status code
    pub status: Option<String>,
    /// Place (shelf/room number)
    pub place: Option<i16>,
    /// Notes
    pub notes: Option<String>,
    /// Price
    pub price: Option<String>,
    /// Source ID
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub source_id: Option<i64>,
}

impl From<ImportItem> for Item {
    fn from(s: ImportItem) -> Self {
        let borrowable = s
            .status
            .as_ref()
            .and_then(|st| st.parse::<i16>().ok())
            .map(|v| v == 98)
            .unwrap_or(true);
        Item {
            id: None,
            biblio_id: None,
            source_id: s.source_id,
            barcode: s.barcode,
            call_number: s.call_number,
            volume_designation: None,
            place: s.place,
            borrowable,
            circulation_status: None,
            notes: s.notes,
            price: s.price,
            created_at: None,
            updated_at: None,
            archived_at: None,
            source_name: None,
        }
    }
}

/// Response body for Z39.50 import (biblio + dedup report)
#[derive(Serialize, ToSchema)]
pub struct Z3950ImportResponse {
    /// The imported or updated bibliographic record
    pub biblio: Biblio,
    /// Deduplication report
    pub import_report: ImportReport,
}

/// Search remote catalogs via Z39.50
#[utoipa::path(
    get,
    path = "/z3950/search",
    tag = "z3950",
    security(("bearer_auth" = [])),
    params(
        ("isbn" = Option<String>, Query, description = "ISBN to search"),
        ("title" = Option<String>, Query, description = "Title to search"),
        ("author" = Option<String>, Query, description = "Author to search"),
        ("max_results" = Option<i32>, Query, description = "Max results (default: 50)")
    ),
    responses(
        (status = 200, description = "Search results", body = Z3950SearchResponse),
        (status = 502, description = "Z39.50 server error")
    )
)]
pub async fn search(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<Z3950SearchQuery>,
) -> AppResult<Json<Z3950SearchResponse>> {
    claims.require_read_items()?;

    let (biblios, total, source) = state.services.z3950.search(&query).await?;

    Ok(Json(Z3950SearchResponse {
        total,
        biblios,
        source,
    }))
}

/// Import a record from Z39.50 search results into local catalog.
/// Applies ISBN deduplication automatically (merge/replace/confirm).
#[utoipa::path(
    post,
    path = "/z3950/import",
    tag = "z3950",
    security(("bearer_auth" = [])),
    request_body = Z3950ImportRequest,
    responses(
        (status = 201, description = "Record imported or merged", body = Z3950ImportResponse),
        (status = 404, description = "Remote item not found"),
        (status = 409, description = "Duplicate ISBN requires confirmation", body = crate::models::import_report::DuplicateConfirmationRequired)
    )
)]
pub async fn import_record(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(request): Json<Z3950ImportRequest>,
) -> AppResult<(StatusCode, Json<Z3950ImportResponse>)> {
    claims.require_write_items()?;

    
    let (biblio, import_report) = state
        .services
        .z3950
        .import_record(
            request.biblio_id,
            request.items,
            request.confirm_replace_existing_id,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(Z3950ImportResponse { biblio, import_report })))
}

/// Build the Z39.50 routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/z3950/search", get(search))
        .route("/z3950/import", post(import_record))
}
