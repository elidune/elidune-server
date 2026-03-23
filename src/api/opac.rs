//! OPAC (Online Public Access Catalog) — public, unauthenticated read-only endpoints
//!
//! Suitable for patron self-service kiosks, embedded catalog widgets, or public websites.
//! No authentication required. Only non-sensitive catalog data is exposed.

use axum::{
    extract::{Path, Query, State},
    Json,
};

use crate::{
    api::biblios::PaginatedResponse,
    error::AppResult,
    models::biblio::{BiblioQuery, BiblioShort},
};

/// Public catalog search — no auth required
#[utoipa::path(
    get,
    path = "/opac/biblios",
    tag = "opac",
    params(
        ("title" = Option<String>, Query, description = "Search in title"),
        ("author" = Option<String>, Query, description = "Search by author"),
        ("isbn" = Option<String>, Query, description = "Search by ISBN"),
        ("freesearch" = Option<String>, Query, description = "Full-text search"),
        ("media_type" = Option<String>, Query, description = "Filter by media type"),
        ("serie" = Option<String>, Query, description = "Filter by series name (substring)"),
        ("serie_id" = Option<i64>, Query, description = "Filter by series ID (exact match)"),
        ("collection" = Option<String>, Query, description = "Filter by collection name (substring)"),
        ("collection_id" = Option<i64>, Query, description = "Filter by collection ID (exact match)"),
        ("page" = Option<i64>, Query, description = "Page number (default 1)"),
        ("per_page" = Option<i64>, Query, description = "Items per page (default 20, max 50)")
    ),
    responses(
        (status = 200, description = "Catalog search results", body = PaginatedResponse<BiblioShort>)
    )
)]
pub async fn opac_search(
    State(state): State<crate::AppState>,
    Query(mut query): Query<BiblioQuery>,
) -> AppResult<Json<PaginatedResponse<BiblioShort>>> {
    // Cap per_page to prevent abuse on public endpoint
    let per_page = query.per_page.unwrap_or(20).min(50);
    let page = query.page.unwrap_or(1).max(1);
    query.per_page = Some(per_page);
    query.page = Some(page);

    let (biblios, total) = state.services.catalog.search_biblios(&query).await?;
    Ok(Json(PaginatedResponse::new(biblios, total, page, per_page)))
}

/// Get a single bibliographic record by ID — public
#[utoipa::path(
    get,
    path = "/opac/biblios/{id}",
    tag = "opac",
    params(("id" = i64, Path, description = "Biblio ID")),
    responses(
        (status = 200, description = "Bibliographic record details", body = crate::models::biblio::Biblio),
        (status = 404, description = "Biblio not found", body = crate::error::ErrorResponse)
    )
)]
pub async fn opac_get_biblio(
    State(state): State<crate::AppState>,
    Path(biblio_id): Path<i64>,
) -> AppResult<Json<crate::models::biblio::Biblio>> {
    let biblio = state.services.catalog.get_biblio(biblio_id).await?;
    Ok(Json(biblio))
}

/// Get availability for a bibliographic record (how many physical copies are available)
#[utoipa::path(
    get,
    path = "/opac/biblios/{id}/availability",
    tag = "opac",
    params(("id" = i64, Path, description = "Biblio ID")),
    responses(
        (status = 200, description = "Availability count", body = serde_json::Value)
    )
)]
pub async fn opac_availability(
    State(state): State<crate::AppState>,
    Path(biblio_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let active_loans = state.services.loans.count_active_for_biblio(biblio_id).await?;
    let hold_count = state.services.reservations.count_for_item(biblio_id).await?;
    Ok(Json(serde_json::json!({
        "biblioId": biblio_id.to_string(),
        "activeLoans": active_loans,
        "holdCount": hold_count,
    })))
}

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/opac/biblios", get(opac_search))
        .route("/opac/biblios/:id", get(opac_get_biblio))
        .route("/opac/biblios/:id/availability", get(opac_availability))
}
