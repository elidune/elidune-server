//! Sources API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use crate::services::audit;

use crate::{
    error::AppResult,
    models::source::{CreateSource, MergeSources, Source, UpdateSource},
};

use super::{AuthenticatedUser, ClientIp};

/// Query parameters for listing sources
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct SourcesQuery {
    /// Include archived sources (default: false)
    pub include_archived: Option<bool>,
}

/// Create a source
#[utoipa::path(
    post,
    path = "/sources",
    tag = "sources",
    security(("bearer_auth" = [])),
    request_body = CreateSource,
    responses(
        (status = 201, description = "Source created", body = Source)
    )
)]
pub async fn create_source(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<CreateSource>,
) -> AppResult<(StatusCode, Json<Source>)> {
    claims.require_write_items()?;
    let source = state.services.sources.create(&data).await?;
    state.services.audit.log(audit::event::SOURCE_CREATED, Some(claims.user_id), Some("source"), Some(source.id), ip, Some((&data, &source)));
    Ok((StatusCode::CREATED, Json(source)))
}

/// List all sources
#[utoipa::path(
    get,
    path = "/sources",
    tag = "sources",
    security(("bearer_auth" = [])),
    params(SourcesQuery),
    responses(
        (status = 200, description = "Sources list", body = Vec<Source>)
    )
)]
pub async fn list_sources(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<SourcesQuery>,
) -> AppResult<Json<Vec<Source>>> {
    claims.require_read_items()?;
    let sources = state
        .services
        .sources
        .list(query.include_archived.unwrap_or(false))
        .await?;
    Ok(Json(sources))
}

/// Get source by ID
#[utoipa::path(
    get,
    path = "/sources/{id}",
    tag = "sources",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Source ID")),
    responses(
        (status = 200, description = "Source details", body = Source)
    )
)]
pub async fn get_source(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Source>> {
    claims.require_read_items()?;
    let source = state.services.sources.get_by_id(id).await?;
    Ok(Json(source))
}

/// Update a source (name and/or default status)
#[utoipa::path(
    post,
    path = "/sources/{id}",
    tag = "sources",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Source ID")),
    request_body = UpdateSource,
    responses(
        (status = 200, description = "Source updated", body = Source)
    )
)]
pub async fn update_source(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(data): Json<UpdateSource>,
) -> AppResult<Json<Source>> {
    claims.require_write_items()?;
    let source = state.services.sources.update(id, &data).await?;
    state.services.audit.log(audit::event::SOURCE_UPDATED, Some(claims.user_id), Some("source"), Some(id), ip, Some((id, &data, &source)));
    Ok(Json(source))
}

/// Archive a source (fails if non-archived specimens are still linked)
#[utoipa::path(
    post,
    path = "/sources/{id}/archive",
    tag = "sources",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Source ID")),
    responses(
        (status = 200, description = "Source archived", body = Source),
        (status = 422, description = "Cannot archive: active specimens linked")
    )
)]
pub async fn archive_source(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<Source>> {
    claims.require_write_items()?;
    let source = state.services.sources.archive(id).await?;
    state.services.audit.log(audit::event::SOURCE_ARCHIVED, Some(claims.user_id), Some("source"), Some(id), ip, Some(&source));
    Ok(Json(source))
}

/// Merge multiple sources into a new one
#[utoipa::path(
    post,
    path = "/sources/merge",
    tag = "sources",
    security(("bearer_auth" = [])),
    request_body = MergeSources,
    responses(
        (status = 201, description = "New merged source created", body = Source)
    )
)]
pub async fn merge_sources(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Json(data): Json<MergeSources>,
) -> AppResult<(StatusCode, Json<Source>)> {
    claims.require_write_items()?;
    let source = state.services.sources.merge(&data).await?;
    state.services.audit.log(audit::event::SOURCE_MERGED, Some(claims.user_id), Some("source"), Some(source.id), ip, Some((&data, &source)));
    Ok((StatusCode::CREATED, Json(source)))
}
