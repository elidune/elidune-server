//! Shared harness for in-process HTTP integration tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Notify;
use tower::ServiceExt;

use elidune_server::{
    build_app_with_options, repository::Repository, services::Services, AppBuildOptions,
    AppConfig, AppState, DynamicConfig, EmailService,
};

pub mod fixtures;

/// In-process test application with database, Redis, and HTTP router.
pub struct TestApp {
    pub router: Router,
    pub state: AppState,
}



impl TestApp {
    /// Build a test app. Skips the test when `DATABASE_URL` is unset and `CI` is not set.
    pub async fn spawn() -> Option<Self> {
        if std::env::var("DATABASE_URL").is_err() && std::env::var("CI").is_err() {
            eprintln!("Skipping integration test: DATABASE_URL not set");
            return None;
        }

        let config = AppConfig::for_test();

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database.url)
            .await
            .expect("connect test database");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        let dynamic_config = DynamicConfig::new(config.clone());

        let redis_service = elidune_server::services::redis::RedisService::new(&config.redis.url)
            .await
            .expect("connect redis");

        let email_service = Arc::new(EmailService::new(dynamic_config.clone(), pool.clone()));

        let repository = Repository::new(pool, Some(dynamic_config.clone()));

        let (event_bus_tx, _) = tokio::sync::broadcast::channel(256);
        let event_bus = elidune_server::services::event_bus::EventBus::new(event_bus_tx.clone());

        let services = Arc::new(
            Services::new(
                repository,
                config.users.clone(),
                dynamic_config.clone(),
                config.redis.clone(),
                redis_service,
                None,
                email_service,
                event_bus,
            )
            .await
            .expect("create services"),
        );

        let state = AppState {
            config: Arc::new(config),
            dynamic_config,
            services,
            scheduler_notify: Arc::new(Notify::new()),
            event_bus: event_bus_tx,
        };

        let router = build_app_with_options(state.clone(), AppBuildOptions::for_tests());

        Some(Self { router, state })
    }

    /// Send an HTTP request and return the full response.
    pub async fn request(&self, req: Request<Body>) -> Response<Body> {
        self.router
            .clone()
            .oneshot(req)
            .await
            .expect("router oneshot")
    }

    /// GET helper returning status and JSON body.
    pub async fn get_json(&self, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = self
            .request(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, json)
    }

    /// POST JSON helper.
    pub async fn post_json(
        &self,
        uri: &str,
        body: &serde_json::Value,
        auth_token: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method("POST").uri(uri);
        if let Some(token) = auth_token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        let response = self
            .request(
                builder
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await;
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, json)
    }

    /// GET helper with Bearer auth.
    pub async fn get_json_with_auth(
        &self,
        uri: &str,
        token: &str,
    ) -> (StatusCode, serde_json::Value) {
        let response = self
            .request(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, json)
    }

    /// POST with empty body.
    pub async fn post_empty(
        &self,
        uri: &str,
        auth_token: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method("POST").uri(uri);
        if let Some(token) = auth_token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        let response = self
            .request(builder.body(Body::empty()).unwrap())
            .await;
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, json)
    }
}
