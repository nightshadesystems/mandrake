//! Background jobs (spec §6.1, ADR-0011).
//!
//! A job is a row in `jobs` plus a tokio task. The task reports progress
//! through a [`JobContext`], which updates the row and publishes
//! `job.progress` events; the outcome is recorded as `succeeded` or
//! `failed` with the problem. Jobs do not survive a daemon restart: on
//! startup every row still `queued` or `running` is marked failed.

use std::{future::Future, sync::Arc};

use mandrake_core::{
    Actor, Id, Timestamp,
    api::{Event, Job, JobState, ObjectRef},
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    app::AppState,
    db,
    error::{ApiError, ApiResult},
};

const COLUMNS: &str = "seq, id, state, kind, target_kind, target_id, target_name, progress, message, created_at, started_at, finished_at, error";

/// Read a job row.
pub fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Job)> {
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

/// Load one job.
pub fn find(conn: &Connection, id: Id) -> rusqlite::Result<Option<Job>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM jobs WHERE id = ?1"),
        [id.to_string()],
        |r| from_row(r).map(|(_, j)| j),
    )
    .optional()
}

/// Mark jobs left `queued` or `running` by a previous daemon as failed.
pub fn recover(conn: &Connection) -> rusqlite::Result<usize> {
    let error = serde_json::json!({
        "type": "about:blank",
        "title": "Interrupted",
        "status": 500,
        "detail": "the daemon restarted while this job was running"
    });
    conn.execute(
        "UPDATE jobs SET state = 'failed', finished_at = ?1, error = ?2 \
         WHERE state IN ('queued', 'running')",
        params![Timestamp::now().to_rfc3339(), error.to_string()],
    )
}

/// Handed to a job's work: report progress, publish events.
#[derive(Clone)]
pub struct JobContext {
    state: AppState,
    /// The job id.
    pub id: Id,
    kind: String,
    target: Option<ObjectRef>,
    actor: Option<Actor>,
}

impl JobContext {
    /// Report progress in `0.0..=1.0` with a message.
    pub async fn progress(&self, fraction: f64, message: impl Into<String>) {
        let message = message.into();
        let id = self.id;
        let fraction = fraction.clamp(0.0, 1.0);
        let msg = message.clone();
        let _ = self
            .state
            .db
            .call(move |conn| {
                conn.execute(
                    "UPDATE jobs SET progress = ?1, message = ?2 WHERE id = ?3 AND state = 'running'",
                    params![fraction, msg, id.to_string()],
                )
            })
            .await;
        self.publish("job.progress").await;
    }

    async fn set_state(&self, state: JobState, error: Option<&ApiError>) {
        let id = self.id;
        let now = Timestamp::now().to_rfc3339();
        let error_json = error.map(|e| serde_json::to_string(&e.0).unwrap_or_default());
        let state_str = match state {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Succeeded => "succeeded",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        };
        let _ = self
            .state
            .db
            .call(move |conn| match state {
                JobState::Running => conn.execute(
                    "UPDATE jobs SET state = 'running', started_at = ?1 WHERE id = ?2",
                    params![now, id.to_string()],
                ),
                JobState::Succeeded => conn.execute(
                    "UPDATE jobs SET state = 'succeeded', progress = 1.0, finished_at = ?1 WHERE id = ?2",
                    params![now, id.to_string()],
                ),
                _ => conn.execute(
                    "UPDATE jobs SET state = ?1, finished_at = ?2, error = ?3 WHERE id = ?4",
                    params![state_str, now, error_json, id.to_string()],
                ),
            })
            .await;
        self.publish(&format!("job.{state_str}")).await;
    }

    async fn publish(&self, kind: &str) {
        let id = self.id;
        let job = self
            .state
            .db
            .call(move |conn| find(conn, id))
            .await
            .ok()
            .flatten();
        let Some(job) = job else { return };
        let at = Timestamp::now();
        let data = serde_json::to_value(&job).ok();
        let object = ObjectRef {
            kind: "job".to_owned(),
            id: Some(id),
            name: Some(self.kind.clone()),
        };
        let actor = self.actor.clone();
        let target = self.target.clone();
        let db_kind = kind.to_owned();
        let db_data = data.clone();
        let db_actor = actor.clone();
        let event_id = self
            .state
            .db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO events (at, kind, object_kind, object_id, object_name, actor, data) \
                     VALUES (?1, ?2, 'job', ?3, ?4, ?5, ?6)",
                    params![
                        at.to_rfc3339(),
                        db_kind,
                        id.to_string(),
                        target.as_ref().and_then(|t| t.name.clone()),
                        serde_json::to_string(&db_actor).ok(),
                        db_data.as_ref().map(ToString::to_string),
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .unwrap_or(0);
        self.state.events.publish(Event {
            id: event_id.to_string(),
            at,
            kind: kind.to_owned(),
            object: Some(object),
            actor,
            data,
        });
    }
}

impl AppState {
    /// Record a job and run `work` on a background task. Returns the job
    /// as queued; the caller answers `202` with it.
    pub async fn start_job<F, Fut>(
        &self,
        kind: &str,
        target: Option<ObjectRef>,
        actor: Option<&Actor>,
        work: F,
    ) -> ApiResult<Job>
    where
        F: FnOnce(JobContext) -> Fut + Send + 'static,
        Fut: Future<Output = ApiResult<String>> + Send + 'static,
    {
        let id = Id::new();
        let now = Timestamp::now();
        let kind_owned = kind.to_owned();
        let target_row = target.clone();
        let job = self
            .db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO jobs (id, state, kind, target_kind, target_id, target_name, created_at) \
                     VALUES (?1, 'queued', ?2, ?3, ?4, ?5, ?6)",
                    params![
                        id.to_string(),
                        kind_owned,
                        target_row.as_ref().map(|t| t.kind.clone()),
                        target_row.as_ref().and_then(|t| t.id.map(|i| i.to_string())),
                        target_row.as_ref().and_then(|t| t.name.clone()),
                        now.to_rfc3339(),
                    ],
                )?;
                find(conn, id)
            })
            .await?
            .ok_or_else(|| ApiError::internal("job row vanished"))?;

        let ctx = JobContext {
            state: self.clone(),
            id,
            kind: kind.to_owned(),
            target,
            actor: actor.cloned(),
        };
        let runner_ctx = ctx.clone();
        tokio::spawn(async move {
            runner_ctx.set_state(JobState::Running, None).await;
            let outcome = work(runner_ctx.clone()).await;
            match outcome {
                Ok(message) => {
                    runner_ctx.progress(1.0, message).await;
                    runner_ctx.set_state(JobState::Succeeded, None).await;
                }
                Err(e) => {
                    tracing::warn!(job = %runner_ctx.id, error = %e, "job failed");
                    runner_ctx.set_state(JobState::Failed, Some(&e)).await;
                }
            }
        });
        Ok(job)
    }
}

/// Shared handle type for tests and routes that need to await a job.
pub type SharedState = Arc<AppState>;
