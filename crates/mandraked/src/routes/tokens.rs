//! `/tokens`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use mandrake_core::{
    Id, Role, Timestamp,
    api::{ObjectRef, Page, Token, TokenCreate, TokenCreated},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::{Auth, token},
    cursor::{self, Pagination},
    db,
    error::{ApiError, ApiResult},
};

const COLUMNS: &str = "seq, id, user_id, name, prefix, created_at, expires_at, last_used_at";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Token)> {
    Ok((
        r.get("seq")?,
        Token {
            id: db::get_id(r, "id")?,
            user_id: db::get_id(r, "user_id")?,
            name: r.get("name")?,
            prefix: r.get("prefix")?,
            created_at: db::get_ts(r, "created_at")?,
            expires_at: db::get_opt_ts(r, "expires_at")?,
            last_used_at: db::get_opt_ts(r, "last_used_at")?,
        },
    ))
}

fn find(conn: &Connection, id: Id) -> rusqlite::Result<Option<Token>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM tokens WHERE id = ?1"),
        [id.to_string()],
        |r| from_row(r).map(|(_, t)| t),
    )
    .optional()
}

/// `GET /tokens` query.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Another user's tokens; admin only.
    pub user_id: Option<Id>,
    /// Cursor.
    pub cursor: Option<String>,
    /// Page size.
    pub limit: Option<u32>,
}

/// `GET /tokens`.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Page<Token>>> {
    auth.require(Role::Viewer)?;
    let owner = match (q.user_id, auth.actor.id) {
        (Some(requested), Some(me)) if requested != me => {
            auth.require(Role::Admin)?;
            Some(requested)
        }
        (Some(requested), _) => {
            auth.require(Role::Admin)?;
            Some(requested)
        }
        (None, Some(me)) => Some(me),
        // Root over the socket has no tokens of its own: list everything.
        (None, None) => None,
    };
    let p = Pagination {
        cursor: q.cursor,
        limit: q.limit,
    };
    let limit = p.limit();
    let after: i64 = p.after().and_then(|s| s.parse().ok()).unwrap_or(i64::MAX);
    let rows = state
        .db
        .call(move |conn| {
            let sql = format!(
                "SELECT {COLUMNS} FROM tokens WHERE seq < ?1 AND (?2 IS NULL OR user_id = ?2) \
                 ORDER BY seq DESC LIMIT ?3"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                params![after, owner.map(|o| o.to_string()), i64::from(limit) + 1],
                from_row,
            )?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    let (items, next_cursor) = cursor::page(rows, limit, |(seq, _)| seq.to_string());
    Ok(Json(Page {
        items: items.into_iter().map(|(_, t)| t).collect(),
        next_cursor,
    }))
}

/// `POST /tokens`.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<TokenCreate>,
) -> ApiResult<(StatusCode, Json<TokenCreated>)> {
    auth.require(Role::Viewer)?;
    if body.name.is_empty() || body.name.len() > 128 {
        return Err(ApiError::unprocessable("name must be 1-128 characters"));
    }
    if body.expires_in_seconds.is_some_and(|s| s < 60) {
        return Err(ApiError::unprocessable(
            "expires_in_seconds must be at least 60",
        ));
    }
    let owner = match (body.user_id, auth.actor.id) {
        (Some(requested), Some(me)) if requested == me => me,
        (Some(requested), _) => {
            auth.require(Role::Admin)?;
            requested
        }
        (None, Some(me)) => me,
        (None, None) => {
            return Err(ApiError::unprocessable(
                "user_id is required when acting as root",
            ));
        }
    };
    let generated = token::generate();
    let id = Id::new();
    let now = Timestamp::now();
    let expires_at = body.expires_in_seconds.map(|s| now.plus_seconds(s));
    let name = body.name.clone();
    let hash = generated.hash.clone();
    let prefix = generated.prefix.clone();
    let created = state
        .db
        .call(move |conn| {
            let exists: Option<String> = conn
                .query_row(
                    "SELECT id FROM users WHERE id = ?1",
                    [owner.to_string()],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Ok(None);
            }
            conn.execute(
                "INSERT INTO tokens (id, user_id, name, prefix, hash, created_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id.to_string(),
                    owner.to_string(),
                    name,
                    prefix,
                    hash,
                    now.to_rfc3339(),
                    expires_at.map(Timestamp::to_rfc3339)
                ],
            )?;
            find(conn, id)
        })
        .await?;
    let Some(token_meta) = created else {
        return Err(ApiError::not_found("user"));
    };
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "token.create",
                ObjectRef::new("token", id, &token_meta.name),
            )
            .after(json!({
                "name": token_meta.name,
                "prefix": token_meta.prefix,
                "user_id": token_meta.user_id,
                "expires_at": token_meta.expires_at,
            })),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(TokenCreated {
            token: token_meta,
            secret: generated.secret,
        }),
    ))
}

async fn load_owned(state: &AppState, auth: &Auth, id: Id) -> ApiResult<Token> {
    let found = state.db.call(move |conn| find(conn, id)).await?;
    let token_meta = found.ok_or_else(|| ApiError::not_found("token"))?;
    if !auth.actor.is_user(token_meta.user_id) {
        auth.require(Role::Admin)
            .map_err(|_| ApiError::not_found("token"))?;
    }
    Ok(token_meta)
}

/// `GET /tokens/{id}`.
pub async fn get_one(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Token>> {
    load_owned(&state, &auth, id).await.map(Json)
}

/// `DELETE /tokens/{id}`.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    let token_meta = load_owned(&state, &auth, id).await?;
    state
        .db
        .call(move |conn| conn.execute("DELETE FROM tokens WHERE id = ?1", [id.to_string()]))
        .await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "token.revoke",
                ObjectRef::new("token", id, &token_meta.name),
            )
            .before(json!({
                "name": token_meta.name,
                "prefix": token_meta.prefix,
                "user_id": token_meta.user_id,
            })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
