//! Elidune Server - Library Management System
//!
//! A modern Rust REST API server for library management.

use axum::{routing::get, Router};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::trace::TraceLayer;
use std::path::Path;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, Layer, reload};

use elidune_server::{
    api,
    config::AppConfig,
    dynamic_config::DynamicConfig,
    repository::Repository,
    services::{audit, Services},
    AppState,
};

/// Build a boxed fmt layer writing to any `MakeWriter` (stdout / stderr).
fn build_fmt_layer<S, W>(format: &str, writer: W) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    W: for<'w> fmt::MakeWriter<'w> + Send + Sync + 'static,
{
    match format {
        "json"  => Box::new(fmt::layer().json().with_writer(writer)),
        "plain" => Box::new(fmt::layer().compact().with_ansi(false).with_writer(writer)),
        _       => Box::new(fmt::layer().with_writer(writer)),
    }
}

/// Build a boxed fmt layer writing to a `NonBlocking` appender (file output).
fn build_fmt_layer_writer<S>(
    format: &str,
    writer: tracing_appender::non_blocking::NonBlocking,
) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    match format {
        "json"  => Box::new(fmt::layer().json().with_writer(writer).with_ansi(false)),
        "plain" => Box::new(fmt::layer().compact().with_ansi(false).with_writer(writer)),
        _       => Box::new(fmt::layer().with_ansi(false).with_writer(writer)),
    }
}

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
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Load configuration
    let config = config_path_from_args()
        .map(|path| AppConfig::load(&path))
        .ok_or_else(|| anyhow::anyhow!("No configuration path provided"))??;

    // Initialize tracing
    let initial_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("elidune_server={},tower_http=debug,z3950_rs=debug", config.logging.level).into());

    // Wrap the filter in a reload layer so the log level can be changed at runtime
    let (filter_layer, reload_handle) = reload::Layer::new(initial_filter);

    let _appender_guard;

    let log_output = config.logging.output.as_str();
    let log_format = config.logging.format.as_str();

    let log_layer: Box<dyn Layer<_> + Send + Sync> = match log_output {
        "syslog" => {
            match tracing_journald::layer() {
                Ok(layer) => Box::new(layer),
                Err(e) => {
                    eprintln!("Failed to connect to journald: {e}. Falling back to stdout.");
                    build_fmt_layer(log_format, std::io::stdout)
                }
            }
        }
        "stderr" => build_fmt_layer(log_format, std::io::stderr),
        "file" => {
            let file_path = config.logging.file_path.as_deref()
                .ok_or_else(|| anyhow::anyhow!(
                    "logging.output = \"file\" requires logging.file_path to be set"
                ))?;
            let rotation_str = config.logging.file_rotation.as_deref().unwrap_or("daily");
            let rotation = match rotation_str {
                "hourly" => tracing_appender::rolling::Rotation::HOURLY,
                "never"  => tracing_appender::rolling::Rotation::NEVER,
                _        => tracing_appender::rolling::Rotation::DAILY,
            };
            let dir = Path::new(file_path).parent().unwrap_or(Path::new("."));
            let filename = Path::new(file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("elidune.log");
            let file_appender = tracing_appender::rolling::RollingFileAppender::new(rotation, dir, filename);
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            _appender_guard = guard;
            build_fmt_layer_writer(log_format, non_blocking)
        }
        _ => build_fmt_layer(log_format, std::io::stdout),
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(log_layer)
        .init();

    tracing::info!("Starting Elidune Server v{}", env!("CARGO_PKG_VERSION"));

    // Create database connection pool
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .connect(&config.database.url)
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    // Run migrations
    // sqlx::migrate!("./migrations")
    //     .run(&pool)
    //     .await
    //     .expect("Failed to run database migrations");
    // tracing::info!("Database migrations completed");

    // Load DB settings overrides and build DynamicConfig
    let dynamic_config = {
        let mut merged = config.clone();

        let db_overrides: Vec<(String, serde_json::Value)> =
            sqlx::query_as::<_, (String, serde_json::Value)>(
                "SELECT key, value FROM settings",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

        for (key, value) in db_overrides {
            let overridable = match key.as_str() {
                "email" => config.email.overridable,
                "logging" => config.logging.overridable,
                "reminders" => config.reminders.overridable,
                "audit" => config.audit.overridable,
                _ => false,
            };
            if !overridable {
                tracing::warn!("DB settings: section '{}' is not overridable, skipping", key);
                continue;
            }
            match key.as_str() {
                "email" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        merged.email = v;
                        tracing::info!("DB settings: overriding [email]");
                    }
                }
                "logging" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        merged.logging = v;
                        tracing::info!("DB settings: overriding [logging]");
                    }
                }
                "reminders" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        merged.reminders = v;
                        tracing::info!("DB settings: overriding [reminders]");
                    }
                }
                "audit" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        merged.audit = v;
                        tracing::info!("DB settings: overriding [audit]");
                    }
                }
                _ => {}
            }
        }

        DynamicConfig::new(merged)
    };

    // Register the log level reload callback so admin API updates take effect immediately
    let reload_handle_cb = reload_handle.clone();
    dynamic_config.set_log_level_reload(Box::new(move |level: &str| {
        let new_filter = tracing_subscriber::EnvFilter::new(
            format!("elidune_server={},tower_http=debug,z3950_rs=debug", level)
        );
        reload_handle_cb.reload(new_filter).map_err(|e| e.to_string())
    }));

    // Apply DB-overridden log level at startup (if different from the file config)
    let effective_level = dynamic_config.read_logging().level;
    if effective_level != config.logging.level {
        let startup_filter = tracing_subscriber::EnvFilter::new(
            format!("elidune_server={},tower_http=debug,z3950_rs=debug", effective_level)
        );
        if let Err(e) = reload_handle.reload(startup_filter) {
            tracing::warn!("Failed to apply DB log level override at startup: {}", e);
        } else {
            tracing::info!("Applied DB log level override at startup: '{}'", effective_level);
        }
    }

    // Initialize Redis connection
    let redis_service = elidune_server::services::redis::RedisService::new(&config.redis.url)
        .await
        .expect("Failed to connect to Redis");

    tracing::info!("Connected to Redis");

    // Save server address before moving config
    let server_host = config.server.host.clone();
    let server_port = config.server.port;

    // Create repository and services
    let repository = Repository::new(pool);
    let services = Services::new(
        repository,
        config.users.clone(),
        dynamic_config.clone(),
        config.redis.clone(),
        redis_service,
        config.meilisearch.clone(),
    )
    .await
    .expect("Failed to create services");

    let services = Arc::new(services);

    // Seed default admin user if the users table is empty (first run)
    match services.users.seed_admin_if_empty().await {
        Ok(Some((login, password))) => {
            tracing::warn!(
                "╔══════════════════════════════════════════════════════╗"
            );
            tracing::warn!(
                "║          INITIAL ADMIN ACCOUNT CREATED               ║"
            );
            tracing::warn!(
                "║  Login    : {:<41}║", login
            );
            tracing::warn!(
                "║  Password : {:<41}║", password
            );
            tracing::warn!(
                "║  Change the password immediately after first login.  ║"
            );
            tracing::warn!(
                "╚══════════════════════════════════════════════════════╝"
            );
        }
        Ok(None) => {}
        Err(e) => tracing::error!("Failed to seed admin user: {}", e),
    }

    // Log system startup audit event
    services.audit.log(
        audit::event::SYSTEM_STARTUP,
        None,
        None,
        None,
        None,
        Some(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })),
    );

    // Start background scheduler (reminder sender + audit cleanup)
    let scheduler_notify = elidune_server::services::scheduler::spawn(
        dynamic_config.clone(),
        services.reminders.clone(),
        services.audit.clone(),
    );

    // Broadcast channel for SSE real-time events (capacity = 256 messages)
    let (event_bus, _) = tokio::sync::broadcast::channel(256);

    // Create application state
    let state = AppState {
        config: Arc::new(config),
        dynamic_config,
        services: services.clone(),
        scheduler_notify,
        event_bus,
    };

    // Build router
    let app = create_router(state);

    // Start server
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

/// Create the application router with all routes.
///
/// Each domain's routes are registered in its own `api::<domain>::router()` function.
/// `main.rs` only merges them under `/api/v1` and applies middleware.
fn create_router(state: AppState) -> Router {
    let cors = build_cors(&state.config);

    // Rate-limit auth endpoints: burst of 2, replenish 1 per 4s by default (secure preset).
    let per_second = state.config.server.auth_rate_per_second.unwrap_or(4);
    let burst_size = state.config.server.auth_rate_burst.unwrap_or(2);
    // Box::leak gives a `'static` reference; the config lives for the entire process lifetime.
    let governor_conf: &'static _ = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst_size)
            .finish()
            .expect("Failed to build auth rate-limit configuration"),
    ));

    // Periodically evict expired entries to bound memory usage.
    let limiter = governor_conf.limiter().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        limiter.retain_recent();
    });

    let auth_router = api::auth::router()
        .layer(GovernorLayer { config: governor_conf });

    let api_v1 = Router::new()
        .merge(api::health::router())
        .merge(auth_router)
        .merge(api::biblios::router())
        .merge(api::users::router())
        .merge(api::loans::router())
        .merge(api::batch::router())
        .merge(api::reservations::router())
        .merge(api::fines::router())
        .merge(api::inventory::router())
        .merge(api::history::router())
        .merge(api::opac::router())
        .merge(api::covers::router())
        .merge(api::sse::router())
        .merge(api::z3950::router())
        .merge(api::stats::router())
        .merge(api::library_info::router())
        .merge(api::settings::router())
        .merge(api::admin_config::router())
        .merge(api::audit::router())
        .merge(api::public_types::router())
        .merge(api::visitor_counts::router())
        .merge(api::schedules::router())
        .merge(api::series::router())
        .merge(api::collections::router())
        .merge(api::sources::router())
        .merge(api::equipment::router())
        .merge(api::events::router())
        .merge(api::maintenance::router())
        .merge(api::tasks::router())
        .with_state(state.clone());

    // OpenAPI documentation
    let openapi = api::openapi::create_openapi_router();

    Router::new()
        .route("/version", get(api::health::version))
        .nest("/api/v1", api_v1)
        .merge(openapi)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

/// Build the CORS layer from configuration.
///
/// In production (`server.cors_origins` is set), only the listed origins are allowed.
/// When the list is empty or the field is absent, CORS falls back to `Any` (dev mode).
fn build_cors(config: &elidune_server::config::AppConfig) -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};
    use axum::http::HeaderValue;

    if let Some(ref origins) = config.server.cors_origins {
        if !origins.is_empty() {
            let parsed: Vec<HeaderValue> = origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            if !parsed.is_empty() {
                return CorsLayer::new()
                    .allow_origin(parsed)
                    .allow_methods(Any)
                    .allow_headers(Any);
            }
        }
    }

    // Permissive default for development / unconfigured deployments.
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
