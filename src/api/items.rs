//! Item (catalog) endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::Multipart;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::{
    error::{AppError, AppResult},
    models::{
        import_report::ImportReport,
        item::{Item, ItemQuery, ItemShort},
        specimen::Specimen,
    },
    services::{
        audit::{self},
        marc::{EnqueueResult, MarcBatchImportReport},
    },
};

use super::{AuthenticatedUser, ClientIp};

#[derive(Debug, Deserialize, Default)]
pub struct GetItemQuery {
    /// If true, include the full MARC record (marc_record JSONB) in the response
    #[serde(default)]
    pub full_record: bool,
}

/// Paginated response wrapper
#[derive(Serialize, ToSchema)]
pub struct PaginatedResponse<T>
where
    T: for<'a> ToSchema<'a>,
{
    /// List of items
    pub items: Vec<T>,
    /// Total number of items
    pub total: i64,
    /// Current page number
    pub page: i64,
    /// Items per page
    pub per_page: i64,
}

/// List items with search and pagination
#[utoipa::path(
    get,
    path = "/items",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("media_type" = Option<String>, Query, description = "Filter by media type"),
        ("title" = Option<String>, Query, description = "Search in title"),
        ("author" = Option<String>, Query, description = "Search by author"),
        ("isbn" = Option<String>, Query, description = "Search by ISBN/ISSN"),
        ("freesearch" = Option<String>, Query, description = "Full-text search"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("per_page" = Option<i64>, Query, description = "Items per page (default: 20)")
    ),
    responses(
        (status = 200, description = "List of items", body = PaginatedResponse<ItemShort>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_items(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<ItemQuery>,
) -> AppResult<Json<PaginatedResponse<ItemShort>>> {
    claims.require_read_items()?;

    let (items, total) = state.services.catalog.search_items(&query).await?;

    Ok(Json(PaginatedResponse {
        items,
        total,
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(20),
    }))
}

/// Get item details by ID
#[utoipa::path(
    get,
    path = "/items/{id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Item ID"),
        ("full_record" = Option<bool>, Query, description = "If true, include full MARC record data")
    ),
    responses(
        (status = 200, description = "Item details", body = Item),
        (status = 404, description = "Item not found")
    )
)]
pub async fn get_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
    Query(query): Query<GetItemQuery>,
) -> AppResult<Json<Item>> {
    claims.require_read_items()?;

   
    let item = state.services.catalog.get_item(id).await?;
    Ok(Json(item))
}

/// Query params for create item
#[serde_as]
#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct CreateItemQuery {
    /// If true, allow creating an item even when another item has the same ISBN
    #[serde(default)]
    pub allow_duplicate_isbn: bool,
    /// Set to the existing item ID to confirm replacement of a duplicate
    pub confirm_replace_existing_id: Option<i64>,
}

/// Response body for item creation (item + optional dedup report)
#[derive(Serialize, ToSchema)]
pub struct CreateItemResponse {
    pub item: Item,
    pub import_report: ImportReport,
}

/// Query params for UNIMARC upload
#[derive(Debug, Deserialize)]
pub struct UploadUnimarcQuery {
}

/// Query params for MARC batch import
#[serde_as]
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportMarcBatchQuery {
    /// Source ID
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub source_id: i64,
    /// Batch identifier returned by upload_unimarc
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    /// Optional record id inside the batch (e.g. \"1\", \"2\", ...)
    pub record_id: Option<usize>,
}

/// Create a new item (with ISBN deduplication)
#[utoipa::path(
    post,
    path = "/items",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("allow_duplicate_isbn" = Option<bool>, Query, description = "Allow duplicate ISBN (default: false)"),
        ("confirm_replace_existing_id" = Option<i64>, Query, description = "Confirm replacement of duplicate item")
    ),
    request_body = Item,
    responses(
        (status = 201, description = "Item created or merged", body = CreateItemResponse),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "Duplicate ISBN requires confirmation", body = crate::models::import_report::DuplicateConfirmationRequired)
    )
)]
pub async fn create_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Query(query): Query<CreateItemQuery>,
    Json(item): Json<Item>,
) -> AppResult<(StatusCode, Json<CreateItemResponse>)> {
    claims.require_write_items()?;
    let (item, import_report) = state
        .services
        .catalog
        .create_item(item, query.allow_duplicate_isbn, query.confirm_replace_existing_id)
        .await?;

    state.services.audit.log(
        audit::event::ITEM_CREATED,
        Some(claims.user_id),
        Some("item"),
        item.id,
        ip,
        Some(&item),
    );

    Ok((StatusCode::CREATED, Json(CreateItemResponse { item, import_report })))
}

/// Upload a UNIMARC file and return parsed items with linked specimens (995/952).
#[utoipa::path(
    post,
    path = "/items/upload-unimarc",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("source_id" = i64, Query, description = "Source ID associated to this MARC batch")
    ),
    responses(
        (status = 200, description = "Parsed items with specimens", body = EnqueueResult),
        (status = 400, description = "Missing file or invalid UNIMARC"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn upload_unimarc(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    mut multipart: Multipart,
) -> AppResult<Json<EnqueueResult>> {
    claims.require_read_items()?;

    let mut data = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name().as_deref() == Some("file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read field: {}", e)))?;
            data = bytes.to_vec();
            break;
        }
    }
    if data.is_empty() {
        return Err(AppError::BadRequest(
            "Missing 'file' field in multipart form".to_string(),
        ));
    }

    let enqueue_result = state
        .services
        .marc
        .enqueue_unimarc_batch(&data)
        .await?;

    // store 
    Ok(Json(enqueue_result))
}

/// Import cached MARC records from a batch into the catalog.
#[utoipa::path(
    post,
    path = "/items/import-marc-batch",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("batch_id" = String, Query, description = "MARC batch identifier returned by upload_unimarc"),
        ("record_id" = Option<String>, Query, description = "Optional record id inside batch; if omitted, import all records")
    ),
    responses(
        (status = 200, description = "MARC batch import report", body = MarcBatchImportReport),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn import_marc_batch(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Query(params): Query<ImportMarcBatchQuery>,
) -> AppResult<Json<MarcBatchImportReport>> {
    claims.require_write_items()?;
    let report = state
        .services
        .marc
        .import_from_batch(params.batch_id, params.source_id, params.record_id)
        .await?;

    state.services.audit.log(
        audit::event::IMPORT_MARC_BATCH,
        Some(claims.user_id),
        None,
        None,
        ip,
        Some(&params),
    );

    Ok(Json(report))
}

/// Update an existing item
#[utoipa::path(
    put,
    path = "/items/{id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Item ID"),
        ("allow_duplicate_isbn" = Option<bool>, Query, description = "Allow duplicate ISBN (default: false)")
    ),
    request_body = Item,
    responses(
        (status = 200, description = "Item updated", body = Item),
        (status = 404, description = "Item not found"),
        (status = 409, description = "Duplicate ISBN requires confirmation")
    )
)]
pub async fn update_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Query(query): Query<UpdateItemQuery>,
    Json(item): Json<Item>,
) -> AppResult<Json<Item>> {
    claims.require_write_items()?;
    let updated = state.services.catalog.update_item(id, item, query.allow_duplicate_isbn).await?;

    state.services.audit.log(
        audit::event::ITEM_UPDATED,
        Some(claims.user_id),
        Some("item"),
        Some(id),
        ip,
        Some((id, &updated)),
    );

    Ok(Json(updated))
}

#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct UpdateItemQuery {
    #[serde(default)]
    pub allow_duplicate_isbn: bool,
}

/// Delete an item
#[utoipa::path(
    delete,
    path = "/items/{id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Item ID"),
        ("force" = Option<bool>, Query, description = "Force delete even if specimens are borrowed")
    ),
    responses(
        (status = 204, description = "Item deleted"),
        (status = 404, description = "Item not found"),
        (status = 409, description = "Item has borrowed specimens")
    )
)]
pub async fn delete_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Query(params): Query<DeleteItemParams>,
) -> AppResult<StatusCode> {
    claims.require_write_items()?;
    state
        .services
        .catalog
        .delete_item(id, params.force.unwrap_or(false))
        .await?;

    state.services.audit.log(
        audit::event::ITEM_DELETED,
        Some(claims.user_id),
        Some("item"),
        Some(id),
        ip,
        Some(serde_json::json!({ "id": id, "force": params.force.unwrap_or(false) })),
    );

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct DeleteItemParams {
    pub force: Option<bool>,
}

/// List specimens for an item
#[utoipa::path(
    get,
    path = "/items/{id}/specimens",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Item ID")
    ),
    responses(
        (status = 200, description = "List of specimens", body = Vec<Specimen>),
        (status = 404, description = "Item not found")
    )
)]
pub async fn list_specimens(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(item_id): Path<i64>,
) -> AppResult<Json<Vec<Specimen>>> {
    claims.require_read_items()?;

    let specimens = state.services.catalog.get_specimens(item_id).await?;
    Ok(Json(specimens))
}

/// Create a new specimen for an item
#[utoipa::path(
    post,
    path = "/items/{id}/specimens",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("id" = i32, Path, description = "Item ID")
    ),
    request_body = Specimen,
    responses(
        (status = 201, description = "Specimen created", body = Specimen),
        (status = 404, description = "Item not found"),
        (status = 409, description = "A specimen with this barcode already exists")
    )
)]
pub async fn create_specimen(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(item_id): Path<i64>,
    Json(specimen): Json<Specimen>,
) -> AppResult<(StatusCode, Json<Specimen>)> {
    claims.require_write_items()?;
    let created = state
        .services
        .catalog
        .create_specimen(item_id, specimen)
        .await?;

    state.services.audit.log(
        audit::event::SPECIMEN_CREATED,
        Some(claims.user_id),
        Some("specimen"),
        created.id,
        ip,
        Some((item_id, &created)),
    );

    Ok((StatusCode::CREATED, Json(created)))
}

/// Update a specimen
#[utoipa::path(
    put,
    path = "/items/{item_id}/specimens/{specimen_id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("item_id" = i32, Path, description = "Item ID"),
        ("specimen_id" = i32, Path, description = "Specimen ID")
    ),
    request_body = Specimen,
    responses(
        (status = 200, description = "Specimen updated", body = Specimen),
        (status = 404, description = "Item or specimen not found"),
        (status = 409, description = "A specimen with this barcode already exists")
    )
)]
pub async fn update_specimen(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(item_id): Path<i64>,
    Json(mut specimen): Json<Specimen>,
) -> AppResult<Json<Specimen>> {
    claims.require_write_items()?;
    let specimen_id = specimen.id;
    state
        .services
        .catalog
        .update_specimen(item_id, &mut specimen)
        .await?;

    state.services.audit.log(
        audit::event::SPECIMEN_UPDATED,
        Some(claims.user_id),
        Some("specimen"),
        specimen_id,
        ip,
        Some((item_id, &specimen)),
    );

    Ok(Json(specimen))
}

/// Delete a specimen
#[utoipa::path(
    delete,
    path = "/items/{item_id}/specimens/{specimen_id}",
    tag = "items",
    security(("bearer_auth" = [])),
    params(
        ("item_id" = i32, Path, description = "Item ID"),
        ("specimen_id" = i32, Path, description = "Specimen ID"),
        ("force" = Option<bool>, Query, description = "Force delete even if borrowed")
    ),
    responses(
        (status = 204, description = "Specimen deleted"),
        (status = 404, description = "Specimen not found"),
        (status = 409, description = "Specimen is borrowed")
    )
)]
pub async fn delete_specimen(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path((item_id, specimen_id)): Path<(i64, i64)>,
    Query(params): Query<DeleteSpecimenParams>,
) -> AppResult<StatusCode> {
    claims.require_write_items()?;
    state
        .services
        .catalog
        .delete_specimen(item_id, specimen_id, params.force.unwrap_or(false))
        .await?;

    state.services.audit.log(
        audit::event::SPECIMEN_DELETED,
        Some(claims.user_id),
        Some("specimen"),
        Some(specimen_id),
        ip,
        Some(serde_json::json!({
            "item_id": item_id,
            "specimen_id": specimen_id,
            "force": params.force.unwrap_or(false),
        })),
    );

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct DeleteSpecimenParams {
    pub force: Option<bool>,
}
