//! Maintenance API — admin-only endpoint to run on-demand data-quality operations.
//!
//! ## Frontend (GUI) — task model
//!
//! All maintenance work runs as **one background task** (`POST /maintenance` → `taskId`,
//! `kind`: `maintenance`). Poll [`GET /tasks/:id`](crate::api::tasks::get_task).
//!
//! - **`progress`**: while `status === running`, read [`TaskProgress`]. The `message` field is
//!   always a JSON object shaped as [`MaintenanceTaskProgress`]: batch position (`step` /
//!   `totalSteps`), optional per-row position for long actions (`subStep` / `subTotal`), and
//!   optional `payload` (e.g. per-biblio Z39.50 step — same shape as [`CatalogZ3950RefreshProgress`]).
//! - **`result`**: when `status === completed`, a [`MaintenanceResponse`] whose `reports` array
//!   contains one [`MaintenanceActionReport`] per requested action. Each report **echoes** the
//!   [`MaintenanceAction`] that ran and puts structured outcomes in **`details`** (`serde_json::Value`):
//!   counter maps for DB cleanups, or a [`CatalogZ3950RefreshResult`] for Z39.50 refresh.
//!
//! ## Database backup / restore (admin only)
//!
//! - **`GET /maintenance/database/dump`** — downloads a plain SQL dump (`pg_dump --clean`, no owner/ACL).
//!   Requires PostgreSQL client tools on the server (`pg_dump`). Prefer a maintenance window; large DBs
//!   need long HTTP timeouts.
//! - **`POST /maintenance/database/restore`** — uploads the same plain SQL, writes it to a temp file
//!   under the system temp directory (usually `/tmp`), then runs `psql -f` on that file. **Destructive**
//!   when the script contains `DROP`/`CREATE` from `--clean` dumps. Stop other writers or restart the
//!   app after restore if connections fail mid-flight.

use std::path::Path;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{header, Response, StatusCode},
    routing::{get, post},
    Json,
};
use chrono::Utc;
use tokio_util::io::ReaderStream;
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;

use crate::{
    api::z3950::Z3950SearchQuery,
    error::{AppError, AppResult},
    models::{
        biblio::{Biblio, Isbn},
        task::TaskKind,
    },
    repository::{maintenance::MaintenanceDetail, maintenance::MaintenanceRepository, Repository},
    services::{
        audit,
        catalog::CatalogService,
        task_manager::TaskHandle,
        z3950::Z3950Service,
    },
    AppState,
};

use super::{tasks::TaskAcceptedResponse, AdminUser, ClientIp};

/// Maximum upload size for [`restore_database`] (this route raises Axum's body limit).
const MAX_RESTORE_SQL_BYTES: usize = 512 * 1024 * 1024;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/maintenance", post(run_maintenance))
        .route("/maintenance/database/dump", get(dump_database))
        .route(
            "/maintenance/database/restore",
            post(restore_database).layer(DefaultBodyLimit::max(MAX_RESTORE_SQL_BYTES)),
        )
}

// ─── Request / Response types ─────────────────────────────────────────────────

/// Single maintenance step. Prefer the **object** form with `"action"` (see examples); legacy
/// **string** form (`"cleanupSeries"`) is still accepted for backward compatibility.
///
/// ### Object form (recommended)
/// ```json
/// { "action": "cleanupSeries" }
/// ```
/// Z39.50 catalog refresh:
/// ```json
/// {
///   "action": "z3950Refresh",
///   "z3950ServerId": 1,
///   "forceRebuild": false
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MaintenanceAction {
    /// Strip surrounding double-quotes from series names; delete orphan series.
    CleanupSeries,
    /// Strip surrounding double-quotes from collection names; delete orphan collections.
    CleanupCollections,
    /// Delete authors not referenced by any biblio.
    CleanupOrphanAuthors,
    /// Merge series whose names are identical (case-insensitive, after trim).
    MergeDuplicateSeries,
    /// Merge collections whose names are identical (case-insensitive, after trim).
    MergeDuplicateCollections,
    /// Remove `biblio_series` rows that reference a non-existent series.
    CleanupDanglingBiblioSeries,
    /// Remove `biblio_collections` rows that reference a non-existent collection.
    CleanupDanglingBiblioCollections,
    /// Cleanup users (soft-deleted rows, city normalization, public type from birthdate).
    CleanupUsers,
    /// Re-fetch bibliographic data by ISBN from the given Z39.50 server (background sub-steps).
    Z3950Refresh {
        z3950_server_id: i64,
        #[serde(default)]
        rebuild_all: bool,
        #[serde(default)]
        biblio_ids: Option<Vec<i64>>,
    },
}

impl MaintenanceAction {
    /// Stable API name for this variant (camelCase), for logs and [`MaintenanceTaskProgress::action`].
    pub fn discriminant(&self) -> &'static str {
        match self {
            Self::CleanupSeries => "cleanupSeries",
            Self::CleanupCollections => "cleanupCollections",
            Self::CleanupOrphanAuthors => "cleanupOrphanAuthors",
            Self::MergeDuplicateSeries => "mergeDuplicateSeries",
            Self::MergeDuplicateCollections => "mergeDuplicateCollections",
            Self::CleanupDanglingBiblioSeries => "cleanupDanglingBiblioSeries",
            Self::CleanupDanglingBiblioCollections => "cleanupDanglingBiblioCollections",
            Self::CleanupUsers => "cleanupUsers",
            Self::Z3950Refresh { .. } => "z3950Refresh",
        }
    }
}

/// Request body for POST /maintenance.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceRequest {
    /// Ordered list of actions. Each entry may be a string (legacy) or an [`MaintenanceAction`] object.
    pub actions: Vec<MaintenanceAction>,
}

impl<'de> Deserialize<'de> for MaintenanceRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(deserialize_with = "deserialize_maintenance_action_list")]
            actions: Vec<MaintenanceAction>,
        }
        Raw::deserialize(deserializer).map(|r| MaintenanceRequest { actions: r.actions })
    }
}

fn deserialize_maintenance_action_list<'de, D>(deserializer: D) -> Result<Vec<MaintenanceAction>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LegacyOrTagged {
        Legacy(String),
        Tagged(MaintenanceAction),
    }

    let items = Vec::<LegacyOrTagged>::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match item {
            LegacyOrTagged::Tagged(a) => out.push(a),
            LegacyOrTagged::Legacy(s) => {
                out.push(parse_legacy_action(&s).map_err(serde::de::Error::custom)?);
            }
        }
    }
    Ok(out)
}

fn parse_legacy_action(s: &str) -> Result<MaintenanceAction, String> {
    match s {
        "cleanupSeries" => Ok(MaintenanceAction::CleanupSeries),
        "cleanupCollections" => Ok(MaintenanceAction::CleanupCollections),
        "cleanupOrphanAuthors" => Ok(MaintenanceAction::CleanupOrphanAuthors),
        "mergeDuplicateSeries" => Ok(MaintenanceAction::MergeDuplicateSeries),
        "mergeDuplicateCollections" => Ok(MaintenanceAction::MergeDuplicateCollections),
        "cleanupDanglingBiblioSeries" => Ok(MaintenanceAction::CleanupDanglingBiblioSeries),
        "cleanupDanglingBiblioCollections" => Ok(MaintenanceAction::CleanupDanglingBiblioCollections),
        "cleanupUsers" => Ok(MaintenanceAction::CleanupUsers),
        "z3950Refresh" => Err(
            "z3950Refresh requires an object with z3950ServerId (and optional forceRebuild)".into(),
        ),
        _ => Err(format!("unknown maintenance action: {s}")),
    }
}

/// Normalized [`TaskProgress::message`] for maintenance tasks (always an object).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceTaskProgress {
    /// Same as [`MaintenanceAction::discriminant`].
    pub action: String,
    /// 1-based index of the current action in the batch.
    pub step: usize,
    /// Number of actions in the batch.
    pub total_steps: usize,
    /// For long-running actions (Z39.50 refresh), 1-based index within that action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_step: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_total: Option<usize>,
    /// Per-action payload. For Z39.50: shape of [`CatalogZ3950RefreshProgress`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Result for a single maintenance action.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceActionReport {
    /// Echo of the action that was executed (same structure as in the request).
    pub action: MaintenanceAction,
    pub success: bool,
    /// Structured outcome. DB cleanups: string keys → integer counts. Z39.50: object matching [`CatalogZ3950RefreshResult`].
    pub details: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response body for POST /maintenance (task `result` when completed).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceResponse {
    pub reports: Vec<MaintenanceActionReport>,
}

// ─── Z39.50 types (details / progress payload) ───────────────────────────────

/// Progress payload for each biblio during Z39.50 refresh (`MaintenanceTaskProgress.payload`).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogZ3950RefreshProgress {
    pub biblio_id: i64,
    /// 1-based index within this Z39.50 action.
    pub index: usize,
    pub total: usize,
    pub status: CatalogZ3950RefreshProgressStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_biblio: Option<Biblio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_biblio: Option<Biblio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CatalogZ3950RefreshProgressStatus {
    Updated,
    NotFound,
    Failed,
}

/// Summary in [`MaintenanceActionReport::details`] for a completed Z39.50 refresh.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogZ3950RefreshResult {
    pub z3950_server_id: i64,
    pub rebuild_all: bool,
    pub total: usize,
    pub updated: i64,
    pub not_found: i64,
    pub failed: i64,
}

// ─── Handler ──────────────────────────────────────────────────────────────────

/// Run one or more maintenance actions (admin only).
///
/// Returns `202 Accepted` with a `taskId`. Poll `GET /tasks/:id` until `status` is `completed` or `failed`.
/// The `result` field contains a [`MaintenanceResponse`]. See module docs for [`MaintenanceTaskProgress`].
#[utoipa::path(
    post,
    path = "/maintenance",
    tag = "maintenance",
    security(("bearer_auth" = [])),
    request_body = MaintenanceRequest,
    responses(
        (status = 202, description = "Maintenance task accepted", body = TaskAcceptedResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Admin access required")
    )
)]
pub async fn run_maintenance(
    State(state): State<AppState>,
    AdminUser(claims): AdminUser,
    ClientIp(ip): ClientIp,
    Json(req): Json<MaintenanceRequest>,
) -> AppResult<(StatusCode, Json<TaskAcceptedResponse>)> {
    if req.actions.is_empty() {
        return Err(crate::error::AppError::Validation(
            "actions list must not be empty".into(),
        ));
    }

    let pool = state.services.repository_pool().clone();
    let catalog = state.services.catalog.clone();
    let z3950 = state.services.z3950.clone();
    let audit_svc = state.services.audit.clone();
    let user_id = claims.user_id;

    let task_id = state.services.tasks.spawn_task(
        TaskKind::Maintenance,
        user_id,
        move |handle| async move {
            let repo = Repository::new(pool, None, None);
            let total = req.actions.len();
            let mut reports = Vec::with_capacity(total);

            for (idx, action) in req.actions.iter().enumerate() {
                let progress_start = MaintenanceTaskProgress {
                    action: action.discriminant().to_string(),
                    step: idx + 1,
                    total_steps: total,
                    sub_step: None,
                    sub_total: None,
                    payload: None,
                };
                if let Ok(v) = serde_json::to_value(&progress_start) {
                    handle.set_progress(idx, total, Some(v)).await;
                }

                let result = dispatch_maintenance_action(
                    &repo,
                    &catalog,
                    &z3950,
                    action,
                    &handle,
                    idx,
                    total,
                )
                .await;

                let report = match result {
                    Ok(details) => {
                        tracing::info!(
                            action = action.discriminant(),
                            "maintenance action completed"
                        );
                        MaintenanceActionReport {
                            action: action.clone(),
                            success: true,
                            details,
                            error: None,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            action = action.discriminant(),
                            error = %e,
                            "maintenance action failed"
                        );
                        MaintenanceActionReport {
                            action: action.clone(),
                            success: false,
                            details: serde_json::json!({}),
                            error: Some(e.to_string()),
                        }
                    }
                };

                reports.push(report);
            }

            let action_count = reports.len();
            let failed = reports.iter().filter(|r| !r.success).count();
            let maint_meta = if failed == 0 {
                audit::AuditLogMeta::success()
            } else {
                audit::AuditLogMeta::failure_background(
                    crate::error::error_code::BUSINESS_RULE,
                    format!("{failed} of {action_count} maintenance actions failed"),
                )
            };

            audit_svc.log(
                audit::event::MAINTENANCE_RUN,
                Some(user_id),
                Some("maintenance"),
                None,
                ip,
                Some(serde_json::json!({
                    "actions": req.actions.iter().map(|a| serde_json::to_value(a).unwrap_or_default()).collect::<Vec<_>>(),
                })),
                maint_meta,
            );

            let response = MaintenanceResponse { reports };
            match serde_json::to_value(&response) {
                Ok(v) => handle.complete(v).await,
                Err(e) => handle.fail(format!("Serialisation error: {e}")).await,
            }
        },
    );

    Ok((StatusCode::ACCEPTED, Json(TaskAcceptedResponse { task_id })))
}

async fn pg_dump_plain_to_read_file(db_url: &str) -> AppResult<(tokio::fs::File, u64)> {
    use std::process::Stdio;
    use tokio::process::Command as TokioCommand;

    let path = std::env::temp_dir().join(format!("elidune-pg-dump-{}.sql", uuid::Uuid::new_v4()));
    let path_str = path
        .to_str()
        .ok_or_else(|| AppError::Internal("invalid temp dump path".into()))?;

    let output = TokioCommand::new("pg_dump")
        .args([
            "--format=plain",
            "--no-owner",
            "--no-acl",
            "--clean",
            "-f",
            path_str,
        ])
        .arg(db_url)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::Internal(
                    "pg_dump not found: install PostgreSQL client tools (e.g. postgresql-client)"
                        .into(),
                )
            } else {
                AppError::Internal(format!("pg_dump: {e}"))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!("pg_dump failed: {stderr}")));
    }

    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| AppError::Internal(format!("dump stat: {e}")))?;
    let len = meta.len();

    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| AppError::Internal(format!("open dump: {e}")))?;

    if let Err(e) = tokio::fs::remove_file(&path).await {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "could not unlink temp pg_dump file"
        );
    }

    Ok((file, len))
}

/// Download a full PostgreSQL plain-SQL dump (admin only). The file is produced with `pg_dump` then streamed to the client.
#[utoipa::path(
    get,
    path = "/maintenance/database/dump",
    tag = "maintenance",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "SQL dump", content_type = "application/sql"),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "pg_dump failed or client tools missing")
    )
)]
pub async fn dump_database(
    State(state): State<AppState>,
    AdminUser(claims): AdminUser,
    ClientIp(ip): ClientIp,
) -> AppResult<Response<Body>> {
    let db_url = state.config.database.url.as_str();
    let (file, byte_len) = pg_dump_plain_to_read_file(db_url).await?;

    state.services.audit.log(
        audit::event::MAINTENANCE_DATABASE_DUMP,
        Some(claims.user_id),
        Some("maintenance"),
        None,
        ip,
        Some(serde_json::json!({ "byteLength": byte_len })),
     audit::AuditLogMeta::success());

    let filename = format!(
        "elidune-db-dump-{}.sql",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );

    let body = Body::from_stream(ReaderStream::new(file));

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/sql")
        .header(
            header::CONTENT_DISPOSITION,
            format!(r#"attachment; filename="{filename}""#),
        )
        .body(body)
        .map_err(|e| AppError::Internal(format!("dump response: {e}")))
}

/// Deletes a temp restore SQL file on drop (best-effort).
struct RestoreTempFile(std::path::PathBuf);

impl Drop for RestoreTempFile {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.0) {
            tracing::warn!(
                path = %self.0.display(),
                error = %e,
                "failed to remove temp restore SQL file"
            );
        }
    }
}

/// Run `psql -f path.sql` (stdin closed).
///
/// Use the blocking process API in a blocking task: `tokio::process` relies on the async child
/// watcher and can surface SIGCHLD in debuggers. The standard `output()` path waits and captures
/// stdout/stderr without adding async child-signal handling to this request path.
async fn run_psql_sql_file(db_url: &str, sql_path: &Path) -> AppResult<()> {
    use std::process::Stdio;

    let db_url = db_url.to_string();
    let sql_path = sql_path.to_path_buf();
    println!("command to start psql: psql --dbname {} -v -f {}", db_url, sql_path.as_os_str().to_string_lossy());
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("psql")
            .arg("--dbname")
            .arg(db_url)
            .args(["-f"])
            .arg(sql_path.as_os_str())
            .stdin(Stdio::null())
            .output()
    })
    .await
    .map_err(|e| AppError::Internal(format!("psql task: {e}")))?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::Internal(
                "psql not found: install PostgreSQL client tools (e.g. postgresql-client package)"
                    .into(),
            )
        } else {
            AppError::Internal(format!("psql: {e}"))
        }
    })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        return Err(AppError::Internal(format!(
            "psql failed (status {status}): {err} {out}",
            status = output.status
        )));
    }

    Ok(())
}

/// Apply a plain SQL script with `psql` (admin only). Use the same format as [`dump_database`] (plain `pg_dump`).
#[utoipa::path(
    post,
    path = "/maintenance/database/restore",
    tag = "maintenance",
    security(("bearer_auth" = [])),
    responses(
        (status = 204, description = "SQL applied"),
        (status = 400, description = "Empty or oversize body"),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "psql failed or client tools missing")
    )
)]
pub async fn restore_database(
    State(state): State<AppState>,
    AdminUser(claims): AdminUser,
    ClientIp(ip): ClientIp,
    body: Body,
) -> AppResult<Response<Body>> {
    let bytes = axum::body::to_bytes(body, MAX_RESTORE_SQL_BYTES)
        .await
        .map_err(|e| {
            AppError::Validation(format!(
                "read body (max {} MiB): {e}",
                MAX_RESTORE_SQL_BYTES / (1024 * 1024)
            ))
        })?;

    if bytes.is_empty() {
        return Err(AppError::Validation("SQL body must not be empty".into()));
    }

    let db_url = state.config.database.url.as_str();

    let path = std::env::temp_dir().join(format!(
        "elidune-restore-{}.sql",
        uuid::Uuid::new_v4()
    ));

    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| AppError::Internal(format!("write temp restore file: {e}")))?;

    let _cleanup = RestoreTempFile(path.clone());

    run_psql_sql_file(db_url, &path).await?;

    state.services.audit.log(
        audit::event::MAINTENANCE_DATABASE_RESTORE,
        Some(claims.user_id),
        Some("maintenance"),
        None,
        ip,
        Some(serde_json::json!({ "byteLength": bytes.len() })),
     audit::AuditLogMeta::success());

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|e| AppError::Internal(format!("restore response: {e}")))
}

fn maintenance_detail_to_json(d: MaintenanceDetail) -> serde_json::Value {
    let m: serde_json::Map<String, serde_json::Value> = d
        .into_iter()
        .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
        .collect();
    serde_json::Value::Object(m)
}

/// Dispatches a single maintenance action. Z39.50 refresh reports fine-grained progress via `handle`.
async fn dispatch_maintenance_action(
    repo: &Repository,
    catalog: &CatalogService,
    z3950: &Z3950Service,
    action: &MaintenanceAction,
    handle: &TaskHandle,
    action_index: usize,
    total_actions: usize,
) -> crate::error::AppResult<serde_json::Value> {
    match action {
        MaintenanceAction::CleanupSeries => {
            let d = repo.maintenance_cleanup_series().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::CleanupCollections => {
            let d = repo.maintenance_cleanup_collections().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::CleanupOrphanAuthors => {
            let d = repo.maintenance_cleanup_authors().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::MergeDuplicateSeries => {
            let d = repo.maintenance_merge_duplicate_series().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::MergeDuplicateCollections => {
            let d = repo.maintenance_merge_duplicate_collections().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::CleanupDanglingBiblioSeries => {
            let d = repo.maintenance_cleanup_dangling_biblio_series().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::CleanupDanglingBiblioCollections => {
            let d = repo.maintenance_cleanup_dangling_biblio_collections().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::CleanupUsers => {
            let d = repo.maintenance_cleanup_users().await?;
            Ok(maintenance_detail_to_json(d))
        }
        MaintenanceAction::Z3950Refresh {
            z3950_server_id,
            rebuild_all,
            biblio_ids,
        } => {
            if *z3950_server_id <= 0 {
                return Err(crate::error::AppError::Validation(
                    "z3950ServerId must be positive".into(),
                ));
            }
            run_z3950_refresh_action(
                repo,
                catalog,
                z3950,
                *z3950_server_id,
                *rebuild_all,
                biblio_ids.as_ref(),
                handle,
                action_index,
                total_actions,
            )
            .await
        }
    }
}

async fn run_z3950_refresh_action(
    repo: &Repository,
    catalog: &CatalogService,
    z3950: &Z3950Service,
    server_id: i64,
    rebuild_all: bool,
    biblio_ids: Option<&Vec<i64>>,
    handle: &TaskHandle,
    action_index: usize,
    total_actions: usize,
) -> crate::error::AppResult<serde_json::Value> {

    let ids = match biblio_ids {
        Some(ids) => ids,
        None =>     &repo
            .biblios_list_ids_for_z3950_refresh(rebuild_all)
            .await?,
    };
       

    let total = ids.len();
    if total == 0 {
        return serde_json::to_value(&CatalogZ3950RefreshResult {
            z3950_server_id: server_id,
            rebuild_all,
            total: 0,
            updated: 0,
            not_found: 0,
            failed: 0,
        })
        .map_err(|e| {
            crate::error::AppError::Internal(format!("Z39.50 refresh result JSON: {}", e))
        });
    }

    let server = z3950.load_active_server(server_id).await?;
    let mut client = Z3950Service::connect_server(&server).await?;

    let mut updated: i64 = 0;
    let mut not_found: i64 = 0;
    let mut failed: i64 = 0;

    
    for (idx, biblio_id) in ids.iter().enumerate() {
        let make_progress = |sub: CatalogZ3950RefreshProgress| -> MaintenanceTaskProgress {
            MaintenanceTaskProgress {
                action: "z3950Refresh".to_string(),
                step: action_index + 1,
                total_steps: total_actions,
                sub_step: Some(idx + 1),
                sub_total: Some(total),
                payload: serde_json::to_value(&sub).ok(),
            }
        };

        let previous_biblio = match repo.biblios_get_by_id(*biblio_id).await {
            Ok(b) => b,
            Err(e) => {
                failed += 1;
                let prog = make_progress(CatalogZ3950RefreshProgress {
                    biblio_id: *biblio_id,
                    index: idx + 1,
                    total,
                    status: CatalogZ3950RefreshProgressStatus::Failed,
                    previous_biblio: None,
                    updated_biblio: None,
                    error: Some(format!("load biblio: {}", e)),
                });
                if let Ok(v) = serde_json::to_value(&prog) {
                    handle.set_progress(idx + 1, total.max(1), Some(v)).await;
                }
                continue;
            }
        };

        let prev_snapshot = previous_biblio.clone();
        let isbn_str = previous_biblio
            .isbn
            .as_ref()
            .map(|i| i.as_str().to_string())
            .unwrap_or_default();

        if isbn_str.is_empty() {
            failed += 1;
            let prog = make_progress(CatalogZ3950RefreshProgress {
                biblio_id: *biblio_id,
                index: idx + 1,
                total,
                status: CatalogZ3950RefreshProgressStatus::Failed,
                previous_biblio: Some(prev_snapshot),
                updated_biblio: None,
                error: Some("biblio has no ISBN".into()),
            });
            if let Ok(v) = serde_json::to_value(&prog) {
                handle.set_progress(idx + 1, total.max(1), Some(v)).await;
            }
            continue;
        }

        let isbn_norm = Isbn::new(&isbn_str);
        let cql = format!(r#"isbn="{}""#, isbn_norm.as_str());
        let search_query = Z3950SearchQuery {
            query: cql,
            server_id: Some(server_id),
            max_results: Some(1),
        };

        let remote = match Z3950Service::query(&mut client, &server, &search_query).await {
            Ok(mut recs) => recs.pop(),
            Err(e) => {
                failed += 1;
                let prog = make_progress(CatalogZ3950RefreshProgress {
                    biblio_id: *biblio_id,
                    index: idx + 1,
                    total,
                    status: CatalogZ3950RefreshProgressStatus::Failed,
                    previous_biblio: Some(prev_snapshot),
                    updated_biblio: None,
                    error: Some(e.to_string()),
                });
                if let Ok(v) = serde_json::to_value(&prog) {
                    handle.set_progress(idx + 1, total.max(1), Some(v)).await;
                }
                continue;
            }
        };

        let Some(marc) = remote else {
            not_found += 1;
            let prog = make_progress(CatalogZ3950RefreshProgress {
                biblio_id: *biblio_id,
                index: idx + 1,
                total,
                status: CatalogZ3950RefreshProgressStatus::NotFound,
                previous_biblio: Some(prev_snapshot),
                updated_biblio: None,
                error: None,
            });
            if let Ok(v) = serde_json::to_value(&prog) {
                handle.set_progress(idx + 1, total.max(1), Some(v)).await;
            }
            continue;
        };

        match catalog
            .refresh_biblio_from_z3950_marc(*biblio_id, marc)
            .await
        {
            Ok(new_biblio) => {
                updated += 1;
                let prog = make_progress(CatalogZ3950RefreshProgress {
                    biblio_id: *biblio_id,
                    index: idx + 1,
                    total,
                    status: CatalogZ3950RefreshProgressStatus::Updated,
                    previous_biblio: Some(prev_snapshot),
                    updated_biblio: Some(new_biblio),
                    error: None,
                });
                if let Ok(v) = serde_json::to_value(&prog) {
                    handle.set_progress(idx + 1, total.max(1), Some(v)).await;
                }
            }
            Err(e) => {
                failed += 1;
                let prog = make_progress(CatalogZ3950RefreshProgress {
                    biblio_id: *biblio_id,
                    index: idx + 1,
                    total,
                    status: CatalogZ3950RefreshProgressStatus::Failed,
                    previous_biblio: Some(prev_snapshot),
                    updated_biblio: None,
                    error: Some(e.to_string()),
                });
                if let Ok(v) = serde_json::to_value(&prog) {
                    handle.set_progress(idx + 1, total.max(1), Some(v)).await;
                }
            }
        }
    }

    let result = CatalogZ3950RefreshResult {
        z3950_server_id: server_id,
        rebuild_all,
        total,
        updated,
        not_found,
        failed,
    };
    let json = serde_json::to_value(&result);
    let _ = client.close().await;
    json.map_err(|e| {
        crate::error::AppError::Internal(format!("Z39.50 refresh result JSON: {}", e))
    })
}
