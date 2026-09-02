//! The `Auth` extractor: who is calling, by socket, token, or cookie.

use axum::{
    extract::FromRequestParts,
    http::{Method, header, request::Parts},
};
use mandrake_core::{Actor, Id, Role, Timestamp, Via};
use rusqlite::{Connection, OptionalExtension, params};

use super::{session, token};
use crate::{app::AppState, error::ApiError};

/// Header that cookie-authenticated mutating requests must carry.
pub const CSRF_HEADER: &str = "x-mandrake-request";

/// Request extension: uid of the peer on the Unix socket, if known.
#[derive(Debug, Clone, Copy)]
pub struct SocketPeer(pub Option<u32>);

/// Request extension: where the request came from, for audit and limits.
#[derive(Debug, Clone)]
pub struct Source(pub String);

/// The authenticated caller.
#[derive(Debug, Clone)]
pub struct Auth {
    /// Who.
    pub actor: Actor,
    /// Session hash when authenticated by cookie, for logout.
    pub session_hash: Option<String>,
    /// Session expiry, when a session.
    pub expires_at: Option<Timestamp>,
    /// Session idle expiry, when a session.
    pub idle_expires_at: Option<Timestamp>,
}

impl Auth {
    /// Refuse unless the actor holds at least `role`.
    pub fn require(&self, role: Role) -> Result<(), ApiError> {
        if self.actor.role.allows(role) {
            Ok(())
        } else {
            Err(ApiError::forbidden(&format!("requires role {role}")))
        }
    }

    /// Refuse unless the actor is `user` or holds at least `role`.
    pub fn require_self_or(&self, user: Id, role: Role) -> Result<(), ApiError> {
        if self.actor.is_user(user) {
            Ok(())
        } else {
            self.require(role)
        }
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(peer) = parts.extensions.get::<SocketPeer>() {
            return match peer.0 {
                Some(0) => Ok(Self {
                    actor: Actor::root(),
                    session_hash: None,
                    expires_at: None,
                    idle_expires_at: None,
                }),
                _ => Err(ApiError::forbidden("only root may use the socket")),
            };
        }

        if let Some(bearer) = bearer(parts) {
            if !token::looks_like_token(&bearer) {
                return Err(ApiError::unauthorized());
            }
            let hash = token::hash(&bearer);
            let found = state.db.call(move |conn| lookup_token(conn, &hash)).await?;
            return found.ok_or_else(ApiError::unauthorized);
        }

        if let Some(secret) = cookie(parts) {
            let found = state
                .db
                .call(move |conn| lookup_session(conn, &secret))
                .await?;
            let auth = found.ok_or_else(ApiError::unauthorized)?;
            if mutating(&parts.method) && !has_csrf_header(parts) {
                return Err(ApiError::typed(
                    axum::http::StatusCode::FORBIDDEN,
                    "csrf",
                    "Forbidden",
                )
                .detail("mutating requests from a session must send X-Mandrake-Request: 1"));
            }
            return Ok(auth);
        }

        Err(ApiError::unauthorized())
    }
}

fn bearer(parts: &Parts) -> Option<String> {
    let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let rest = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    Some(rest.trim().to_owned())
}

fn cookie(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(session::from_cookie_header)
        .map(str::to_owned)
}

fn mutating(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn has_csrf_header(parts: &Parts) -> bool {
    parts
        .headers
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim() == "1")
}

struct UserBits {
    id: Id,
    username: String,
    role: Role,
    disabled: bool,
}

fn user_bits(conn: &Connection, user_id: &str) -> rusqlite::Result<Option<UserBits>> {
    conn.query_row(
        "SELECT id, username, role, disabled FROM users WHERE id = ?1",
        [user_id],
        |r| {
            let role: String = r.get("role")?;
            Ok(UserBits {
                id: crate::db::get_id(r, "id")?,
                username: r.get("username")?,
                role: role.parse().unwrap_or(Role::Viewer),
                disabled: r.get::<_, i64>("disabled")? != 0,
            })
        },
    )
    .optional()
}

fn lookup_token(conn: &Connection, hash: &str) -> rusqlite::Result<Option<Auth>> {
    let row = conn
        .query_row(
            "SELECT id, user_id, expires_at, last_used_at FROM tokens WHERE hash = ?1",
            [hash],
            |r| {
                Ok((
                    crate::db::get_id(r, "id")?,
                    r.get::<_, String>("user_id")?,
                    crate::db::get_opt_ts(r, "expires_at")?,
                    crate::db::get_opt_ts(r, "last_used_at")?,
                ))
            },
        )
        .optional()?;
    let Some((token_id, user_id, expires_at, last_used_at)) = row else {
        return Ok(None);
    };
    let now = Timestamp::now();
    if expires_at.is_some_and(|t| now >= t) {
        return Ok(None);
    }
    let Some(user) = user_bits(conn, &user_id)? else {
        return Ok(None);
    };
    if user.disabled {
        return Ok(None);
    }
    if last_used_at.is_none_or(|t| now.seconds_since(t) >= 60) {
        conn.execute(
            "UPDATE tokens SET last_used_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), token_id.to_string()],
        )?;
    }
    Ok(Some(Auth {
        actor: Actor {
            id: Some(user.id),
            username: user.username,
            role: user.role,
            via: Via::Token,
            token_id: Some(token_id),
        },
        session_hash: None,
        expires_at: None,
        idle_expires_at: None,
    }))
}

fn lookup_session(conn: &Connection, secret: &str) -> rusqlite::Result<Option<Auth>> {
    let Some(row) = session::touch(conn, secret)? else {
        return Ok(None);
    };
    let Some(user) = user_bits(conn, &row.user_id.to_string())? else {
        return Ok(None);
    };
    if user.disabled {
        session::delete(conn, &row.hash)?;
        return Ok(None);
    }
    Ok(Some(Auth {
        actor: Actor {
            id: Some(user.id),
            username: user.username,
            role: user.role,
            via: Via::Session,
            token_id: None,
        },
        session_hash: Some(row.hash),
        expires_at: Some(row.expires_at),
        idle_expires_at: Some(row.idle_expires_at),
    }))
}
