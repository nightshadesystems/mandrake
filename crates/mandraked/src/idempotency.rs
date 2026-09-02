//! `Idempotency-Key` handling for POST (ADR-0007).
//!
//! Middleware on the API router. A repeat with the same key and body from
//! the same actor gets the stored response; a different body gets 422.
//! Two identical requests racing each other both execute; that window is
//! accepted for v1.

use axum::{
    body::{Body, Bytes, to_bytes},
    extract::{FromRequestParts, Request, State},
    http::{Method, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use mandrake_core::Timestamp;
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::{
    app::AppState,
    auth::{Auth, token},
    error::ApiError,
};

/// Header name.
pub const HEADER: &str = "idempotency-key";
/// Largest request or response body that is stored.
const BODY_LIMIT: usize = 1 << 20;

struct Stored {
    body_hash: String,
    status: u16,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn key_of(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|k| !k.is_empty() && k.len() <= 128)
        .map(str::to_owned)
}

async fn lookup(
    state: &AppState,
    actor_key: String,
    key: String,
) -> Result<Option<Stored>, ApiError> {
    state
        .db
        .call(move |conn| {
            conn.query_row(
                "SELECT body_hash, status, content_type, body FROM idempotency \
                 WHERE actor_key = ?1 AND key = ?2",
                [actor_key, key],
                |r| {
                    Ok(Stored {
                        body_hash: r.get("body_hash")?,
                        status: r
                            .get::<_, i64>("status")
                            .map(|s| u16::try_from(s).unwrap_or(500))?,
                        content_type: r.get("content_type")?,
                        body: r.get("body")?,
                    })
                },
            )
            .optional()
        })
        .await
}

async fn store(
    state: &AppState,
    actor_key: String,
    key: String,
    body_hash: String,
    response: Response,
) -> Response {
    // Server errors may be retried, so they are never stored.
    if response.status().is_server_error() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, BODY_LIMIT).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let status = i64::from(parts.status.as_u16());
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let stored_body = bytes.to_vec();
    let _ = state
        .db
        .call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO idempotency \
                 (actor_key, key, body_hash, status, content_type, body, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    actor_key,
                    key,
                    body_hash,
                    status,
                    content_type,
                    stored_body,
                    Timestamp::now().to_rfc3339()
                ],
            )
        })
        .await;
    Response::from_parts(parts, Body::from(bytes))
}

fn replay(s: Stored) -> Response {
    let mut response = Response::new(Body::from(s.body));
    *response.status_mut() = StatusCode::from_u16(s.status).unwrap_or(StatusCode::OK);
    if let Some(ct) = s
        .content_type
        .as_deref()
        .and_then(|c| header::HeaderValue::from_str(c).ok())
    {
        response.headers_mut().insert(header::CONTENT_TYPE, ct);
    }
    response.headers_mut().insert(
        "idempotent-replayed",
        header::HeaderValue::from_static("true"),
    );
    response
}

/// The middleware.
pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let (mut parts, body) = req.into_parts();
    let Some(key) = key_of(&parts) else {
        return next.run(Request::from_parts(parts, body)).await;
    };
    let auth = match Auth::from_request_parts(&mut parts, &state).await {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    let actor_key = auth
        .actor
        .id
        .map_or_else(|| "root".to_owned(), |id| id.to_string());
    let Ok(bytes) = to_bytes(body, BODY_LIMIT).await else {
        return ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "Payload Too Large").into_response();
    };
    let body_hash = token::hex(&Sha256::digest(&bytes));

    match lookup(&state, actor_key.clone(), key.clone()).await {
        Err(e) => return e.into_response(),
        Ok(Some(s)) if s.body_hash != body_hash => {
            return ApiError::typed(
                StatusCode::UNPROCESSABLE_ENTITY,
                "idempotency-mismatch",
                "Unprocessable Entity",
            )
            .detail("Idempotency-Key was already used with a different request body")
            .into_response();
        }
        Ok(Some(s)) => return replay(s),
        Ok(None) => {}
    }

    let req = Request::from_parts(parts, Body::from(Bytes::clone(&bytes)));
    let response = next.run(req).await;
    store(&state, actor_key, key, body_hash, response).await
}
