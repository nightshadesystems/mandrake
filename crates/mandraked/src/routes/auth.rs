//! `/auth/login`, `/auth/logout`, `/auth/session`.

use axum::{
    Json,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use mandrake_core::{
    Actor, Id, Role, Timestamp, Via,
    api::{LoginRequest, ObjectRef, Session},
};
use rusqlite::{Connection, OptionalExtension, params};

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::{Auth, password, session},
    error::{ApiError, ApiResult},
};

/// Failures within the window before a lock.
const LOCK_AFTER: i64 = 5;
/// Window and lock length in seconds.
const LOCK_SECS: i64 = 15 * 60;

struct Candidate {
    id: Id,
    username: String,
    role: Role,
    hash: String,
    disabled: bool,
    failed: i64,
    first_failed_at: Option<Timestamp>,
    locked_until: Option<Timestamp>,
}

fn load(conn: &Connection, username: &str) -> rusqlite::Result<Option<Candidate>> {
    conn.query_row(
        "SELECT id, username, role, password_hash, disabled, failed_logins, first_failed_at, locked_until \
         FROM users WHERE username = ?1",
        [username],
        |r| {
            let role: String = r.get("role")?;
            Ok(Candidate {
                id: crate::db::get_id(r, "id")?,
                username: r.get("username")?,
                role: role.parse().unwrap_or(Role::Viewer),
                hash: r.get("password_hash")?,
                disabled: r.get::<_, i64>("disabled")? != 0,
                failed: r.get("failed_logins")?,
                first_failed_at: crate::db::get_opt_ts(r, "first_failed_at")?,
                locked_until: crate::db::get_opt_ts(r, "locked_until")?,
            })
        },
    )
    .optional()
}

enum Outcome {
    Ok { cookie: String, session: Session },
    Locked(Timestamp),
    Bad,
}

fn attempt(
    conn: &Connection,
    req: &LoginRequest,
    source: Option<&str>,
) -> rusqlite::Result<Outcome> {
    let now = Timestamp::now();
    let Some(user) = load(conn, &req.username)? else {
        password::burn_time(&req.password);
        return Ok(Outcome::Bad);
    };
    if let Some(until) = user.locked_until.filter(|t| *t > now) {
        password::burn_time(&req.password);
        return Ok(Outcome::Locked(until));
    }
    let good = !user.disabled && password::verify(&req.password, &user.hash);
    if !good {
        let in_window = user
            .first_failed_at
            .is_some_and(|t| now.seconds_since(t) < LOCK_SECS);
        let failed = if in_window { user.failed + 1 } else { 1 };
        let first = if in_window {
            user.first_failed_at
        } else {
            Some(now)
        };
        let lock = (failed >= LOCK_AFTER).then(|| now.plus_seconds(LOCK_SECS));
        conn.execute(
            "UPDATE users SET failed_logins = ?1, first_failed_at = ?2, locked_until = ?3 WHERE id = ?4",
            params![
                failed,
                first.map(Timestamp::to_rfc3339),
                lock.map(Timestamp::to_rfc3339),
                user.id.to_string()
            ],
        )?;
        return Ok(lock.map_or(Outcome::Bad, Outcome::Locked));
    }
    conn.execute(
        "UPDATE users SET failed_logins = 0, first_failed_at = NULL, locked_until = NULL, last_login_at = ?1 \
         WHERE id = ?2",
        params![now.to_rfc3339(), user.id.to_string()],
    )?;
    let (secret, row) = session::create(conn, user.id, source)?;
    Ok(Outcome::Ok {
        cookie: session::set_cookie(&secret),
        session: Session {
            actor: Actor {
                id: Some(user.id),
                username: user.username,
                role: user.role,
                via: Via::Session,
                token_id: None,
            },
            expires_at: Some(row.expires_at),
            idle_expires_at: Some(row.idle_expires_at),
        },
    })
}

/// `POST /auth/login`.
pub async fn login(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Json(req): Json<LoginRequest>,
) -> Response {
    let source = ctx.source.clone().unwrap_or_else(|| "unknown".to_owned());
    if let Err(retry) = state.login_limiter.check(&source) {
        let mut response = ApiError::new(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests")
            .detail("too many login attempts; slow down")
            .into_response();
        if let Ok(v) = HeaderValue::from_str(&retry.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, v);
        }
        return response;
    }
    if req.username.is_empty() || req.username.len() > 64 || req.password.len() > password::MAX_LEN
    {
        return ApiError::unauthorized().into_response();
    }

    let attempt_req = req.clone();
    let attempt_source = ctx.source.clone();
    let outcome = state
        .db
        .call(move |conn| attempt(conn, &attempt_req, attempt_source.as_deref()))
        .await;
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return e.into_response(),
    };

    let anonymous = Actor {
        id: None,
        username: req.username.clone(),
        role: Role::Viewer,
        via: Via::Session,
        token_id: None,
    };
    let object = ObjectRef {
        kind: "user".to_owned(),
        id: None,
        name: Some(req.username.clone()),
    };
    match outcome {
        Outcome::Ok { cookie, session } => {
            let rec = Record::ok(
                "auth.login",
                ObjectRef::new("user", session.actor.id.unwrap_or_default(), &req.username),
            );
            if let Err(e) = state.record(&session.actor, &ctx, rec).await {
                return e.into_response();
            }
            let mut response = (StatusCode::OK, Json(session)).into_response();
            if let Ok(v) = HeaderValue::from_str(&cookie) {
                response.headers_mut().insert(header::SET_COOKIE, v);
            }
            response
        }
        Outcome::Locked(until) => {
            let _ = state
                .record(
                    &anonymous,
                    &ctx,
                    Record::denied("auth.login", object, "account locked"),
                )
                .await;
            ApiError::typed(StatusCode::LOCKED, "locked", "Locked")
                .detail(format!("too many failed logins; locked until {until}"))
                .into_response()
        }
        Outcome::Bad => {
            let _ = state
                .record(
                    &anonymous,
                    &ctx,
                    Record::denied("auth.login", object, "invalid credentials"),
                )
                .await;
            ApiError::typed(
                StatusCode::UNAUTHORIZED,
                "invalid-credentials",
                "Unauthorized",
            )
            .detail("invalid username or password")
            .into_response()
        }
    }
}

/// `POST /auth/logout`.
pub async fn logout(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    auth: Auth,
) -> ApiResult<Response> {
    if let Some(hash) = auth.session_hash.clone() {
        state
            .db
            .call(move |conn| session::delete(conn, &hash))
            .await?;
    }
    let object = ObjectRef {
        kind: "user".to_owned(),
        id: auth.actor.id,
        name: Some(auth.actor.username.clone()),
    };
    state
        .record(&auth.actor, &ctx, Record::ok("auth.logout", object))
        .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(v) = HeaderValue::from_str(&session::clear_cookie()) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    Ok(response)
}

/// `GET /auth/session`.
pub async fn session(auth: Auth) -> Json<Session> {
    Json(Session {
        actor: auth.actor,
        expires_at: auth.expires_at,
        idle_expires_at: auth.idle_expires_at,
    })
}
