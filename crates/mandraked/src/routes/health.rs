//! `GET /health`.

use axum::http::StatusCode;

/// Liveness: 204 when serving.
pub async fn get() -> StatusCode {
    StatusCode::NO_CONTENT
}
