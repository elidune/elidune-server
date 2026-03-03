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
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use elidune_server::{
    api,
    config::AppConfig,
    repository::Repository,
    services::Services,
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
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Load configuration

    let config = config_path_from_args().map(|path| AppConfig::load(&path)).ok_or_else(|| anyhow::anyhow!("No configuration path provided"))??;
    
    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("elidune_server={},tower_http=debug,z3950_rs=debug", config.logging.level).into());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
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
        config.email.clone(),
        config.redis.clone(),
        redis_service,
    )
    .await
    .expect("Failed to create services");

    // Create application state
    let state = AppState {
        config: Arc::new(config),
        services: Arc::new(services),
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
    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the application router with all routes
fn create_router(state: AppState) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // API v1 routes
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
        .route("/items/:id/specimens", get(api::items::list_specimens))
        .route("/items/:id/specimens", post(api::items::create_specimen))
        .route("/items/:item_id/specimens/:specimen_id", put(api::items::update_specimen))
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
        // Settings
        .route("/settings", get(api::settings::get_settings))
        .route("/settings", put(api::settings::update_settings))
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
        // Events
        .route("/events", get(api::events::list_events))
        .route("/events", post(api::events::create_event))
        .route("/events/:id", get(api::events::get_event))
        .route("/events/:id", put(api::events::update_event))
        .route("/events/:id", delete(api::events::delete_event))
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
