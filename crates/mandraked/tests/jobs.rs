//! The job runner: progress, outcomes, events, and recovery after restart.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::time::Duration;

use mandrake_core::{
    Id,
    api::{JobState, ObjectRef},
};
use mandraked::{app::AppState, db::Db, error::ApiError, jobs};

async fn state() -> AppState {
    AppState::new(Db::open_in_memory().expect("db"))
        .await
        .expect("state")
}

async fn wait_for(state: &AppState, id: Id, want: JobState) -> mandrake_core::api::Job {
    for _ in 0..200 {
        let job = state
            .db
            .call(move |conn| jobs::find(conn, id))
            .await
            .expect("query")
            .expect("job exists");
        if job.state == want {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} never reached {want:?}");
}

async fn event_kinds(state: &AppState) -> Vec<String> {
    state
        .db
        .call(|conn| {
            let mut stmt = conn.prepare("SELECT kind FROM events ORDER BY id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect()
        })
        .await
        .expect("events")
}

#[tokio::test]
async fn a_job_reports_progress_and_succeeds() {
    let state = state().await;
    let mut rx = state.events.subscribe();
    let target = ObjectRef::new("pool", Id::new(), "tank");
    let job = state
        .start_job("pool.scrub", Some(target.clone()), None, |ctx| async move {
            ctx.progress(0.25, "a quarter").await;
            ctx.progress(0.5, "half").await;
            Ok("scrub finished".to_owned())
        })
        .await
        .expect("start");
    assert_eq!(job.state, JobState::Queued);
    assert_eq!(job.kind, "pool.scrub");
    assert_eq!(job.target, Some(target));

    let done = wait_for(&state, job.id, JobState::Succeeded).await;
    assert_eq!(done.progress, Some(1.0));
    assert_eq!(done.message.as_deref(), Some("scrub finished"));
    assert!(done.started_at.is_some() && done.finished_at.is_some());
    assert!(done.error.is_none());

    let kinds = event_kinds(&state).await;
    assert_eq!(
        kinds,
        vec![
            "job.running",
            "job.progress",
            "job.progress",
            "job.progress",
            "job.succeeded"
        ]
    );
    // The bus saw them live too.
    let first = rx.recv().await.expect("live event");
    assert_eq!(first.kind, "job.running");
    assert_eq!(first.object.and_then(|o| o.id), Some(job.id));
}

#[tokio::test]
async fn a_failing_job_records_the_problem() {
    let state = state().await;
    let job = state
        .start_job("dataset.destroy", None, None, |_ctx| async move {
            Err::<String, _>(ApiError::conflict("dataset is busy"))
        })
        .await
        .expect("start");
    let failed = wait_for(&state, job.id, JobState::Failed).await;
    let problem = failed.error.expect("problem recorded");
    assert_eq!(problem.status, 409);
    assert_eq!(problem.detail.as_deref(), Some("dataset is busy"));
    assert!(
        event_kinds(&state)
            .await
            .ends_with(&["job.failed".to_owned()])
    );
}

#[tokio::test]
async fn recovery_fails_jobs_left_running() {
    let state = state().await;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let job = state
        .start_job("image.import", None, None, |_ctx| async move {
            let _ = rx.await;
            Ok(String::new())
        })
        .await
        .expect("start");
    wait_for(&state, job.id, JobState::Running).await;
    let n = state
        .db
        .call(|conn| jobs::recover(conn))
        .await
        .expect("recover");
    assert_eq!(n, 1);
    let job = wait_for(&state, job.id, JobState::Failed).await;
    assert_eq!(job.error.map(|p| p.title), Some("Interrupted".to_owned()));
    drop(tx);
}
