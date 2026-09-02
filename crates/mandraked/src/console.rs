//! The embedded web console (spec §6.1).
//!
//! `console/dist` is embedded at build time in release builds and read from
//! disk in debug builds. Unknown paths fall back to `index.html` so the
//! single-page app owns routing; anything under `/api/` is a real 404.

use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

use crate::error::ApiError;

// Relative to this crate's Cargo.toml; build.rs makes sure it exists.
#[derive(Embed)]
#[folder = "../../console/dist"]
struct Assets;

/// Fallback handler serving console assets.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return ApiError::not_found("route").into_response();
    }
    let direct = if path.is_empty() {
        None
    } else {
        Assets::get(path)
    };
    let (file, is_index) = match direct {
        Some(f) => (Some(f), false),
        None => (Assets::get("index.html"), true),
    };
    let Some(file) = file else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "mandraked: console assets are not built; the API is available under /api/v1\n",
        )
            .into_response();
    };
    let cache = if !is_index && path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mime = file.metadata.mimetype().to_owned();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache)
        .header("x-content-type-options", "nosniff")
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
