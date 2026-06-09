//! Prometheus metrics endpoint.

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

use crate::{error::AppResult, services::operational_metrics};

/// Build the metrics routes for this domain.
pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new().route("/metrics", get(metrics))
}

/// Prometheus scrape endpoint (unauthenticated to support cluster scraping).
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "health",
    responses(
        (status = 200, description = "Prometheus metrics payload")
    )
)]
pub async fn metrics(State(state): State<crate::AppState>) -> AppResult<Response> {
    let payload = operational_metrics::gather_handler(state.services.repository.as_ref()).await?;
    let mut response = (StatusCode::OK, payload).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    Ok(response)
}
