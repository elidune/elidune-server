//! Prometheus operational metrics for scheduler and queue health.

use std::sync::OnceLock;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::{
    error::{AppError, AppResult},
    repository::Repository,
};

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder once.
pub fn init_prometheus_recorder() -> AppResult<()> {
    if PROMETHEUS_HANDLE.get().is_some() {
        return Ok(());
    }

    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| AppError::Internal(format!("failed to install prometheus recorder: {e}")))?;

    let _ = PROMETHEUS_HANDLE.set(handle);
    Ok(())
}

/// Record the latest run timestamp for a scheduler loop.
pub fn record_scheduler_run(kind: &'static str, unix_ts: i64) {
    metrics::gauge!("elidune_scheduler_last_run_unix_seconds", "kind" => kind).set(unix_ts as f64);
}

/// Update snapshot gauges and return OpenMetrics/Prometheus text payload.
pub async fn gather_handler(repository: &Repository) -> AppResult<String> {
    let snapshot = repository.metrics_snapshot().await?;

    metrics::gauge!("elidune_active_loans").set(snapshot.active_loans as f64);
    metrics::gauge!("elidune_pending_holds").set(snapshot.pending_holds as f64);
    metrics::gauge!("elidune_outbox_pending_count").set(snapshot.outbox_pending_count as f64);
    metrics::gauge!("elidune_outbox_oldest_pending_seconds")
        .set(snapshot.outbox_oldest_pending_seconds as f64);

    let rendered = PROMETHEUS_HANDLE
        .get()
        .ok_or_else(|| AppError::Internal("prometheus recorder not initialized".to_string()))?
        .render();

    Ok(rendered)
}
