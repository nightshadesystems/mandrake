//! `GET /system` and `GET /system/resources`.

use axum::{Json, extract::State};
use mandrake_core::{
    Timestamp,
    api::{SystemInfo, SystemResources},
};

use crate::{app::AppState, auth::Auth, error::ApiResult, host};

/// Host identity.
pub async fn info(State(state): State<AppState>, _auth: Auth) -> ApiResult<Json<SystemInfo>> {
    let facts = host::facts().await;
    Ok(Json(SystemInfo {
        id: state.host_id,
        hostname: facts.hostname,
        product: "mandrake".to_owned(),
        version: state.version.to_owned(),
        omnios_release: facts.omnios_release,
        boot_environment: facts.boot_environment,
        uptime_seconds: facts.uptime_seconds,
        time: Timestamp::now(),
        timezone: facts.timezone,
    }))
}

/// CPU, load, memory.
pub async fn resources(_auth: Auth) -> ApiResult<Json<SystemResources>> {
    Ok(Json(host::resources().await))
}
