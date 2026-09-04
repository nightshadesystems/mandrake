//! Updates into a new boot environment (ADR-0015): the state kept in
//! SQLite, the boot-environment name rule, and the check and apply jobs.

use mandrake_core::{
    Actor, Id, Timestamp,
    api::{Job, ObjectRef},
    system::{UpdatePlan, UpdateState},
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    app::AppState,
    error::{ApiError, ApiResult},
    pkg::DryRun,
};

/// The package whose version names the boot environment.
pub const INCORPORATION: &str = "incorporation/mandrake/mandrake-incorporation";

/// The audit and job target for host-wide operations.
pub fn host_ref(state: &AppState) -> ObjectRef {
    ObjectRef::new("system", state.host_id, "host")
}

/// The stored row.
#[derive(Debug, Clone, Default)]
pub struct Stored {
    /// The last plan.
    pub plan: Option<UpdatePlan>,
    /// Last check job.
    pub check_job: Option<Id>,
    /// Last apply job.
    pub apply_job: Option<Id>,
    /// When the last apply finished.
    pub applied_at: Option<Timestamp>,
    /// The BE it created.
    pub applied_be: Option<String>,
    /// The BE that was active before it.
    pub previous_be: Option<String>,
}

/// The six nullable columns of the row.
type Row = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Read the row.
pub fn load(conn: &Connection) -> rusqlite::Result<Stored> {
    let row: Option<Row> = conn
        .query_row(
            "SELECT plan, check_job, apply_job, applied_at, applied_be, previous_be \
             FROM update_state WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((plan, check_job, apply_job, applied_at, applied_be, previous_be)) = row else {
        return Ok(Stored::default());
    };
    Ok(Stored {
        plan: plan.and_then(|p| serde_json::from_str(&p).ok()),
        check_job: check_job.and_then(|s| s.parse().ok()),
        apply_job: apply_job.and_then(|s| s.parse().ok()),
        applied_at: applied_at.and_then(|s| s.parse().ok()),
        applied_be,
        previous_be,
    })
}

/// Write the row.
pub fn save(conn: &Connection, s: &Stored) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE update_state SET plan = ?1, check_job = ?2, apply_job = ?3, applied_at = ?4, \
         applied_be = ?5, previous_be = ?6 WHERE id = 1",
        params![
            s.plan.as_ref().and_then(|p| serde_json::to_string(p).ok()),
            s.check_job.map(|i| i.to_string()),
            s.apply_job.map(|i| i.to_string()),
            s.applied_at.map(Timestamp::to_rfc3339),
            s.applied_be,
            s.previous_be,
        ],
    )?;
    Ok(())
}

fn job_running(conn: &Connection, id: Option<Id>) -> rusqlite::Result<bool> {
    let Some(id) = id else { return Ok(false) };
    let state: Option<String> = conn
        .query_row(
            "SELECT state FROM jobs WHERE id = ?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(matches!(state.as_deref(), Some("queued" | "running")))
}

/// The wire view: the row plus whether its jobs still run.
pub async fn view(state: &AppState) -> ApiResult<UpdateState> {
    state
        .db
        .call(|conn| {
            let s = load(conn)?;
            let checking = job_running(conn, s.check_job)?;
            let applying = job_running(conn, s.apply_job)?;
            Ok(UpdateState {
                plan: s.plan,
                checking,
                applying,
                check_job: s.check_job,
                apply_job: s.apply_job,
                applied_at: s.applied_at,
                applied_boot_environment: s.applied_be,
                previous_boot_environment: s.previous_be,
            })
        })
        .await
}

/// The boot environment an apply creates: `mandrake-<version>` when the
/// plan moves the Mandrake incorporation, else `mandrake-<current>-<date>`,
/// with `-N` appended while the name is taken.
pub fn be_name(run: &DryRun, current_version: &str, existing: &[String], date: &str) -> String {
    let base = run.mandrake_version().map_or_else(
        || format!("mandrake-{current_version}-{date}"),
        |v| format!("mandrake-{v}"),
    );
    if !existing.contains(&base) {
        return base;
    }
    (2..1000)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !existing.contains(c))
        .unwrap_or(base)
}

fn today() -> String {
    Timestamp::now()
        .to_rfc3339()
        .chars()
        .take(10)
        .filter(|c| *c != '-')
        .collect()
}

fn busy(detail: &str) -> ApiError {
    ApiError::typed(axum::http::StatusCode::CONFLICT, "busy", "Conflict").detail(detail)
}

/// `POST /system/updates/check` as a job.
pub async fn start_check(state: &AppState, actor: &Actor) -> ApiResult<Job> {
    let current = view(state).await?;
    if current.checking {
        return Err(busy("an update check is already running"));
    }
    if current.applying {
        return Err(busy("an update is being applied"));
    }
    let job_state = state.clone();
    let version = state.version.to_owned();
    let job = state
        .start_job(
            "system.update_check",
            Some(host_ref(state)),
            Some(actor),
            move |job| async move {
                job.progress(0.1, "refreshing publishers").await;
                job_state.pkg.refresh().await?;
                job.progress(0.5, "planning").await;
                let run = job_state.pkg.dry_run().await?;
                let existing: Vec<String> = job_state
                    .beadm
                    .list()
                    .await?
                    .into_iter()
                    .map(|b| b.name)
                    .collect();
                let plan = UpdatePlan {
                    packages: run.packages.clone(),
                    reboot_required: run.creates_be || run.rebuilds_boot_archive,
                    boot_environment: be_name(&run, &version, &existing, &today()),
                    mandrake_version: run.mandrake_version(),
                    checked_at: Timestamp::now(),
                    raw: None,
                };
                let count = plan.packages.len();
                job_state
                    .db
                    .call(move |conn| {
                        let mut s = load(conn)?;
                        s.plan = Some(plan);
                        save(conn, &s)
                    })
                    .await?;
                Ok(if count == 0 {
                    "up to date".to_owned()
                } else {
                    format!("{count} package(s) to update")
                })
            },
        )
        .await?;
    let id = job.id;
    state
        .db
        .call(move |conn| {
            let mut s = load(conn)?;
            s.check_job = Some(id);
            save(conn, &s)
        })
        .await?;
    Ok(job)
}

/// `POST /system/updates/apply` as a job.
pub async fn start_apply(state: &AppState, actor: &Actor) -> ApiResult<Job> {
    let current = view(state).await?;
    if current.applying {
        return Err(busy("an update is already being applied"));
    }
    if current.checking {
        return Err(busy("an update check is running; wait for its plan"));
    }
    let Some(plan) = current.plan else {
        return Err(ApiError::unprocessable("no update plan; run a check first"));
    };
    if plan.is_empty() {
        return Err(ApiError::unprocessable(
            "the plan is empty; the host is up to date",
        ));
    }
    let be = plan.boot_environment.clone();
    let job_state = state.clone();
    let job = state
        .start_job(
            "system.update_apply",
            Some(host_ref(state)),
            Some(actor),
            move |job| async move {
                let previous = job_state
                    .beadm
                    .list()
                    .await?
                    .into_iter()
                    .find(|b| b.active)
                    .map(|b| b.name);
                job.progress(0.1, format!("updating into {be}")).await;
                let output = job_state.pkg.update(&be).await?;
                tracing::info!(be = %be, "pkg update finished");
                tracing::debug!(output = %output, "pkg update output");
                let done_be = be.clone();
                job_state
                    .db
                    .call(move |conn| {
                        let mut s = load(conn)?;
                        s.plan = None;
                        s.applied_at = Some(Timestamp::now());
                        s.applied_be = Some(done_be);
                        s.previous_be = previous;
                        save(conn, &s)
                    })
                    .await?;
                job_state
                    .emit(
                        "system.updated",
                        host_ref(&job_state),
                        None,
                        Some(serde_json::json!({ "boot_environment": be })),
                    )
                    .await;
                Ok(format!("updated into {be}; reboot to use it"))
            },
        )
        .await?;
    let id = job.id;
    state
        .db
        .call(move |conn| {
            let mut s = load(conn)?;
            s.apply_job = Some(id);
            save(conn, &s)
        })
        .await?;
    Ok(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> DryRun {
        crate::pkg::parse_dry_run(text).unwrap_or_else(|_| DryRun {
            packages: Vec::new(),
            creates_be: false,
            rebuilds_boot_archive: false,
            raw: String::new(),
        })
    }

    #[test]
    fn names_the_boot_environment() {
        let plan = run(include_str!("../testdata/pkg-update-nv.synthetic.txt"));
        assert_eq!(be_name(&plan, "0.1.0", &[], "20260903"), "mandrake-0.2.0");
        let taken = vec!["mandrake-0.2.0".to_owned(), "mandrake-0.2.0-2".to_owned()];
        assert_eq!(
            be_name(&plan, "0.1.0", &taken, "20260903"),
            "mandrake-0.2.0-3"
        );
        let none = run("No updates available");
        assert_eq!(
            be_name(&none, "0.1.0", &[], "20260903"),
            "mandrake-0.1.0-20260903"
        );
    }
}
