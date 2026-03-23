//! Series CRUD endpoints.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
};
use axum::routing::get;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    models::biblio::{BiblioShort, CreateSerie, Serie, SerieQuery, UpdateSerie},
};

use super::{AuthenticatedUser, StaffUser};

/// Paginated list of series.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedSeries {
    pub items: Vec<Serie>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub page_count: i64,
}

fn page_count(total: i64, per_page: i64) -> i64 {
    if per_page > 0 { (total + per_page - 1) / per_page } else { 0 }
}

/// List series (paginated, optional name filter).
#[utoipa::path(
    get,
    path = "/series",
    tag = "series",
    security(("bearer_auth" = [])),
    params(
        ("name" = Option<String>, Query, description = "Filter by name (substring)"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("per_page" = Option<i64>, Query, description = "Items per page (default: 50, max: 200)"),
    ),
    responses(
        (status = 200, description = "Paginated list of series", body = PaginatedSeries),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn list_series(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<SerieQuery>,
) -> AppResult<Json<PaginatedSeries>> {
    claims.require_read_items()?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let (items, total) = state.services.catalog.list_series(&query).await?;
    Ok(Json(PaginatedSeries { items, total, page, per_page, page_count: page_count(total, per_page) }))
}

/// Get a series by ID.
#[utoipa::path(
    get,
    path = "/series/{id}",
    tag = "series",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Series ID")),
    responses(
        (status = 200, description = "Series detail", body = Serie),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_serie(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Serie>> {
    claims.require_read_items()?;
    let serie = state.services.catalog.get_serie(id).await?;
    Ok(Json(serie))
}

/// List biblios in a series (ordered by volume number).
#[utoipa::path(
    get,
    path = "/series/{id}/biblios",
    tag = "series",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Series ID")),
    responses(
        (status = 200, description = "Biblios in this series", body = Vec<BiblioShort>),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_serie_biblios(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<BiblioShort>>> {
    claims.require_read_items()?;
    state.services.catalog.get_serie(id).await?;
    let biblios = state.services.catalog.get_biblios_by_series(id).await?;
    Ok(Json(biblios))
}

/// Create a new series.
#[utoipa::path(
    post,
    path = "/series",
    tag = "series",
    security(("bearer_auth" = [])),
    request_body = CreateSerie,
    responses(
        (status = 201, description = "Series created", body = Serie),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Staff access required"),
        (status = 409, description = "Duplicate key"),
    )
)]
pub async fn create_serie(
    State(state): State<crate::AppState>,
    StaffUser(_claims): StaffUser,
    Json(data): Json<CreateSerie>,
) -> AppResult<impl IntoResponse> {
    let serie = state.services.catalog.create_serie(&data).await?;
    Ok((StatusCode::CREATED, Json(serie)))
}

/// Update a series.
#[utoipa::path(
    put,
    path = "/series/{id}",
    tag = "series",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Series ID")),
    request_body = UpdateSerie,
    responses(
        (status = 200, description = "Series updated", body = Serie),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Staff access required"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn update_serie(
    State(state): State<crate::AppState>,
    StaffUser(_claims): StaffUser,
    Path(id): Path<i64>,
    Json(data): Json<UpdateSerie>,
) -> AppResult<Json<Serie>> {
    let serie = state.services.catalog.update_serie(id, &data).await?;
    Ok(Json(serie))
}

/// Delete a series (only if no biblios are linked).
#[utoipa::path(
    delete,
    path = "/series/{id}",
    tag = "series",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Series ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Staff access required"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Still linked to biblios"),
    )
)]
pub async fn delete_serie(
    State(state): State<crate::AppState>,
    StaffUser(_claims): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    state.services.catalog.delete_serie(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<crate::AppState> {
    use axum::routing::{delete, post, put};
    Router::new()
        .route("/series", get(list_series).post(create_serie))
        .route("/series/:id", get(get_serie).put(update_serie).delete(delete_serie))
        .route("/series/:id/biblios", get(get_serie_biblios))
}
