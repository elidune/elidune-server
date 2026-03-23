//! Collections CRUD endpoints.

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
    models::biblio::{BiblioShort, Collection, CollectionQuery, CreateCollection, UpdateCollection},
};

use super::{AuthenticatedUser, StaffUser};

/// Paginated list of collections.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedCollections {
    pub items: Vec<Collection>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub page_count: i64,
}

fn page_count(total: i64, per_page: i64) -> i64 {
    if per_page > 0 { (total + per_page - 1) / per_page } else { 0 }
}

/// List collections (paginated, optional name filter).
#[utoipa::path(
    get,
    path = "/collections",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("name" = Option<String>, Query, description = "Filter by name (substring)"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("per_page" = Option<i64>, Query, description = "Items per page (default: 50, max: 200)"),
    ),
    responses(
        (status = 200, description = "Paginated list of collections", body = PaginatedCollections),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn list_collections(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<CollectionQuery>,
) -> AppResult<Json<PaginatedCollections>> {
    claims.require_read_items()?;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(200);
    let (items, total) = state.services.catalog.list_collections(&query).await?;
    Ok(Json(PaginatedCollections { items, total, page, per_page, page_count: page_count(total, per_page) }))
}

/// Get a collection by ID.
#[utoipa::path(
    get,
    path = "/collections/{id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Collection ID")),
    responses(
        (status = 200, description = "Collection detail", body = Collection),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_collection(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Collection>> {
    claims.require_read_items()?;
    let collection = state.services.catalog.get_collection(id).await?;
    Ok(Json(collection))
}

/// List biblios in a collection (ordered by volume number).
#[utoipa::path(
    get,
    path = "/collections/{id}/biblios",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Collection ID")),
    responses(
        (status = 200, description = "Biblios in this collection", body = Vec<BiblioShort>),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_collection_biblios(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<BiblioShort>>> {
    claims.require_read_items()?;
    state.services.catalog.get_collection(id).await?;
    let biblios = state.services.catalog.get_biblios_by_collection(id).await?;
    Ok(Json(biblios))
}

/// Create a new collection.
#[utoipa::path(
    post,
    path = "/collections",
    tag = "collections",
    security(("bearer_auth" = [])),
    request_body = CreateCollection,
    responses(
        (status = 201, description = "Collection created", body = Collection),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Staff access required"),
        (status = 409, description = "Duplicate key"),
    )
)]
pub async fn create_collection(
    State(state): State<crate::AppState>,
    StaffUser(_claims): StaffUser,
    Json(data): Json<CreateCollection>,
) -> AppResult<impl IntoResponse> {
    let collection = state.services.catalog.create_collection(&data).await?;
    Ok((StatusCode::CREATED, Json(collection)))
}

/// Update a collection.
#[utoipa::path(
    put,
    path = "/collections/{id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Collection ID")),
    request_body = UpdateCollection,
    responses(
        (status = 200, description = "Collection updated", body = Collection),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Staff access required"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn update_collection(
    State(state): State<crate::AppState>,
    StaffUser(_claims): StaffUser,
    Path(id): Path<i64>,
    Json(data): Json<UpdateCollection>,
) -> AppResult<Json<Collection>> {
    let collection = state.services.catalog.update_collection(id, &data).await?;
    Ok(Json(collection))
}

/// Delete a collection (only if no biblios are linked).
#[utoipa::path(
    delete,
    path = "/collections/{id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(("id" = i64, Path, description = "Collection ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Staff access required"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Still linked to biblios"),
    )
)]
pub async fn delete_collection(
    State(state): State<crate::AppState>,
    StaffUser(_claims): StaffUser,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    state.services.catalog.delete_collection(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<crate::AppState> {
    use axum::routing::{delete, post, put};
    Router::new()
        .route("/collections", get(list_collections).post(create_collection))
        .route("/collections/:id", get(get_collection).put(update_collection).delete(delete_collection))
        .route("/collections/:id/biblios", get(get_collection_biblios))
}
