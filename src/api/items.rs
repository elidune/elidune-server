//! Physical item (copy) endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    error::AppResult,
    models::biblio::Biblio,
    models::item::Item,
    services::audit::{self},
};

use super::{AuthenticatedUser, ClientIp, ValidatedJson};

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route(
            "/items/barcode/:barcode",
            get(get_biblio_by_barcode),
        )
        .route(
            "/items/:id",
            get(get_biblio_by_item).put(update_item).delete(delete_item),
        )
}

/// Get the bibliographic record for a physical copy.
///
/// Response is a full [`Biblio`]; `items` contains **only** the copy whose id was requested.
#[utoipa::path(
    get,
    path = "/items/{id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Physical copy (item) ID")
    ),
    responses(
        (status = 200, description = "Biblio with a single item entry", body = Biblio),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "Item not found or archived", body = crate::error::ErrorResponse),
        (status = 410, description = "Bibliographic record is archived", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_biblio_by_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(item_id): Path<i64>,
) -> AppResult<Json<Biblio>> {
    claims.require_read_items()?;
    let biblio = state.services.catalog.get_biblio_for_item(item_id).await?;
    Ok(Json(biblio))
}

/// Get the bibliographic record for a physical copy identified by barcode.
///
/// Response is a full [`Biblio`]; `items` contains **only** the matching copy.
#[utoipa::path(
    get,
    path = "/items/barcode/{barcode}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("barcode" = String, Path, description = "Physical copy barcode (exact match)")
    ),
    responses(
        (status = 200, description = "Biblio with a single item entry", body = Biblio),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
        (status = 404, description = "No active item with this barcode", body = crate::error::ErrorResponse),
        (status = 410, description = "Bibliographic record is archived", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_biblio_by_barcode(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(barcode): Path<String>,
) -> AppResult<Json<Biblio>> {
    claims.require_read_items()?;
    let biblio = state
        .services
        .catalog
        .get_biblio_for_item_barcode(barcode.as_str())
        .await?;
    Ok(Json(biblio))
}

/// Update a physical item. The path id is authoritative.
#[utoipa::path(
    put,
    path = "/items/{id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Physical copy (item) ID")
    ),
    request_body = Item,
    responses(
        (status = 200, description = "Physical item updated", body = Item),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 404, description = "Biblio or item not found", body = crate::error::ErrorResponse),
        (status = 409, description = "An item with this barcode already exists")
    )
)]
pub async fn update_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(item_id): Path<i64>,
    ValidatedJson(mut item): ValidatedJson<Item>,
) -> AppResult<Json<Item>> {
    claims.require_write_items()?;
    let (biblio_id, _) = state
        .services
        .catalog
        .update_item(item_id, &mut item)
        .await?;

    state.services.audit.log(
        audit::event::ITEM_UPDATED,
        Some(claims.user_id),
        Some("item"),
        Some(item_id),
        ip,
        Some((biblio_id, &item)),
     audit::AuditLogMeta::success());

    Ok(Json(item))
}

/// Delete a physical item (soft delete unless `force` when borrowed).
#[utoipa::path(
    delete,
    path = "/items/{id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Physical copy (item) ID"),
        ("force" = Option<bool>, Query, description = "Force delete even if borrowed")
    ),
    responses(
        (status = 204, description = "Physical item deleted"),
        (status = 404, description = "Item not found"),
        (status = 409, description = "Item is borrowed")
    )
)]
pub async fn delete_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(item_id): Path<i64>,
    Query(params): Query<DeleteItemParams>,
) -> AppResult<StatusCode> {
    claims.require_write_items()?;
    let force = params.force.unwrap_or(false);
    let biblio_id = state.services.catalog.delete_item(item_id, force).await?;

    state.services.audit.log(
        audit::event::ITEM_DELETED,
        Some(claims.user_id),
        Some("item"),
        Some(item_id),
        ip,
        Some(serde_json::json!({
            "biblio_id": biblio_id,
            "item_id": item_id,
            "force": force,
        })),
     audit::AuditLogMeta::success());

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteItemParams {
    pub force: Option<bool>,
}
