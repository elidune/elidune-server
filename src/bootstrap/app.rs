//! Application composition: services, state, and HTTP router.

use std::sync::Arc;

use axum::Router;
use sqlx::{Pool, Postgres};
use tokio::sync::{broadcast, Notify};

use crate::{
    app::{build_app, build_app_with_options, AppBuildOptions},
    config::AppConfig,
    dynamic_config::DynamicConfig,
    email::EmailService,
    repository::Repository,
    services::{audit, operational_metrics, scheduler, Services},
    AppState,
};

/// Handle for waking background schedulers on config change.
pub type ShutdownHandle = ();

/// Fully wired application ready to serve HTTP.
pub struct AppBuildResult {
    pub state: AppState,
    pub router: Router,
    pub pool: Pool<Postgres>,
}

impl AppBuildResult {
    /// Build services, scheduler, and router from file config and database pool.
    pub async fn build(
        file_config: AppConfig,
        pool: Pool<Postgres>,
        options: AppBuildOptions,
    ) -> crate::error::AppResult<Self> {
        let dynamic_config = DynamicConfig::load_with_db_overrides(file_config.clone(), &pool).await;

        let redis_service =
            crate::services::redis::RedisService::new(&file_config.redis.url).await?;

        let email_service = Arc::new(EmailService::new(dynamic_config.clone(), pool.clone()));

        let (event_bus_tx, _) = broadcast::channel(256);
        let event_bus = crate::services::event_bus::EventBus::new(event_bus_tx.clone());

        let repository = Repository::new(pool.clone(), Some(dynamic_config.clone()));

        let services = Arc::new(
            Services::new(
                repository,
                file_config.users.clone(),
                dynamic_config.clone(),
                file_config.redis.clone(),
                redis_service,
                file_config.meilisearch.clone(),
                email_service,
                event_bus,
            )
            .await?,
        );
        operational_metrics::init_prometheus_recorder()?;

        services.audit.log(
            audit::event::SYSTEM_STARTUP,
            None,
            None,
            None,
            None,
            Some(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })),
            audit::AuditLogMeta::success(),
        );

        let scheduler_notify = scheduler::spawn(
            dynamic_config.clone(),
            services.reminders.clone(),
            services.audit.clone(),
            services.holds.clone(),
            services.email.clone(),
            services.repository.clone(),
        );

        let state = AppState {
            config: Arc::new(file_config),
            dynamic_config,
            services,
            scheduler_notify,
            event_bus: event_bus_tx,
        };

        let router = if options == AppBuildOptions::default() {
            build_app(state.clone())
        } else {
            build_app_with_options(state.clone(), options)
        };

        Ok(Self {
            state,
            router,
            pool,
        })
    }
}
