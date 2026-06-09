//! Elidune Server - Library Management System
//!
//! A modern Rust REST API server for library management.

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use elidune_server::{
    bootstrap::db::connect_pool,
    build_app,
    config::AppConfig,
    dynamic_config::DynamicConfig,
    services::{audit, event_bus::EventBus, operational_metrics, scheduler, Services},
    AppState,
};

/// Parse config path from args: --config <path> or -c <path>
fn config_path_from_args() -> Option<String> {
    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--config" || args[i] == "-c" {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        i += 1;
    }
    None
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // 1. File config (static sections: server, database, JWT, …)
    let file_config = AppConfig::load(config_path_from_args().as_deref())
        .expect("Failed to load configuration");
    file_config
        .validate_security()
        .expect("Invalid security configuration");

    // 2. Database (no tracing yet — effective logging comes from DB overrides)
    let pool = connect_pool(&file_config.database)
        .await
        .expect("Failed to connect to database");

    // 3. Merge DB overrides into dynamic config, then apply all runtime side effects
    let dynamic_config =
        DynamicConfig::load_with_db_overrides(file_config.clone(), &pool).await;
    let _config_guard = dynamic_config
        .apply(&pool)
        .await
        .expect("Failed to apply effective configuration");

    tracing::info!("Starting Elidune Server v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Connected to database and migrations completed");

    let redis_service = elidune_server::services::redis::RedisService::new(&file_config.redis.url)
        .await
        .expect("Failed to connect to Redis");
    tracing::info!("Connected to Redis");

    let server_host = file_config.server.host.clone();
    let server_port = file_config.server.port;

    let email_service = Arc::new(elidune_server::EmailService::new(
        dynamic_config.clone(),
        pool.clone(),
    ));

    let (event_bus_tx, _) = tokio::sync::broadcast::channel(256);
    let event_bus = EventBus::new(event_bus_tx.clone());

    let repository =
        elidune_server::repository::Repository::new(pool, Some(dynamic_config.clone()));
    let services = Services::new(
        repository,
        file_config.users.clone(),
        dynamic_config.clone(),
        file_config.redis.clone(),
        redis_service,
        file_config.meilisearch.clone(),
        email_service,
        event_bus,
    )
    .await
    .expect("Failed to create services");
    let services = Arc::new(services);
    operational_metrics::init_prometheus_recorder()
        .expect("Failed to initialize Prometheus metrics recorder");

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
        services: services.clone(),
        scheduler_notify,
        event_bus: event_bus_tx,
    };

    let app = build_app(state);
    let addr = SocketAddr::new(
        server_host.parse().expect("Invalid host address"),
        server_port,
    );

    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("Server has shut down cleanly");
    Ok(())
}

/// Waits for SIGTERM or SIGINT (Ctrl-C) and returns so that Axum can drain
/// in-flight requests before the process exits.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl-C, initiating graceful shutdown"),
        _ = terminate => tracing::info!("Received SIGTERM, initiating graceful shutdown"),
    }
}
