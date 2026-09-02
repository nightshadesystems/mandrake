//! `/jobs`: read-only view of the job table (`crate::jobs` runs them).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use mandrake_core::{
    Id, Role,
    api::{Job, JobState, Page},
};
use rusqlite::params;
use serde::Deserialize;

use crate::{
    app::AppState,
    auth::Auth,
    cursor::{self, Pagination},
    error::{ApiError, ApiResult},
    jobs,
};

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
            let mut stmt = conn.prepare(
                "SELECT seq, id, state, kind, target_kind, target_id, target_name, progress, \
                 message, created_at, started_at, finished_at, error FROM jobs \
                 WHERE seq < ?1 AND (?2 IS NULL OR state = ?2) ORDER BY seq DESC LIMIT ?3",
            )?;
            let rows =
                stmt.query_map(params![after, wanted, i64::from(limit) + 1], jobs::from_row)?;
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
    let job = state.db.call(move |conn| jobs::find(conn, id)).await?;
    job.map(Json).ok_or_else(|| ApiError::not_found("job"))
}
