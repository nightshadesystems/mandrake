//! API errors as RFC 7807 problem responses.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use mandrake_core::Problem;

/// An error that renders as `application/problem+json`. Boxed so `Result`
/// stays small on the happy path.
#[derive(Debug, Clone)]
pub struct ApiError(pub Box<Problem>);

/// Result alias for handlers.
pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    /// Plain HTTP problem.
    pub fn new(status: StatusCode, title: &str) -> Self {
        Self(Box::new(Problem::new(status.as_u16(), title)))
    }

    /// Application problem with a slug under the Mandrake problem base.
    pub fn typed(status: StatusCode, slug: &str, title: &str) -> Self {
        Self(Box::new(Problem::typed(status.as_u16(), slug, title)))
    }

    /// Attach a detail message.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.0.detail = Some(detail.into());
        self
    }

    /// 401.
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Unauthorized")
    }

    /// 403 with a reason.
    pub fn forbidden(detail: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, "Forbidden").detail(detail)
    }

    /// 404 for a named thing.
    pub fn not_found(what: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not Found").detail(format!("{what} not found"))
    }

    /// 409.
    pub fn conflict(detail: &str) -> Self {
        Self::new(StatusCode::CONFLICT, "Conflict").detail(detail)
    }

    /// 422.
    pub fn unprocessable(detail: &str) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "Unprocessable Entity").detail(detail)
    }

    /// 500. The cause is logged, never sent to the client.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!(%cause, "internal error");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        StatusCode::from_u16(self.0.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl From<Problem> for ApiError {
    fn from(p: Problem) -> Self {
        Self(Box::new(p))
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(format!("sqlite: {e}"))
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::internal(format!("task: {e}"))
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::internal(format!("json: {e}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut response = (status, Json(*self.0)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ApiError {}
