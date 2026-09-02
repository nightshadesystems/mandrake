//! `/jobs`. Phase 2 has no job producers yet; the table and the read
//! endpoints exist so later phases only add workers.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use mandrake_core::{
    Id, Role,
    api::{Job, JobState, ObjectRef, Page},
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

use crate::{
    app::AppState,
    auth::Auth,
    cursor::{self, Pagination},
    db,
    error::{ApiError, ApiResult},
};

const COLUMNS: &str = "seq, id, state, kind, target_kind, target_id, target_name, progress, message, created_at, started_at, finished_at, error";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Job)> {
    let state: String = r.get("state")?;
    let target_kind: Option<String> = r.get("target_kind")?;
    Ok((
        r.get("seq")?,
        Job {
            id: db::get_id(r, "id")?,
            state: match state.as_str() {
                "queued" => JobState::Queued,
                "running" => JobState::Running,
                "succeeded" => JobState::Succeeded,
                "cancelled" => JobState::Cancelled,
                _ => JobState::Failed,
            },
            kind: r.get("kind")?,
            target: target_kind
                .map(|kind| -> rusqlite::Result<ObjectRef> {
                    Ok(ObjectRef {
                        kind,
                        id: db::get_opt_id(r, "target_id")?,
                        name: r.get("target_name")?,
                    })
                })
                .transpose()?,
            progress: r.get("progress")?,
            message: r.get("message")?,
            created_at: db::get_ts(r, "created_at")?,
            started_at: db::get_opt_ts(r, "started_at")?,
            finished_at: db::get_opt_ts(r, "finished_at")?,
            error: db::get_opt_json(r, "error")?.and_then(|v| serde_json::from_value(v).ok()),
        },
    ))
}

/// `GET /jobs` query.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Only this state.
    pub state: Option<JobState>,
    /// Cursor.
    pub cursor: Option<String>,
    /// Page size.
    pub limit: Option<u32>,
}

/// `GET /jobs`.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Page<Job>>> {
    auth.require(Role::Viewer)?;
    let p = Pagination {
        cursor: q.cursor,
        limit: q.limit,
    };
    let limit = p.limit();
    let after: i64 = p.after().and_then(|s| s.parse().ok()).unwrap_or(i64::MAX);
    let wanted = q
        .state
        .and_then(|s| serde_json::to_value(s).ok())
        .and_then(|v| v.as_str().map(str::to_owned));
    let rows = state
        .db
        .call(move |conn| {
            let sql = format!(
                "SELECT {COLUMNS} FROM jobs WHERE seq < ?1 AND (?2 IS NULL OR state = ?2) ORDER BY seq DESC LIMIT ?3"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![after, wanted, i64::from(limit) + 1], from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    let (items, next_cursor) = cursor::page(rows, limit, |(seq, _)| seq.to_string());
    Ok(Json(Page {
        items: items.into_iter().map(|(_, j)| j).collect(),
        next_cursor,
    }))
}

/// `GET /jobs/{id}`.
pub async fn get_one(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Job>> {
    auth.require(Role::Viewer)?;
    let job = state
        .db
        .call(move |conn| {
            conn.query_row(
                &format!("SELECT {COLUMNS} FROM jobs WHERE id = ?1"),
                [id.to_string()],
                |r| from_row(r).map(|(_, j)| j),
            )
            .optional()
        })
        .await?;
    job.map(Json).ok_or_else(|| ApiError::not_found("job"))
}
