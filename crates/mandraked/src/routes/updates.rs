//! `/system/updates*` and `/system/reboot` (ADR-0015).

use std::time::Duration;

use axum::{Json, extract::State, http::StatusCode};
use mandrake_core::{Role, api::Job, shell::Command, system::UpdateState};
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::Auth,
    error::{ApiError, ApiResult},
    updates,
};

/// How long the reboot job waits so the response and audit reach clients.
pub const REBOOT_GRACE: Duration = Duration::from_secs(2);

/// `GET /system/updates`.
pub async fn get(State(state): State<AppState>, auth: Auth) -> ApiResult<Json<UpdateState>> {
    auth.require(Role::Viewer)?;
    Ok(Json(updates::view(&state).await?))
}

/// `POST /system/updates/check`.
pub async fn check(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Admin)?;
    let job = updates::start_check(&state, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("system.update_check", updates::host_ref(&state))
                .after(json!({ "job": job.id })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// `POST /system/updates/apply`.
pub async fn apply(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Admin)?;
    let job = updates::start_apply(&state, &auth.actor).await?;
    let be = updates::view(&state)
        .await?
        .plan
        .map(|p| p.boot_environment)
        .unwrap_or_default();
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("system.update_apply", updates::host_ref(&state))
                .after(json!({ "job": job.id, "boot_environment": be })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// `POST /system/reboot`.
pub async fn reboot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Admin)?;
    let pending: i64 = state
        .db
        .call(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'system.reboot' AND state IN ('queued', 'running')",
                [],
                |r| r.get(0),
            )
        })
        .await?;
    if pending > 0 {
        return Err(ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict")
            .detail("a reboot is already scheduled"));
    }
    // Audit first: once shutdown runs there may be no time left to write.
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("system.reboot", updates::host_ref(&state)),
        )
        .await?;
    let runner = state.runner.clone();
    let grace = state.reboot_grace;
    let job = state
        .start_job(
            "system.reboot",
            Some(updates::host_ref(&state)),
            Some(&auth.actor),
            move |job| async move {
                job.progress(0.5, "rebooting shortly").await;
                tokio::time::sleep(grace).await;
                runner
                    .run(
                        &Command::new("shutdown")
                            .args(["-y", "-g", "0", "-i", "6"])
                            .privileged(),
                    )
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?;
                Ok("reboot requested".to_owned())
            },
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}
