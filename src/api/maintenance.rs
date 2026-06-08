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
    error::{AppError, AppResult},
    models::task::TaskKind,
    services::audit,
    AppState,
};

pub use crate::services::maintenance_service::{
    CatalogZ3950RefreshProgress, CatalogZ3950RefreshProgressStatus, CatalogZ3950RefreshResult,
    MaintenanceAction, MaintenanceActionReport, MaintenanceResponse, MaintenanceTaskProgress,
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

// ─── Request types ────────────────────────────────────────────────────────────

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
    let maintenance = state.services.maintenance.clone();
    let user_id = claims.user_id;
    let actions = req.actions;

    let task_id = state
        .services
        .tasks
        .spawn_task(TaskKind::Maintenance, user_id, move |handle| async move {
            maintenance
                .run_maintenance_task(pool, actions, user_id, ip, handle)
                .await;
        })
        .await;

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
            "--if-exists",
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
        audit::AuditLogMeta::success(),
    );

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
async fn run_psql_sql_file(db_url: &str, sql_path: &Path) -> AppResult<()> {
    use std::process::Stdio;

    let db_url = db_url.to_string();
    let sql_path = sql_path.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("psql")
            .arg("--dbname")
            .arg(db_url)
            .args(["--variable", "ON_ERROR_STOP=1", "-f"])
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
        audit::AuditLogMeta::success(),
    );

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|e| AppError::Internal(format!("restore response: {e}")))
}
