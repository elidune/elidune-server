//! Health check endpoints

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthDatabaseStatus {
    pub connected: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthSetupStatus {
    /// True when there are no users and no `settings` rows (initial wizard required).
    pub need_first_setup: bool,
    pub users_empty: bool,
    pub settings_empty: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// `healthy`, `need_first_setup`, or `degraded` (database unreachable).
    pub status: String,
    /// Server version from Cargo.toml
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<HealthDatabaseStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<HealthSetupStatus>,
}

async fn database_connected(pool: &sqlx::PgPool) -> bool {
    // Use `execute`: `SELECT 1` is INT4 in PostgreSQL; `i8`/`query_scalar` types must match exactly.
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .is_ok()
}

async fn load_setup_status(state: &crate::AppState) -> Option<HealthSetupStatus> {
    let repo = state.services.minimal_repository();
    let users_empty = repo.users_count().await.ok()? == 0;
    let settings_empty = repo.settings_count().await.ok()? == 0;
    let need_first_setup = users_empty && settings_empty;
    Some(HealthSetupStatus {
        need_first_setup,
        users_empty,
        settings_empty,
    })
}

fn build_health_response(
    status: &str,
    db: Option<HealthDatabaseStatus>,
    setup: Option<HealthSetupStatus>,
) -> HealthResponse {
    HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: db,
        setup,
    }
}

/// Health check — process is up; includes DB/setup snapshot when the database is reachable.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Liveness / service status", body = HealthResponse)
    )
)]
pub async fn health_check(State(state): State<crate::AppState>) -> Json<HealthResponse> {
    let pool = state.services.repository_pool();
    let connected = database_connected(pool).await;
    println!("connected: {}", connected);
    if !connected {
        return Json(build_health_response(
            "degraded",
            Some(HealthDatabaseStatus { connected: false }),
            None,
        ));
    }

    let setup = load_setup_status(&state).await;
    let status = setup
        .as_ref()
        .map(|s| {
            if s.need_first_setup {
                "need_first_setup"
            } else {
                "healthy"
            }
        })
        .unwrap_or("healthy");

    Json(build_health_response(
        status,
        Some(HealthDatabaseStatus { connected: true }),
        setup,
    ))
}

/// Readiness — database must be reachable; HTTP 503 when not.
#[utoipa::path(
    get,
    path = "/ready",
    tag = "health",
    responses(
        (status = 200, description = "Service is ready", body = HealthResponse),
        (status = 503, description = "Database not reachable", body = HealthResponse)
    )
)]
pub async fn readiness_check(State(state): State<crate::AppState>) -> impl IntoResponse {
    let pool = state.services.repository_pool();
    let connected = database_connected(pool).await;
    if !connected {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(build_health_response(
                "not_ready",
                Some(HealthDatabaseStatus { connected: false }),
                None,
            )),
        )
            .into_response();
    }

    let setup = load_setup_status(&state).await;
    let status = setup
        .as_ref()
        .map(|s| {
            if s.need_first_setup {
                "need_first_setup"
            } else {
                "ready"
            }
        })
        .unwrap_or("ready");

    (
        StatusCode::OK,
        Json(build_health_response(
            status,
            Some(HealthDatabaseStatus { connected: true }),
            setup,
        )),
    )
        .into_response()
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionResponse {
    /// Server version (from Cargo.toml)
    pub version: String,
}

/// Server version endpoint
#[utoipa::path(
    get,
    path = "/version",
    tag = "health",
    responses(
        (status = 200, description = "Server version", body = VersionResponse)
    )
)]
pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Build the health routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
}
