//! Maintenance API — admin-only endpoint to run on-demand data-quality operations.

use std::collections::BTreeMap;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    models::task::TaskKind,
    repository::{maintenance::MaintenanceRepository, Repository},
    services::audit,
    AppState,
};

use super::{tasks::TaskAcceptedResponse, AdminUser, ClientIp};

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<AppState> {
    use axum::routing::post;
    axum::Router::new().route("/maintenance", post(run_maintenance))
}


// ─── Request / Response types ─────────────────────────────────────────────────

/// Maintenance action identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
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
    /// Cleanup users
    CleanupUsers,
}

impl MaintenanceAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::CleanupSeries => "cleanupSeries",
            Self::CleanupCollections => "cleanupCollections",
            Self::CleanupOrphanAuthors => "cleanupOrphanAuthors",
            Self::MergeDuplicateSeries => "mergeDuplicateSeries",
            Self::MergeDuplicateCollections => "mergeDuplicateCollections",
            Self::CleanupDanglingBiblioSeries => "cleanupDanglingBiblioSeries",
            Self::CleanupDanglingBiblioCollections => "cleanupDanglingBiblioCollections",
            Self::CleanupUsers => "cleanupUsers",
        }
    }
}

/// Request body for POST /maintenance.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceRequest {
    /// List of actions to execute (in order).
    pub actions: Vec<MaintenanceAction>,
}

/// Result for a single maintenance action.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceActionReport {
    /// Action identifier.
    pub action: String,
    /// Whether the action completed without error.
    pub success: bool,
    /// Named counters describing what was done (e.g. `orphans_deleted: 3`).
    pub details: BTreeMap<String, i64>,
    /// Error message if `success` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response body for POST /maintenance.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceResponse {
    pub reports: Vec<MaintenanceActionReport>,
}

// ─── Handler ──────────────────────────────────────────────────────────────────

/// Run one or more maintenance actions (admin only).
///
/// Returns `202 Accepted` immediately with a `taskId`.  Poll `GET /tasks/:id`
/// until `status` is `completed` or `failed`.  The `result` field of the
/// completed task contains a `MaintenanceResponse` with per-action reports.
///
/// Each action is executed sequentially and independently — a failure in one
/// action does not prevent the others from running.
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
    let audit = state.services.audit.clone();

    let task_id = state.services.tasks.spawn_task(
        TaskKind::Maintenance,
        claims.user_id,
        move |handle| async move {
            let repo = Repository::new(pool);
            let total = req.actions.len();
            let mut reports = Vec::with_capacity(total);

            for (idx, action) in req.actions.iter().enumerate() {
                handle
                    .set_progress(
                        idx,
                        total,
                        Some(serde_json::json!({
                            "action": action,
                            "step": idx + 1,
                            "total": total,
                        })),
                    )
                    .await;

                let result = dispatch_action(&repo, *action).await;

                let report = match result {
                    Ok(detail) => {
                        let details: BTreeMap<String, i64> =
                            detail.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
                        tracing::info!(
                            action = action.as_str(),
                            ?details,
                            "maintenance action completed"
                        );
                        MaintenanceActionReport {
                            action: action.as_str().to_string(),
                            success: true,
                            details,
                            error: None,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            action = action.as_str(),
                            error = %e,
                            "maintenance action failed"
                        );
                        MaintenanceActionReport {
                            action: action.as_str().to_string(),
                            success: false,
                            details: BTreeMap::new(),
                            error: Some(e.to_string()),
                        }
                    }
                };

                reports.push(report);
            }

            audit.log(
                audit::event::MAINTENANCE_RUN,
                Some(claims.user_id),
                Some("maintenance"),
                None,
                ip,
                Some(serde_json::json!({
                    "actions": req.actions.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
                })),
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

async fn dispatch_action(
    repo: &Repository,
    action: MaintenanceAction,
) -> crate::error::AppResult<crate::repository::maintenance::MaintenanceDetail> {
    match action {
        MaintenanceAction::CleanupSeries => repo.maintenance_cleanup_series().await,
        MaintenanceAction::CleanupCollections => repo.maintenance_cleanup_collections().await,
        MaintenanceAction::CleanupOrphanAuthors => repo.maintenance_cleanup_authors().await,
        MaintenanceAction::MergeDuplicateSeries => repo.maintenance_merge_duplicate_series().await,
        MaintenanceAction::MergeDuplicateCollections => repo.maintenance_merge_duplicate_collections().await,
        MaintenanceAction::CleanupDanglingBiblioSeries => repo.maintenance_cleanup_dangling_biblio_series().await,
        MaintenanceAction::CleanupDanglingBiblioCollections => repo.maintenance_cleanup_dangling_biblio_collections().await,
        MaintenanceAction::CleanupUsers => repo.maintenance_cleanup_users().await,
    }
}

