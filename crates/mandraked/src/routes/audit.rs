//! `GET /audit`.

use axum::{
    Json,
    extract::{Query, State},
};
use mandrake_core::{
    Actor, Id, Role, Timestamp, Via,
    api::{AuditEntry, AuditResult, ObjectRef, Page},
};
use rusqlite::{params_from_iter, types::Value};
use serde::Deserialize;

use crate::{
    app::AppState,
    auth::Auth,
    cursor::{self, Pagination},
    db,
    error::ApiResult,
};

/// `GET /audit` query.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Only this actor.
    pub actor_id: Option<Id>,
    /// Only this object.
    pub object_id: Option<Id>,
    /// Only this action.
    pub action: Option<String>,
    /// Not before.
    pub since: Option<Timestamp>,
    /// Not after.
    pub until: Option<Timestamp>,
    /// Cursor.
    pub cursor: Option<String>,
    /// Page size.
    pub limit: Option<u32>,
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, AuditEntry)> {
    let id: i64 = r.get("id")?;
    let role: String = r.get("actor_role")?;
    let via: String = r.get("actor_via")?;
    let result: String = r.get("result")?;
    Ok((
        id,
        AuditEntry {
            id: id.to_string(),
            at: db::get_ts(r, "at")?,
            actor: Actor {
                id: db::get_opt_id(r, "actor_id")?,
                username: r.get("actor_username")?,
                role: role.parse().unwrap_or(Role::Viewer),
                via: match via.as_str() {
                    "token" => Via::Token,
                    "socket" => Via::Socket,
                    _ => Via::Session,
                },
                token_id: db::get_opt_id(r, "actor_token_id")?,
            },
            action: r.get("action")?,
            object: ObjectRef {
                kind: r.get("object_kind")?,
                id: db::get_opt_id(r, "object_id")?,
                name: r.get("object_name")?,
            },
            before: db::get_opt_json(r, "before")?,
            after: db::get_opt_json(r, "after")?,
            result: match result.as_str() {
                "denied" => AuditResult::Denied,
                "failed" => AuditResult::Failed,
                _ => AuditResult::Ok,
            },
            detail: r.get("detail")?,
            request_id: r.get("request_id")?,
            source: r.get("source")?,
        },
    ))
}

/// `GET /audit`.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Page<AuditEntry>>> {
    auth.require(Role::Viewer)?;
    let p = Pagination {
        cursor: q.cursor.clone(),
        limit: q.limit,
    };
    let limit = p.limit();
    let before_id: i64 = p.after().and_then(|s| s.parse().ok()).unwrap_or(i64::MAX);

    let mut clauses = vec!["id < ?".to_owned()];
    let mut values: Vec<Value> = vec![Value::Integer(before_id)];
    if let Some(a) = q.actor_id {
        clauses.push("actor_id = ?".to_owned());
        values.push(Value::Text(a.to_string()));
    }
    if let Some(o) = q.object_id {
        clauses.push("object_id = ?".to_owned());
        values.push(Value::Text(o.to_string()));
    }
    if let Some(action) = q.action {
        clauses.push("action = ?".to_owned());
        values.push(Value::Text(action));
    }
    if let Some(since) = q.since {
        clauses.push("at >= ?".to_owned());
        values.push(Value::Text(since.to_rfc3339()));
    }
    if let Some(until) = q.until {
        clauses.push("at <= ?".to_owned());
        values.push(Value::Text(until.to_rfc3339()));
    }
    values.push(Value::Integer(i64::from(limit) + 1));
    let sql = format!(
        "SELECT id, at, actor_id, actor_username, actor_role, actor_via, actor_token_id, action, \
         object_kind, object_id, object_name, before, after, result, detail, request_id, source \
         FROM audit WHERE {} ORDER BY id DESC LIMIT ?",
        clauses.join(" AND ")
    );
    let rows = state
        .db
        .call(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(values), from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    let (items, next_cursor) = cursor::page(rows, limit, |(id, _)| id.to_string());
    Ok(Json(Page {
        items: items.into_iter().map(|(_, e)| e).collect(),
        next_cursor,
    }))
}
