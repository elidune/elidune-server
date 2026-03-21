//! Elidune Server - Library Management System
//!
//! A modern Rust REST API server for library management.

use axum::{
    routing::{get, post, put, delete},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use std::path::Path;
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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    tracing::info!("Database migrations completed");

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

    // Create application state
    let state = AppState {
        config: Arc::new(config),
        dynamic_config,
        services: services.clone(),
        scheduler_notify,
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
    .await?;

    Ok(())
}

/// Create the application router with all routes
fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_v1 = Router::new()
        // Health check
        .route("/health", get(api::health::health_check))
        .route("/ready", get(api::health::readiness_check))
        // Authentication
        .route("/auth/login", post(api::auth::login))
        .route("/auth/me", get(api::auth::me))
        .route("/auth/profile", put(api::users::update_my_profile))
        .route("/auth/verify-2fa", post(api::auth::verify_2fa))
        .route("/auth/verify-recovery", post(api::auth::verify_recovery))
        .route("/auth/request-password-reset", post(api::auth::request_password_reset))
        .route("/auth/reset-password", post(api::auth::reset_password))
        .route("/auth/setup-2fa", post(api::auth::setup_2fa))
        .route("/auth/disable-2fa", post(api::auth::disable_2fa))
        // Items (catalog)
        .route("/items", get(api::items::list_items))
        .route("/items", post(api::items::create_item))
        .route("/items/load-marc", post(api::items::upload_unimarc))
        .route("/items/import-marc", post(api::items::import_marc_batch))
        .route("/items/:id", get(api::items::get_item))
        .route("/items/:id", put(api::items::update_item))
        .route("/items/:id", delete(api::items::delete_item))
        // Specimens
        .route("/items/:item_id/specimens", get(api::items::list_specimens))
        .route("/items/:item_id/specimens", post(api::items::create_specimen))
        .route("/items/:item_id/specimens", put(api::items::update_specimen))
        .route("/items/:item_id/specimens/:specimen_id", delete(api::items::delete_specimen))
        // Users
        .route("/users", get(api::users::list_users))
        .route("/users", post(api::users::create_user))
        .route("/users/:id", get(api::users::get_user))
        .route("/users/:id", put(api::users::update_user))
        .route("/users/:id", delete(api::users::delete_user))
        .route("/users/:id/account-type", put(api::users::update_account_type))
        .route("/users/:id/loans", get(api::loans::get_user_loans))
        // Loans
        .route("/loans", post(api::loans::create_loan))
        .route("/loans/overdue", get(api::loans::get_overdue_loans))
        .route("/loans/send-overdue-reminders", post(api::loans::send_overdue_reminders))
        .route("/loans/:id/return", post(api::loans::return_loan))
        .route("/loans/:id/renew", post(api::loans::renew_loan))
        .route("/loans/specimens/:specimen_id/return", post(api::loans::return_loan_by_specimen))
        .route("/loans/specimens/:specimen_id/renew", post(api::loans::renew_loan_by_specimen))
        // Z39.50
        .route("/z3950/search", get(api::z3950::search))
        .route("/z3950/import", post(api::z3950::import_record))
        // Statistics
        .route("/stats", get(api::stats::get_stats))
        .route("/stats/loans", get(api::stats::get_loan_stats))
        .route("/stats/users", get(api::stats::get_user_stats))
        .route("/stats/catalog", get(api::stats::get_catalog_stats))
        // Library information
        .route("/library-info", get(api::library_info::get_library_info))
        .route("/library-info", put(api::library_info::update_library_info))
        // Settings (loan rules)
        .route("/settings", get(api::settings::get_settings))
        .route("/settings", put(api::settings::update_settings))
        // Admin config (dynamic config override)
        .route("/admin/config", get(api::admin_config::get_config))
        .route("/admin/config/:section", put(api::admin_config::update_config_section))
        .route("/admin/config/:section", delete(api::admin_config::reset_config_section))
        .route("/admin/config/email/test", post(api::admin_config::test_email))
        .route("/admin/reindex-search", post(api::admin_config::reindex_search))
        // Audit log
        .route("/audit", get(api::audit::get_audit_log))
        .route("/audit/export", get(api::audit::export_audit_log))
        // Public types
        .route("/public-types", get(api::public_types::list_public_types))
        .route("/public-types", post(api::public_types::create_public_type))
        .route("/public-types/:id", get(api::public_types::get_public_type))
        .route("/public-types/:id", put(api::public_types::update_public_type))
        .route("/public-types/:id", delete(api::public_types::delete_public_type))
        .route("/public-types/:id/loan-settings", put(api::public_types::upsert_loan_setting))
        .route("/public-types/:id/loan-settings/:media_type", delete(api::public_types::delete_loan_setting))
        // Visitor counts
        .route("/visitor-counts", get(api::visitor_counts::list_visitor_counts))
        .route("/visitor-counts", post(api::visitor_counts::create_visitor_count))
        .route("/visitor-counts/:id", delete(api::visitor_counts::delete_visitor_count))
        // Schedules
        .route("/schedules/periods", get(api::schedules::list_periods))
        .route("/schedules/periods", post(api::schedules::create_period))
        .route("/schedules/periods/:id", put(api::schedules::update_period))
        .route("/schedules/periods/:id", delete(api::schedules::delete_period))
        .route("/schedules/periods/:id/slots", get(api::schedules::list_slots))
        .route("/schedules/periods/:id/slots", post(api::schedules::create_slot))
        .route("/schedules/slots/:id", delete(api::schedules::delete_slot))
        .route("/schedules/closures", get(api::schedules::list_closures))
        .route("/schedules/closures", post(api::schedules::create_closure))
        .route("/schedules/closures/:id", delete(api::schedules::delete_closure))
        // Sources
        .route("/sources", get(api::sources::list_sources).post(api::sources::create_source))
        .route("/sources/merge", post(api::sources::merge_sources))
        .route("/sources/:id", get(api::sources::get_source))
        .route("/sources/:id", put(api::sources::update_source))
        .route("/sources/:id/archive", post(api::sources::archive_source))
        // Equipment
        .route("/equipment", get(api::equipment::list_equipment))
        .route("/equipment", post(api::equipment::create_equipment))
        .route("/equipment/:id", get(api::equipment::get_equipment))
        .route("/equipment/:id", put(api::equipment::update_equipment))
        .route("/equipment/:id", delete(api::equipment::delete_equipment))
        // Events (cultural)
        .route("/events", get(api::events::list_events))
        .route("/events", post(api::events::create_event))
        .route("/events/:id", get(api::events::get_event))
        .route("/events/:id", put(api::events::update_event))
        .route("/events/:id", delete(api::events::delete_event))
        .route("/events/:id/send-announcement", post(api::events::send_event_announcement))
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
