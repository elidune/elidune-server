//! Maintenance orchestration — data-quality batch operations run as background tasks.

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use utoipa::ToSchema;

use crate::{
    models::dto::z3950::Z3950SearchQuery,
    error::AppResult,
    models::biblio::{Biblio, Isbn},
    repository::{maintenance::MaintenanceDetail, maintenance::MaintenanceRepository, Repository},
    services::{
        audit::{self, AuditService},
        catalog::CatalogService,
        task_manager::TaskHandle,
        z3950::Z3950Service,
    },
};

/// Single maintenance step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MaintenanceAction {
    CleanupSeries,
    CleanupCollections,
    CleanupOrphanAuthors,
    MergeDuplicateSeries,
    MergeDuplicateCollections,
    CleanupDanglingBiblioSeries,
    CleanupDanglingBiblioCollections,
    CleanupUsers,
    Z3950Refresh {
        z3950_server_id: i64,
        #[serde(default)]
        rebuild_all: bool,
        #[serde(default)]
        biblio_ids: Option<Vec<i64>>,
    },
}

impl MaintenanceAction {
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

/// Normalized task progress message for maintenance tasks.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceTaskProgress {
    pub action: String,
    pub step: usize,
    pub total_steps: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_step: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Result for a single maintenance action.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceActionReport {
    pub action: MaintenanceAction,
    pub success: bool,
    pub details: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregated maintenance task result.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceResponse {
    pub reports: Vec<MaintenanceActionReport>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogZ3950RefreshProgress {
    pub biblio_id: i64,
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

#[derive(Clone)]
pub struct MaintenanceService {
    catalog: CatalogService,
    z3950: Z3950Service,
    audit: AuditService,
}

impl MaintenanceService {
    pub fn new(catalog: CatalogService, z3950: Z3950Service, audit: AuditService) -> Self {
        Self { catalog, z3950, audit }
    }

    /// Run the ordered maintenance actions, reporting progress via `handle`.
    pub async fn run_maintenance_task(
        &self,
        pool: Pool<Postgres>,
        actions: Vec<MaintenanceAction>,
        user_id: i64,
        ip: Option<String>,
        handle: TaskHandle,
    ) {
        let repo = Repository::new(pool, None);
        let total = actions.len();
        let mut reports = Vec::with_capacity(total);

        for (idx, action) in actions.iter().enumerate() {
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
                &self.catalog,
                &self.z3950,
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

        self.audit.log(
            audit::event::MAINTENANCE_RUN,
            Some(user_id),
            Some("maintenance"),
            None,
            ip,
            Some(serde_json::json!({
                "actions": actions.iter().map(|a| serde_json::to_value(a).unwrap_or_default()).collect::<Vec<_>>(),
            })),
            maint_meta,
        );

        let response = MaintenanceResponse { reports };
        match serde_json::to_value(&response) {
            Ok(v) => handle.complete(v).await,
            Err(e) => handle.fail(format!("Serialisation error: {e}")).await,
        }
    }
}

fn maintenance_detail_to_json(d: MaintenanceDetail) -> serde_json::Value {
    let m: serde_json::Map<String, serde_json::Value> = d
        .into_iter()
        .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
        .collect();
    serde_json::Value::Object(m)
}

async fn dispatch_maintenance_action(
    repo: &Repository,
    catalog: &CatalogService,
    z3950: &Z3950Service,
    action: &MaintenanceAction,
    handle: &TaskHandle,
    action_index: usize,
    total_actions: usize,
) -> AppResult<serde_json::Value> {
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
) -> AppResult<serde_json::Value> {
    let ids = match biblio_ids {
        Some(ids) => ids,
        None => &repo.biblios_list_ids_for_z3950_refresh(rebuild_all).await?,
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
        .map_err(|e| crate::error::AppError::Internal(format!("Z39.50 refresh result JSON: {e}")));
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
                    error: Some(format!("load biblio: {e}")),
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
    json.map_err(|e| crate::error::AppError::Internal(format!("Z39.50 refresh result JSON: {e}")))
}
