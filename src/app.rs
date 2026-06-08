//! HTTP application router assembly (shared by the binary and integration tests).

use std::time::Duration;

use axum::{routing::get, Router};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::trace::TraceLayer;

use crate::{api, config::AppConfig, AppState};

/// Options for building the HTTP router (production defaults vs permissive test limits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppBuildOptions {
    /// Override auth rate limit (requests per second per IP). `None` uses `AppConfig`.
    pub auth_rate_per_second: Option<u64>,
    /// Override auth burst size. `None` uses `AppConfig`.
    pub auth_rate_burst: Option<u32>,
    /// Override public API rate limit. `None` uses `AppConfig`.
    pub public_rate_per_second: Option<u64>,
    /// Override public burst size. `None` uses `AppConfig`.
    pub public_rate_burst: Option<u32>,
    /// Spawn background thread to evict expired governor entries (disable in tests).
    pub spawn_rate_limit_cleanup: bool,
    /// Include OpenAPI / Swagger UI routes.
    pub include_openapi: bool,
    /// Include TraceLayer HTTP logging.
    pub include_trace_layer: bool,
}

impl Default for AppBuildOptions {
    fn default() -> Self {
        Self {
            auth_rate_per_second: None,
            auth_rate_burst: None,
            public_rate_per_second: None,
            public_rate_burst: None,
            spawn_rate_limit_cleanup: true,
            include_openapi: true,
            include_trace_layer: true,
        }
    }
}

impl AppBuildOptions {
    /// High burst limits and no background cleanup — suitable for in-process integration tests.
    pub fn for_tests() -> Self {
        Self {
            auth_rate_per_second: Some(10_000),
            auth_rate_burst: Some(10_000),
            public_rate_per_second: Some(10_000),
            public_rate_burst: Some(10_000),
            spawn_rate_limit_cleanup: false,
            include_openapi: false,
            include_trace_layer: false,
        }
    }
}

/// Build the full Axum router with all API routes and middleware.
pub fn build_app(state: AppState) -> Router {
    build_app_with_options(state, AppBuildOptions::default())
}

/// Build the Axum router with explicit build options (used by integration tests).
pub fn build_app_with_options(state: AppState, options: AppBuildOptions) -> Router {
    let cors = build_cors(&state.config);

    let per_second = options
        .auth_rate_per_second
        .or(state.config.server.auth_rate_per_second)
        .unwrap_or(4);
    let burst_size = options
        .auth_rate_burst
        .or(state.config.server.auth_rate_burst)
        .unwrap_or(2);

    let governor_conf: &'static _ = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst_size)
            .finish()
            .expect("Failed to build auth rate-limit configuration"),
    ));

    let public_per_second = options
        .public_rate_per_second
        .or(state.config.server.public_rate_per_second)
        .unwrap_or(30);
    let public_burst = options
        .public_rate_burst
        .or(state.config.server.public_rate_burst)
        .unwrap_or(100);

    let public_governor_conf: &'static _ = Box::leak(Box::new(
        GovernorConfigBuilder::default()
            .per_second(public_per_second)
            .burst_size(public_burst)
            .finish()
            .expect("Failed to build public rate-limit configuration"),
    ));

    if options.spawn_rate_limit_cleanup {
        let auth_limiter = governor_conf.limiter().clone();
        let public_limiter = public_governor_conf.limiter().clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(60));
            auth_limiter.retain_recent();
            public_limiter.retain_recent();
        });
    }

    let auth_router = api::auth::router().layer(GovernorLayer {
        config: governor_conf,
    });

    let public_router = Router::new()
        .merge(api::opac::router())
        .merge(api::covers::router())
        .merge(api::library_info::router_public())
        .layer(GovernorLayer {
            config: public_governor_conf,
        });

    let first_setup_router = api::first_setup::router().layer(GovernorLayer {
        config: governor_conf,
    });

    let api_v1 = Router::new()
        .merge(api::health::router())
        .merge(first_setup_router)
        .merge(auth_router)
        .merge(public_router)
        .merge(api::biblios::router())
        .merge(api::items::router())
        .merge(api::users::router())
        .merge(api::loans::router())
        .merge(api::batch::router())
        .merge(api::holds::router())
        .merge(api::fines::router())
        .merge(api::inventory::router())
        .merge(api::sse::router())
        .merge(api::z3950::router())
        .merge(api::stats::router())
        .merge(api::library_info::router_staff())
        .merge(api::email_templates::router())
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
        .merge(api::account_types::router())
        .merge(api::maintenance::router())
        .merge(api::tasks::router())
        .with_state(state.clone());

    let mut app = Router::new()
        .route("/version", get(api::health::version))
        .nest("/api/v1", api_v1);

    if options.include_openapi {
        app = app.merge(api::openapi::create_openapi_router());
    }

    if options.include_trace_layer {
        app = app.layer(TraceLayer::new_for_http());
    }

    app.layer(cors)
}

/// Build the CORS layer from configuration.
///
/// When `server.cors_origins` is set, only listed origins are allowed.
/// When empty or absent, CORS falls back to `Any` (development mode).
pub fn build_cors(config: &AppConfig) -> tower_http::cors::CorsLayer {
    use axum::http::HeaderValue;
    use tower_http::cors::{Any, CorsLayer};

    if let Some(ref origins) = config.server.cors_origins {
        if !origins.is_empty() {
            let parsed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
            if !parsed.is_empty() {
                return CorsLayer::new()
                    .allow_origin(parsed)
                    .allow_methods(Any)
                    .allow_headers(Any);
            }
        }
    }

    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
