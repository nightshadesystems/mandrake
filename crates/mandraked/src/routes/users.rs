//! `/users`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use mandrake_core::{
    Id, Role, Timestamp,
    api::{ObjectRef, Page, PasswordChange, User, UserCreate, UserUpdate},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::{Auth, password, session},
    cursor::{self, Pagination},
    db,
    error::{ApiError, ApiResult},
};

const COLUMNS: &str = "id, username, display_name, role, disabled, locked_until, last_login_at, created_at, updated_at";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    let role: String = r.get("role")?;
    Ok(User {
        id: db::get_id(r, "id")?,
        username: r.get("username")?,
        role: role.parse().unwrap_or(Role::Viewer),
        display_name: r.get("display_name")?,
        disabled: r.get::<_, i64>("disabled")? != 0,
        locked_until: db::get_opt_ts(r, "locked_until")?.filter(|t| *t > Timestamp::now()),
        last_login_at: db::get_opt_ts(r, "last_login_at")?,
        created_at: db::get_ts(r, "created_at")?,
        updated_at: db::get_ts(r, "updated_at")?,
    })
}

/// Load one user by id.
pub fn find(conn: &Connection, id: Id) -> rusqlite::Result<Option<User>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM users WHERE id = ?1"),
        [id.to_string()],
        from_row,
    )
    .optional()
}

fn enabled_admins(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM users WHERE role = 'admin' AND disabled = 0",
        [],
        |r| r.get(0),
    )
}

/// Audit summary of a user; never includes secrets.
pub fn summary(u: &User) -> Value {
    json!({
        "username": u.username,
        "role": u.role,
        "display_name": u.display_name,
        "disabled": u.disabled,
    })
}

fn validate_username(name: &str) -> Result<(), ApiError> {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    let rest_ok =
        chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'));
    if !first_ok || !rest_ok || name.len() > 32 {
        return Err(ApiError::unprocessable(
            "username must be 1-32 lowercase letters, digits, `_`, `.`, or `-`, starting with a letter or `_`",
        ));
    }
    Ok(())
}

/// `GET /users`.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(p): Query<Pagination>,
) -> ApiResult<Json<Page<User>>> {
    auth.require(Role::Viewer)?;
    let limit = p.limit();
    let after = p.after().unwrap_or_default();
    let rows = state
        .db
        .call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM users WHERE username > ?1 ORDER BY username LIMIT ?2"
            ))?;
            let rows = stmt.query_map(params![after, i64::from(limit) + 1], from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    let (items, next_cursor) = cursor::page(rows, limit, |u| u.username.clone());
    Ok(Json(Page { items, next_cursor }))
}

/// `POST /users`.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<UserCreate>,
) -> ApiResult<(StatusCode, Json<User>)> {
    auth.require(Role::Admin)?;
    validate_username(&body.username)?;
    password::check_policy(&body.password).map_err(ApiError::unprocessable)?;
    let hash = password::hash(&body.password).map_err(ApiError::internal)?;
    let id = Id::new();
    let now = Timestamp::now().to_rfc3339();
    let username = body.username.clone();
    let created = state
        .db
        .call(move |conn| {
            let exists: Option<String> = conn
                .query_row("SELECT id FROM users WHERE username = ?1", [&body.username], |r| r.get(0))
                .optional()?;
            if exists.is_some() {
                return Ok(None);
            }
            conn.execute(
                "INSERT INTO users (id, username, display_name, role, password_hash, disabled, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
                params![id.to_string(), body.username, body.display_name, body.role.as_str(), hash, now],
            )?;
            find(conn, id)
        })
        .await?;
    let Some(user) = created else {
        return Err(ApiError::conflict(&format!(
            "username `{username}` already exists"
        )));
    };
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "user.create",
                ObjectRef::new("user", user.id, &user.username),
            )
            .after(summary(&user)),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

/// `GET /users/{id}`.
pub async fn get_one(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<User>> {
    auth.require(Role::Viewer)?;
    let user = state.db.call(move |conn| find(conn, id)).await?;
    user.map(Json).ok_or_else(|| ApiError::not_found("user"))
}

/// `PATCH /users/{id}`.
pub async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<UserUpdate>,
) -> ApiResult<Json<User>> {
    auth.require(Role::Admin)?;
    if body.display_name.as_ref().is_some_and(|d| d.len() > 128) {
        return Err(ApiError::unprocessable("display_name is too long"));
    }
    let is_self = auth.actor.is_user(id);
    if is_self && body.role.is_some_and(|r| r != Role::Admin) {
        return Err(ApiError::typed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "self-demotion",
            "Unprocessable Entity",
        )
        .detail("an admin cannot remove their own admin role"));
    }
    if is_self && body.disabled == Some(true) {
        return Err(ApiError::unprocessable(
            "you cannot disable your own account",
        ));
    }
    let before = state
        .db
        .call(move |conn| find(conn, id))
        .await?
        .ok_or_else(|| ApiError::not_found("user"))?;

    let loses_admin = before.role == Role::Admin
        && !before.disabled
        && (body.role.is_some_and(|r| r != Role::Admin) || body.disabled == Some(true));
    let update_body = body.clone();
    let after = state
        .db
        .call(move |conn| {
            if loses_admin && enabled_admins(conn)? <= 1 {
                return Ok(None);
            }
            conn.execute(
                "UPDATE users SET role = COALESCE(?1, role), display_name = COALESCE(?2, display_name), \
                 disabled = COALESCE(?3, disabled), updated_at = ?4 WHERE id = ?5",
                params![
                    update_body.role.map(Role::as_str),
                    update_body.display_name,
                    update_body.disabled.map(i64::from),
                    Timestamp::now().to_rfc3339(),
                    id.to_string()
                ],
            )?;
            if update_body.disabled == Some(true) {
                session::delete_for_user(conn, id, None)?;
            }
            find(conn, id)
        })
        .await?;
    let Some(after) = after else {
        return Err(ApiError::typed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "last-admin",
            "Unprocessable Entity",
        )
        .detail("this is the last enabled admin"));
    };
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("user.update", ObjectRef::new("user", id, &after.username))
                .before(summary(&before))
                .after(summary(&after)),
        )
        .await?;
    Ok(Json(after))
}

/// `DELETE /users/{id}`.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    if auth.actor.is_user(id) {
        return Err(ApiError::unprocessable(
            "you cannot delete your own account",
        ));
    }
    let before = state
        .db
        .call(move |conn| find(conn, id))
        .await?
        .ok_or_else(|| ApiError::not_found("user"))?;
    let was_admin = before.role == Role::Admin && !before.disabled;
    let deleted = state
        .db
        .call(move |conn| {
            if was_admin && enabled_admins(conn)? <= 1 {
                return Ok(false);
            }
            conn.execute("DELETE FROM users WHERE id = ?1", [id.to_string()])?;
            Ok(true)
        })
        .await?;
    if !deleted {
        return Err(ApiError::typed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "last-admin",
            "Unprocessable Entity",
        )
        .detail("this is the last enabled admin"));
    }
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("user.delete", ObjectRef::new("user", id, &before.username))
                .before(summary(&before)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /users/{id}/password`.
pub async fn set_password(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<PasswordChange>,
) -> ApiResult<StatusCode> {
    auth.require_self_or(id, Role::Admin)?;
    password::check_policy(&body.new_password).map_err(ApiError::unprocessable)?;
    let is_self = auth.actor.is_user(id);
    if is_self && body.current_password.is_none() {
        return Err(ApiError::unprocessable(
            "current_password is required when changing your own password",
        ));
    }
    let user = state
        .db
        .call(move |conn| find(conn, id))
        .await?
        .ok_or_else(|| ApiError::not_found("user"))?;
    let new_hash = password::hash(&body.new_password).map_err(ApiError::internal)?;
    let keep = auth.session_hash.clone();
    let current = body.current_password.clone();
    let changed = state
        .db
        .call(move |conn| {
            if is_self {
                let stored: String = conn.query_row(
                    "SELECT password_hash FROM users WHERE id = ?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?;
                if !current
                    .as_deref()
                    .is_some_and(|c| password::verify(c, &stored))
                {
                    return Ok(false);
                }
            }
            conn.execute(
                "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_hash, Timestamp::now().to_rfc3339(), id.to_string()],
            )?;
            session::delete_for_user(conn, id, keep.as_deref())?;
            Ok(true)
        })
        .await?;
    if !changed {
        return Err(ApiError::forbidden("current password is incorrect"));
    }
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("user.password", ObjectRef::new("user", id, &user.username)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
