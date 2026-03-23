//! Cover image management — proxy from Open Library
//!
//! Proxies cover images from Open Library (https://covers.openlibrary.org)
//! so the frontend never needs to call external services directly.
//! Supports S (small), M (medium), L (large) sizes.

use axum::{
    extract::{Path, Query},
    http::{header, StatusCode},
    response::Response,
};
use serde::Deserialize;

/// Cover image size
#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
pub enum CoverSize {
    S,
    M,
    L,
}

impl CoverSize {
    fn as_str(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::M => "M",
            Self::L => "L",
        }
    }
}

impl Default for CoverSize {
    fn default() -> Self {
        Self::M
    }
}

/// Cover image query params
#[derive(Deserialize)]
pub struct CoverQuery {
    #[serde(default)]
    pub size: CoverSize,
}

/// Proxy cover image from Open Library by ISBN
///
/// Fetches the cover from Open Library and streams it back.
/// Falls back to a 404 if no cover is available.
#[utoipa::path(
    get,
    path = "/covers/isbn/{isbn}",
    tag = "covers",
    params(
        ("isbn" = String, Path, description = "ISBN-10 or ISBN-13"),
        ("size" = Option<String>, Query, description = "Cover size: S, M (default), or L")
    ),
    responses(
        (status = 200, description = "Cover image (JPEG)", content_type = "image/jpeg"),
        (status = 404, description = "No cover available for this ISBN")
    )
)]
pub async fn get_cover_by_isbn(
    Path(isbn): Path<String>,
    Query(query): Query<CoverQuery>,
) -> Result<Response, StatusCode> {
    proxy_cover(&format!(
        "https://covers.openlibrary.org/b/isbn/{}-{}.jpg",
        isbn,
        query.size.as_str()
    ))
    .await
}

async fn proxy_cover(url: &str) -> Result<Response, StatusCode> {
    let response = reqwest::get(url)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    if !response.status().is_success() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    let bytes = response.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    // Open Library returns a 1x1 GIF when no cover is available — treat as 404
    if bytes.len() < 100 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

pub fn router() -> axum::Router<crate::AppState> {
    use axum::routing::get;
    axum::Router::new().route("/covers/isbn/:isbn", get(get_cover_by_isbn))
}
