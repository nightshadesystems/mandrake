//! `/system/boot-environments/*` (ADR-0015).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use mandrake_core::{
    Role,
    api::ObjectRef,
    system::{BootEnvironment, BootEnvironmentCreate, BootEnvironmentList},
};
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::Auth,
    error::{ApiError, ApiResult},
};

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        && name.len() <= 63
}

async fn find(state: &AppState, name: &str) -> ApiResult<BootEnvironment> {
    state
        .beadm
        .list()
        .await?
        .into_iter()
        .find(|b| b.name == name)
        .ok_or_else(|| ApiError::not_found("boot environment"))
}

fn be_ref(be: &BootEnvironment) -> ObjectRef {
    ObjectRef::new("boot-environment", be.id, &be.name)
}

fn summary(be: &BootEnvironment) -> serde_json::Value {
    json!({ "name": be.name, "active": be.active, "booted": be.booted })
}

/// `GET /system/boot-environments`.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<BootEnvironmentList>> {
    auth.require(Role::Viewer)?;
    let items = state.beadm.list().await?;
    Ok(Json(BootEnvironmentList { items }))
}

/// `GET /system/boot-environments/{name}`.
pub async fn get_one(
    State(state): State<AppState>,
    auth: Auth,
    Path(name): Path<String>,
) -> ApiResult<Json<BootEnvironment>> {
    auth.require(Role::Viewer)?;
    Ok(Json(find(&state, &name).await?))
}

/// `POST /system/boot-environments`.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<BootEnvironmentCreate>,
) -> ApiResult<(StatusCode, Json<BootEnvironment>)> {
    auth.require(Role::Admin)?;
    if !valid_name(&body.name) {
        return Err(ApiError::unprocessable(
            "name: letters, digits, _ . : - and at most 63 characters",
        ));
    }
    if find(&state, &body.name).await.is_ok() {
        return Err(ApiError::conflict(&format!(
            "boot environment `{}` already exists",
            body.name
        )));
    }
    state.beadm.create(&body.name).await?;
    let be = find(&state, &body.name).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("be.create", be_ref(&be)).after(summary(&be)),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(be)))
}

/// `DELETE /system/boot-environments/{name}`.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    let be = find(&state, &name).await?;
    if be.booted {
        return Err(ApiError::conflict(
            "the running boot environment cannot be destroyed",
        ));
    }
    if be.active {
        return Err(ApiError::conflict(
            "the boot environment that boots next cannot be destroyed; activate another first",
        ));
    }
    state.beadm.destroy(&name).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("be.delete", be_ref(&be)).before(summary(&be)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /system/boot-environments/{name}/activate`.
pub async fn activate(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(name): Path<String>,
) -> ApiResult<Json<BootEnvironment>> {
    auth.require(Role::Admin)?;
    let before = find(&state, &name).await?;
    state.beadm.activate(&name).await?;
    let be = find(&state, &name).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("be.activate", be_ref(&be))
                .before(summary(&before))
                .after(summary(&be)),
        )
        .await?;
    Ok(Json(be))
}
