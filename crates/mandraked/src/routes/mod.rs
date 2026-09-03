//! HTTP handlers, one module per resource family.

pub mod audit;
pub mod auth;
pub mod events;
pub mod health;
pub mod images;
pub mod jobs;
pub mod network;
pub mod storage;
pub mod system;
pub mod tokens;
pub mod users;

use std::future::Future;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{app::AppState, audit::Context, auth::Source};

/// Extractor for the per-request audit context.
pub struct Ctx(pub Context);

impl FromRequestParts<AppState> for Ctx {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &AppState,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let source = parts.extensions.get::<Source>().map(|s| s.0.clone());
        let request_id = parts
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        std::future::ready(Ok(Self(Context { source, request_id })))
    }
}
