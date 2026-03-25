//! Biblio (catalog) and Item (physical copy) endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::Multipart;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::{
    error::{AppError, AppResult},
    models::{
        biblio::{Biblio, BiblioQuery, BiblioShort},
        import_report::ImportReport,
        item::Item,
    },
    models::task::TaskKind,
    services::{
        audit::{self},
        marc::{EnqueueResult, MarcBatchInfo},
    },
};

use super::{tasks::TaskAcceptedResponse, AuthenticatedUser, ClientIp, ValidatedJson};


/// Build the biblios and items routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        .route("/biblios", get(list_biblios).post(create_biblio))
        .route("/biblios/:id", get(get_biblio).put(update_biblio).delete(delete_biblio))
        .route("/biblios/:id/items", get(list_items).post(create_item).put(update_item))
        .route("/biblios/:biblio_id/items/:item_id", delete(delete_item))
        .route("/biblios/export.csv", get(export_biblios_csv))
        .route("/biblios/load-marc", post(load_marc))
        .route("/biblios/import-marc-batch", post(import_marc_batch))
        .route("/biblios/list-marc-batches", get(list_marc_batches))
        .route("/biblios/marc-batch/:batch_id", get(load_marc_batch))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetBiblioQuery {
    /// If true, include the full MARC record (marc_record JSONB) in the response
    #[serde(default)]
    pub full_record: bool,
}

/// Generic paginated response wrapper returned by list endpoints.
///
/// All list endpoints return this envelope so clients have a consistent way
/// to read pagination metadata without inspecting headers.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T>
where
    T: for<'a> ToSchema<'a>,
{
    /// Page contents
    pub items: Vec<T>,
    /// Total number of matching records across all pages
    pub total: i64,
    /// Current 1-based page number
    pub page: i64,
    /// Maximum records per page
    pub per_page: i64,
    /// Total number of pages (`ceil(total / per_page)`)
    pub page_count: i64,
}

impl<T: for<'a> ToSchema<'a>> PaginatedResponse<T> {
    /// Construct a paginated response, calculating `page_count` automatically.
    pub fn new(items: Vec<T>, total: i64, page: i64, per_page: i64) -> Self {
        let page_count = if per_page > 0 {
            (total + per_page - 1) / per_page
        } else {
            0
        };
        Self { items, total, page, per_page, page_count }
    }
}

/// List biblios with search and pagination
#[utoipa::path(
    get,
    path = "/biblios",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("media_type" = Option<String>, Query, description = "Filter by media type"),
        ("title" = Option<String>, Query, description = "Search in title"),
        ("author" = Option<String>, Query, description = "Search by author"),
        ("isbn" = Option<String>, Query, description = "Search by ISBN/ISSN"),
        ("freesearch" = Option<String>, Query, description = "Full-text search"),
        ("serie" = Option<String>, Query, description = "Filter by series name (substring)"),
        ("serie_id" = Option<i64>, Query, description = "Filter by series ID (exact match)"),
        ("collection" = Option<String>, Query, description = "Filter by collection name (substring)"),
        ("collection_id" = Option<i64>, Query, description = "Filter by collection ID (exact match)"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("per_page" = Option<i64>, Query, description = "Items per page (default: 20)")
    ),
    responses(
        (status = 200, description = "List of bibliographic records", body = PaginatedResponse<BiblioShort>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_biblios(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<BiblioQuery>,
) -> AppResult<Json<PaginatedResponse<BiblioShort>>> {
    claims.require_read_items()?;

    let (biblios, total) = state.services.catalog.search_biblios(&query).await?;
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);

    Ok(Json(PaginatedResponse::new(biblios, total, page, per_page)))
}

/// Get biblio details by ID
#[utoipa::path(
    get,
    path = "/biblios/{id}",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Biblio ID"),
        ("full_record" = Option<bool>, Query, description = "If true, include full MARC record data")
    ),
    responses(
        (status = 200, description = "Bibliographic record details", body = Biblio),
        (status = 404, description = "Biblio not found")
    )
)]
pub async fn get_biblio(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<i64>,
    Query(_query): Query<GetBiblioQuery>,
) -> AppResult<Json<Biblio>> {
    claims.require_read_items()?;

    let biblio = state.services.catalog.get_biblio(id).await?;
    Ok(Json(biblio))
}

/// Query params for create biblio
#[serde_as]
#[derive(Debug, Deserialize, Default, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateBiblioQuery {
    /// If true, allow creating a biblio even when another has the same ISBN
    #[serde(default)]
    pub allow_duplicate_isbn: bool,
    /// Set to the existing biblio ID to confirm replacement of a duplicate
    pub confirm_replace_existing_id: Option<i64>,
}

/// Response body for biblio creation (biblio + optional dedup report)
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateBiblioResponse {
    pub biblio: Biblio,
    pub import_report: ImportReport,
}

/// Query params for UNIMARC upload
#[derive(Debug, Deserialize)]
pub struct UploadUnimarcQuery {}

/// Query params for MARC batch import
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportMarcBatchQuery {
    /// Source ID
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub source_id: i64,
    /// Batch identifier returned by upload_unimarc
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub batch_id: i64,
    /// Optional record id inside the batch (e.g. "1", "2", ...)
    pub record_id: Option<usize>,
    /// If true, allow creating a biblio even when another has the same ISBN
    #[serde(default)]
    pub allow_duplicate_isbn: bool,
    /// Set to the existing biblio ID to confirm replacement of a duplicate
    pub confirm_replace_existing_id: Option<i64>,
}

/// Create a new bibliographic record (with ISBN deduplication)
#[utoipa::path(
    post,
    path = "/biblios",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("allow_duplicate_isbn" = Option<bool>, Query, description = "Allow duplicate ISBN (default: false)"),
        ("confirm_replace_existing_id" = Option<i64>, Query, description = "Confirm replacement of duplicate biblio")
    ),
    request_body = Biblio,
    responses(
        (status = 201, description = "Biblio created or merged", body = CreateBiblioResponse),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "Duplicate ISBN requires confirmation", body = crate::models::import_report::DuplicateConfirmationRequired)
    )
)]
pub async fn create_biblio(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Query(query): Query<CreateBiblioQuery>,
    Json(biblio): Json<Biblio>,
) -> AppResult<(StatusCode, Json<CreateBiblioResponse>)> {
    claims.require_write_items()?;
    let (biblio, import_report) = state
        .services
        .catalog
        .create_biblio(biblio, query.allow_duplicate_isbn, query.confirm_replace_existing_id)
        .await?;

    state.services.audit.log(
        audit::event::ITEM_CREATED,
        Some(claims.user_id),
        Some("biblio"),
        biblio.id,
        ip,
        Some(&biblio),
    );

    Ok((StatusCode::CREATED, Json(CreateBiblioResponse { biblio, import_report })))
}

/// Upload a UNIMARC file and return parsed biblios with linked items (995/952).
#[utoipa::path(
    post,
    path = "/biblios/load-marc",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("source_id" = i64, Query, description = "Source ID associated to this MARC batch")
    ),
    responses(
        (status = 200, description = "Parsed biblios with physical items", body = EnqueueResult),
        (status = 400, description = "Missing file or invalid UNIMARC"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn load_marc(
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

    let enqueue_result = state.services.marc.enqueue_unimarc_batch(&data).await?;

    Ok(Json(enqueue_result))
}

/// Import cached MARC records from a batch into the catalog.
///
/// Returns `202 Accepted` immediately with a `taskId`.  Poll `GET /tasks/:id`
/// until `status` is `completed` or `failed`.  The `result` field of the
/// completed task contains a `MarcBatchImportReport`.
#[utoipa::path(
    post,
    path = "/biblios/import-marc-batch",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("batch_id" = String, Query, description = "MARC batch identifier returned by load-marc"),
        ("source_id" = String, Query, description = "Source ID to attach to imported biblios"),
        ("record_id" = Option<usize>, Query, description = "Optional single record index; if omitted, all records in the batch are imported"),
        ("allow_duplicate_isbn" = Option<bool>, Query, description = "Allow creating a biblio even when another has the same ISBN (default: false)"),
        ("confirm_replace_existing_id" = Option<i64>, Query, description = "Confirm replacement of an existing biblio by its ID")
    ),
    responses(
        (status = 202, description = "Import task accepted", body = TaskAcceptedResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn import_marc_batch(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Query(params): Query<ImportMarcBatchQuery>,
) -> AppResult<(StatusCode, Json<TaskAcceptedResponse>)> {
    claims.require_write_items()?;

    let marc = state.services.marc.clone();
    let audit = state.services.audit.clone();
    let p = params.clone();

    let task_id = state.services.tasks.spawn_task(
        TaskKind::MarcBatchImport,
        claims.user_id,
        move |handle| async move {
            match marc
                .import_from_batch(
                    p.batch_id,
                    p.source_id,
                    p.record_id,
                    p.allow_duplicate_isbn,
                    p.confirm_replace_existing_id,
                    Some(handle.clone()),
                )
                .await
            {
                Ok(report) => {
                    audit.log(
                        audit::event::IMPORT_MARC_BATCH,
                        Some(claims.user_id),
                        None,
                        None,
                        ip,
                        Some(&p),
                    );
                    let result = serde_json::to_value(&report).unwrap_or_default();
                    handle.complete(result).await;
                }
                Err(e) => handle.fail(e.to_string()).await,
            }
        },
    );

    Ok((StatusCode::ACCEPTED, Json(TaskAcceptedResponse { task_id })))
}

/// Update an existing bibliographic record
#[utoipa::path(
    put,
    path = "/biblios/{id}",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Biblio ID"),
        ("allow_duplicate_isbn" = Option<bool>, Query, description = "Allow duplicate ISBN (default: false)")
    ),
    request_body = Biblio,
    responses(
        (status = 200, description = "Biblio updated", body = Biblio),
        (status = 404, description = "Biblio not found"),
        (status = 409, description = "Duplicate ISBN requires confirmation")
    )
)]
pub async fn update_biblio(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Query(query): Query<UpdateBiblioQuery>,
    Json(biblio): Json<Biblio>,
) -> AppResult<Json<Biblio>> {
    claims.require_write_items()?;
    let updated = state.services.catalog.update_biblio(id, biblio, query.allow_duplicate_isbn).await?;

    state.services.audit.log(
        audit::event::ITEM_UPDATED,
        Some(claims.user_id),
        Some("biblio"),
        Some(id),
        ip,
        Some((id, &updated)),
    );

    Ok(Json(updated))
}

#[derive(Debug, Deserialize, Default, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBiblioQuery {
    #[serde(default)]
    pub allow_duplicate_isbn: bool,
}

/// Delete a bibliographic record
#[utoipa::path(
    delete,
    path = "/biblios/{id}",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Biblio ID"),
        ("force" = Option<bool>, Query, description = "Force delete even if physical items are borrowed")
    ),
    responses(
        (status = 204, description = "Biblio deleted"),
        (status = 404, description = "Biblio not found"),
        (status = 409, description = "Biblio has borrowed items")
    )
)]
pub async fn delete_biblio(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Query(params): Query<DeleteBiblioParams>,
) -> AppResult<StatusCode> {
    claims.require_write_items()?;
    state
        .services
        .catalog
        .delete_biblio(id, params.force.unwrap_or(false))
        .await?;

    state.services.audit.log(
        audit::event::ITEM_DELETED,
        Some(claims.user_id),
        Some("biblio"),
        Some(id),
        ip,
        Some(serde_json::json!({ "id": id, "force": params.force.unwrap_or(false) })),
    );

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBiblioParams {
    pub force: Option<bool>,
}

/// List physical items for a bibliographic record
#[utoipa::path(
    get,
    path = "/biblios/{id}/items",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Biblio ID")
    ),
    responses(
        (status = 200, description = "List of physical items (copies)", body = Vec<Item>),
        (status = 404, description = "Biblio not found")
    )
)]
pub async fn list_items(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(biblio_id): Path<i64>,
) -> AppResult<Json<Vec<Item>>> {
    claims.require_read_items()?;

    let items = state.services.catalog.get_items(biblio_id).await?;
    Ok(Json(items))
}

/// Create a new physical item for a bibliographic record
#[utoipa::path(
    post,
    path = "/biblios/{id}/items",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("id" = i64, Path, description = "Biblio ID")
    ),
    request_body = Item,
    responses(
        (status = 201, description = "Physical item created", body = Item),
        (status = 404, description = "Biblio not found"),
        (status = 409, description = "An item with this barcode already exists", body = crate::models::import_report::DuplicateItemBarcodeRequired)
    )
)]
pub async fn create_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(biblio_id): Path<i64>,
    ValidatedJson(item): ValidatedJson<Item>,
) -> AppResult<(StatusCode, Json<Item>)> {
    claims.require_write_items()?;
    let created = state
        .services
        .catalog
        .create_item(biblio_id, item)
        .await?;

    state.services.audit.log(
        audit::event::SPECIMEN_CREATED,
        Some(claims.user_id),
        Some("item"),
        created.id,
        ip,
        Some((biblio_id, &created)),
    );

    Ok((StatusCode::CREATED, Json(created)))
}

/// Update a physical item
#[utoipa::path(
    put,
    path = "/biblios/{biblio_id}/items/{item_id}",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("biblio_id" = i64, Path, description = "Biblio ID"),
        ("item_id" = i64, Path, description = "Physical item ID")
    ),
    request_body = Item,
    responses(
        (status = 200, description = "Physical item updated", body = Item),
        (status = 404, description = "Biblio or item not found"),
        (status = 409, description = "An item with this barcode already exists")
    )
)]
pub async fn update_item(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    ClientIp(ip): ClientIp,
    Path(biblio_id): Path<i64>,
    ValidatedJson(mut item): ValidatedJson<Item>,
) -> AppResult<Json<Item>> {
    claims.require_write_items()?;
    let item_id = item.id;
    state
        .services
        .catalog
        .update_item(biblio_id, &mut item)
        .await?;

    state.services.audit.log(
        audit::event::SPECIMEN_UPDATED,
        Some(claims.user_id),
        Some("item"),
        item_id,
        ip,
        Some((biblio_id, &item)),
    );

    Ok(Json(item))
}

/// Delete a physical item
#[utoipa::path(
    delete,
    path = "/biblios/{biblio_id}/items/{item_id}",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("biblio_id" = i64, Path, description = "Biblio ID"),
        ("item_id" = i64, Path, description = "Physical item ID"),
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
    Path((biblio_id, item_id)): Path<(i64, i64)>,
    Query(params): Query<DeleteItemParams>,
) -> AppResult<StatusCode> {
    claims.require_write_items()?;
    state
        .services
        .catalog
        .delete_item(biblio_id, item_id, params.force.unwrap_or(false))
        .await?;

    state.services.audit.log(
        audit::event::SPECIMEN_DELETED,
        Some(claims.user_id),
        Some("item"),
        Some(item_id),
        ip,
        Some(serde_json::json!({
            "biblio_id": biblio_id,
            "item_id": item_id,
            "force": params.force.unwrap_or(false),
        })),
    );

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteItemParams {
    pub force: Option<bool>,
}

/// List all MARC batches currently cached in Redis.
#[utoipa::path(
    get,
    path = "/biblios/list-marc-batches",
    tag = "biblios",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of cached MARC batches", body = Vec<MarcBatchInfo>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_marc_batches(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<Vec<MarcBatchInfo>>> {
    claims.require_read_items()?;
    let batches = state.services.marc.list_marc_batches().await?;
    Ok(Json(batches))
}

/// Reload a cached MARC batch from Redis by its batch ID.
#[utoipa::path(
    get,
    path = "/biblios/marc-batch/{batch_id}",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("batch_id" = String, Path, description = "MARC batch identifier (Snowflake ID as string)")
    ),
    responses(
        (status = 200, description = "Cached MARC batch contents", body = EnqueueResult),
        (status = 404, description = "Batch not found or expired"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn load_marc_batch(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(batch_id): Path<i64>,
) -> AppResult<Json<EnqueueResult>> {
    claims.require_read_items()?;
    let result = state.services.marc.load_marc_batch(batch_id).await?;
    Ok(Json(result))
}

/// Export catalog as CSV
///
/// Returns a UTF-8 CSV file with all bibliographic records matching the query.
/// Streams all pages — does not paginate. Use the same query params as `GET /biblios`.
#[utoipa::path(
    get,
    path = "/biblios/export.csv",
    tag = "biblios",
    security(("bearer_auth" = [])),
    params(
        ("title" = Option<String>, Query, description = "Filter by title"),
        ("author" = Option<String>, Query, description = "Filter by author"),
        ("media_type" = Option<String>, Query, description = "Filter by media type")
    ),
    responses(
        (status = 200, description = "CSV file", content_type = "text/csv"),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse)
    )
)]
pub async fn export_biblios_csv(
    State(state): State<crate::AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(mut query): Query<BiblioQuery>,
) -> AppResult<axum::response::Response> {
    claims.require_read_catalog()?;

    query.page = Some(1);
    query.per_page = Some(10_000);

    let (biblios, _) = state.services.catalog.search_biblios(&query).await?;

    let mut csv = String::from("id,isbn,title,author,media_type,date,items\n");
    for biblio in &biblios {
        let author_name = biblio
            .author
            .as_ref()
            .map(|a| {
                format!(
                    "{} {}",
                    a.firstname.as_deref().unwrap_or(""),
                    a.lastname.as_deref().unwrap_or("")
                )
                .trim()
                .to_string()
            })
            .unwrap_or_default();
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            biblio.id,
            escape_csv(biblio.isbn.as_ref().map(|i| i.as_str()).unwrap_or("")),
            escape_csv(biblio.title.as_deref().unwrap_or("")),
            escape_csv(&author_name),
            escape_csv(biblio.media_type.as_db_str()),
            escape_csv(biblio.date.as_deref().unwrap_or("")),
            biblio.items.len(),
        ));
    }

    use axum::http::header;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"catalog.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}

fn escape_csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}


