//! `/zones/*`: native and lx zones (ADR-0012).

use std::{collections::HashMap, net::IpAddr};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use mandrake_core::{
    Id, Role,
    api::{Job, Metadata, ObjectRef, Page},
    image::{ImageState, ImageType},
    network::LinkKind,
    zone::{Zone, ZoneBrand, ZoneCreate, ZoneNic, ZoneState, ZoneStop, ZoneUpdate},
};
use mandrake_zones::{HOSTNAME_ATTR, InstallSource, RESOLVERS_ATTR, ZoneSpec, parse};
use serde::Deserialize;
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::Auth,
    cursor::{self, Pagination},
    error::{ApiError, ApiResult},
    images, metadata, zone_console,
    zones::{self, InstallPlan, Lifecycle},
};

/// The zone routes, mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/zones", get(list_zones).post(create_zone))
        .route(
            "/zones/{id}",
            get(get_zone).patch(update_zone).delete(delete_zone),
        )
        .route("/zones/{id}/start", post(start_zone))
        .route("/zones/{id}/stop", post(stop_zone))
        .route("/zones/{id}/restart", post(restart_zone))
        .route("/zones/{id}/console", get(zone_console::attach))
}

// ------------------------------------------------------------ views

async fn metadata_for(state: &AppState, ids: &[Id]) -> ApiResult<HashMap<Id, Metadata>> {
    let ids = ids.to_vec();
    state
        .db
        .call(move |conn| metadata::get_many(conn, &ids))
        .await
}

async fn zones_view(state: &AppState) -> ApiResult<Vec<Zone>> {
    let infos = zones::all_zones(state).await?;
    let ids = zones::ids_for(state, &infos).await;
    let mut meta = metadata_for(state, &ids).await?;
    Ok(infos
        .iter()
        .zip(ids)
        .filter_map(|(info, id)| zones::to_zone(info, id, meta.remove(&id)))
        .collect())
}

async fn zone_view(state: &AppState, id: Id) -> ApiResult<Zone> {
    let (id, info) = zones::find_zone(state, id).await?;
    let meta = state.db.call(move |conn| metadata::get(conn, id)).await?;
    zones::to_zone(&info, id, meta).ok_or_else(|| ApiError::not_found("zone"))
}

fn busy(detail: impl Into<String>) -> ApiError {
    ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(detail)
}

// ------------------------------------------------------------ validation

fn parse_prefixed(s: &str) -> Option<(IpAddr, u8)> {
    let (ip, prefix) = s.split_once('/')?;
    let ip: IpAddr = ip.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    (prefix <= max).then_some((ip, prefix))
}

fn valid_link_name(name: &str) -> bool {
    let b = name.as_bytes();
    (2..=31).contains(&b.len())
        && b[0].is_ascii_alphabetic()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && b[b.len() - 1].is_ascii_digit()
}

/// NICs must have valid names, sit on existing links of a kind that can
/// carry a VNIC, and carry well-formed addresses.
async fn validate_nics(state: &AppState, nics: &[ZoneNic]) -> ApiResult<()> {
    let net = state.net.clone();
    let links = state
        .links_cache
        .get_or(|| async move { net.list_links().await })
        .await?;
    let mut seen = std::collections::HashSet::new();
    for nic in nics {
        if !valid_link_name(&nic.name) {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}`: invalid link name",
                nic.name
            )));
        }
        if !seen.insert(nic.name.as_str()) {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}` is listed twice",
                nic.name
            )));
        }
        let Some(over) = links.iter().find(|l| l.name == nic.over) else {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}`: `{}` is not a link",
                nic.name, nic.over
            )));
        };
        if !matches!(
            over.kind,
            LinkKind::Phys | LinkKind::Aggr | LinkKind::Etherstub
        ) {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}`: `{}` must be a physical link, aggregation, or etherstub",
                nic.name, nic.over
            )));
        }
        if let Some(vid) = nic.vid
            && !(1..=4094).contains(&vid)
        {
            return Err(ApiError::unprocessable("vid must be between 1 and 4094"));
        }
        if let Some(mac) = &nic.mac
            && mandrake_net::parse::normalize_mac(mac).is_none()
        {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}`: mac must be six colon-separated hex bytes",
                nic.name
            )));
        }
        if let Some(a) = &nic.address
            && parse_prefixed(a).is_none()
        {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}`: address must be `a.b.c.d/prefix` or `xx::/prefix`",
                nic.name
            )));
        }
        if let Some(g) = &nic.gateway
            && g.parse::<IpAddr>().is_err()
        {
            return Err(ApiError::unprocessable(&format!(
                "nic `{}`: gateway must be an IP address",
                nic.name
            )));
        }
    }
    Ok(())
}

fn validate_caps(cpu: Option<f64>, memory: Option<u64>) -> ApiResult<()> {
    if cpu.is_some_and(|c| c <= 0.0 || !c.is_finite()) {
        return Err(ApiError::unprocessable("cpu_cap must be a positive number"));
    }
    if memory.is_some_and(|m| m < 64 << 20) {
        return Err(ApiError::unprocessable(
            "memory_cap_bytes must be at least 64 MiB",
        ));
    }
    Ok(())
}

fn validate_resolvers(resolvers: &[String]) -> ApiResult<()> {
    for r in resolvers {
        if r.parse::<IpAddr>().is_err() {
            return Err(ApiError::unprocessable(&format!(
                "resolver `{r}` is not an IP address"
            )));
        }
    }
    Ok(())
}

// ------------------------------------------------------------ list / get

/// Query for `GET /zones`.
#[derive(Debug, Default, Deserialize)]
pub struct ZoneFilter {
    /// Brand.
    pub brand: Option<ZoneBrand>,
    /// State.
    pub state: Option<ZoneState>,
    /// Paging.
    #[serde(flatten)]
    pub paging: Pagination,
}

/// `GET /zones`.
pub async fn list_zones(
    State(state): State<AppState>,
    auth: Auth,
    Query(filter): Query<ZoneFilter>,
) -> ApiResult<Json<Page<Zone>>> {
    auth.require(Role::Viewer)?;
    let mut items: Vec<Zone> = zones_view(&state)
        .await?
        .into_iter()
        .filter(|z| filter.brand.is_none_or(|b| z.brand == b))
        .filter(|z| filter.state.is_none_or(|s| z.state == s))
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let after = filter.paging.after().unwrap_or_default();
    let limit = filter.paging.limit();
    let rows: Vec<Zone> = items
        .into_iter()
        .filter(|z| z.name > after)
        .take(usize::try_from(limit).unwrap_or(usize::MAX) + 1)
        .collect();
    let (items, next_cursor) = cursor::page(rows, limit, |z| z.name.clone());
    Ok(Json(Page { items, next_cursor }))
}

/// `GET /zones/{id}`.
pub async fn get_zone(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Zone>> {
    auth.require(Role::Viewer)?;
    Ok(Json(zone_view(&state, id).await?))
}

// ------------------------------------------------------------ create

/// The image a zone is cloned from, checked for state and type.
async fn image_for(
    state: &AppState,
    brand: ZoneBrand,
    image_id: Id,
) -> ApiResult<(String, String)> {
    let image = state
        .db
        .call(move |conn| images::get_image(conn, image_id))
        .await?
        .ok_or_else(|| ApiError::not_found("image"))?;
    if image.state != ImageState::Ready {
        return Err(ApiError::unprocessable(&format!(
            "image {}@{} is {}",
            image.name, image.version, image.state
        )));
    }
    let wanted = if brand == ZoneBrand::Lx {
        ImageType::ZoneLx
    } else {
        ImageType::ZoneNative
    };
    if image.type_ != wanted {
        return Err(ApiError::unprocessable(&format!(
            "image {}@{} is {}, a {brand} zone needs {wanted}",
            image.name, image.version, image.type_
        )));
    }
    let (Some(pool), Some(dataset)) = (image.pool, image.dataset) else {
        return Err(ApiError::unprocessable("image has no dataset"));
    };
    Ok((pool, mandrake_images::import::clone_source(&dataset)))
}

/// `POST /zones`.
pub async fn create_zone(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<ZoneCreate>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    if !parse::valid_zone_name(&body.name) {
        return Err(ApiError::unprocessable(
            "invalid zone name; letters, digits, _ . - and not `global`",
        ));
    }
    if body.brand == ZoneBrand::Lx && body.image_id.is_none() {
        return Err(ApiError::unprocessable("an lx zone needs an image_id"));
    }
    validate_nics(&state, &body.nics).await?;
    validate_caps(body.cpu_cap, body.memory_cap_bytes)?;
    validate_resolvers(&body.resolvers)?;
    if zones::all_zones(&state)
        .await?
        .iter()
        .any(|z| z.summary.name == body.name)
    {
        return Err(ApiError::conflict(&format!(
            "zone `{}` already exists",
            body.name
        )));
    }
    // Which pool: the request's, else the image's, else the default.
    let clone_from = match body.image_id {
        Some(image_id) => Some(image_for(&state, body.brand, image_id).await?),
        None => None,
    };
    let pool = match (&body.pool, &clone_from) {
        (Some(p), _) => {
            if !images::pool_exists(&state, p).await? {
                return Err(ApiError::not_found(&format!("pool `{p}`")));
            }
            p.clone()
        }
        (None, Some((image_pool, _))) => image_pool.clone(),
        (None, None) => images::default_pool(&state).await?,
    };
    if let Some((image_pool, _)) = &clone_from
        && image_pool != &pool
    {
        return Err(ApiError::unprocessable(&format!(
            "the image lives in pool `{image_pool}`; a clone cannot cross pools"
        )));
    }
    let id = Id::new();
    let zonepath = zones::zonepath_for(&pool, &body.name);
    let dataset = format!("{pool}/zones/{}", body.name);
    let hostname = body.hostname.clone().unwrap_or_else(|| body.name.clone());
    let spec = ZoneSpec {
        name: body.name.clone(),
        brand: body.brand.as_str().to_owned(),
        zonepath,
        autoboot: body.autoboot,
        nics: body.nics.clone(),
        cpu_cap: body.cpu_cap,
        memory_cap: body.memory_cap_bytes,
        attrs: zones::attrs(id, body.image_id, Some(&hostname), &body.resolvers),
    };
    state.zones.create(&spec).await?;
    zones::invalidate(&state);
    if let Some(m) = &body.metadata
        && !m.is_empty()
    {
        let m = m.clone();
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    let plan = InstallPlan {
        dataset,
        clone_from: clone_from.as_ref().map(|(_, s)| s.clone()),
        source: if clone_from.is_some() {
            InstallSource::Prepared
        } else {
            InstallSource::Packages
        },
        boot: body.start,
    };
    let job = zones::start_install(&state, id, &body.name, plan, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("zone.create", ObjectRef::new("zone", id, &body.name)).after(json!({
                "brand": body.brand,
                "image_id": body.image_id,
                "pool": pool,
                "nics": zones::nics_summary(&body.nics),
                "cpu_cap": body.cpu_cap,
                "memory_cap_bytes": body.memory_cap_bytes,
                "autoboot": body.autoboot,
                "job": job.id,
            })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

// ------------------------------------------------------------ update

/// `PATCH /zones/{id}`.
pub async fn update_zone(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(patch): Json<ZoneUpdate>,
) -> ApiResult<Json<Zone>> {
    auth.require(Role::Operator)?;
    let (id, info) = zones::find_zone(&state, id).await?;
    let mut spec = zones::spec_from(&info.config);
    let mut changed = false;
    if let Some(nics) = &patch.nics {
        validate_nics(&state, nics).await?;
        spec.nics.clone_from(nics);
        changed = true;
    }
    if let Some(cpu) = patch.cpu_cap {
        validate_caps(cpu, None)?;
        spec.cpu_cap = cpu;
        changed = true;
    }
    if let Some(mem) = patch.memory_cap_bytes {
        validate_caps(None, mem)?;
        spec.memory_cap = mem;
        changed = true;
    }
    if let Some(a) = patch.autoboot {
        spec.autoboot = a;
        changed = true;
    }
    if let Some(h) = &patch.hostname {
        if h.trim().is_empty() {
            return Err(ApiError::unprocessable("hostname cannot be empty"));
        }
        spec.attrs
            .insert(HOSTNAME_ATTR.to_owned(), h.trim().to_owned());
        changed = true;
    }
    if let Some(r) = &patch.resolvers {
        validate_resolvers(r)?;
        if r.is_empty() {
            spec.attrs.remove(RESOLVERS_ATTR);
        } else {
            spec.attrs.insert(RESOLVERS_ATTR.to_owned(), r.join(","));
        }
        changed = true;
    }
    if changed {
        state.zones.update(&spec).await?;
        zones::invalidate(&state);
    }
    if let Some(m) = &patch.metadata
        && !m.is_empty()
    {
        let m = m.clone();
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    let zone = zone_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("zone.update", ObjectRef::new("zone", id, &zone.name)).after(json!({
                "nics": zones::nics_summary(&zone.nics),
                "cpu_cap": zone.cpu_cap,
                "memory_cap_bytes": zone.memory_cap_bytes,
                "autoboot": zone.autoboot,
                "hostname": zone.hostname,
                "resolvers": zone.resolvers,
                "metadata": zone.metadata,
            })),
        )
        .await?;
    Ok(Json(zone))
}

// ------------------------------------------------------------ delete

/// Query for `DELETE /zones/{id}`.
#[derive(Debug, Default, Deserialize)]
pub struct DeleteQuery {
    /// Destroy the datasets too.
    #[serde(default)]
    pub purge: bool,
}

/// `DELETE /zones/{id}`.
pub async fn delete_zone(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Query(q): Query<DeleteQuery>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    let (id, info) = zones::find_zone(&state, id).await?;
    if state.console_sessions.contains(&info.summary.name) {
        return Err(busy("a console session is attached; close it first"));
    }
    let job = zones::start_delete(&state, id, &info, q.purge, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "zone.delete",
                ObjectRef::new("zone", id, &info.summary.name),
            )
            .before(json!({
                "brand": info.summary.brand,
                "state": info.summary.state,
                "zonepath": info.summary.zonepath,
                "purge": q.purge,
                "job": job.id,
            })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

// ------------------------------------------------------------ lifecycle

async fn lifecycle(
    state: &AppState,
    auth: &Auth,
    ctx: &crate::audit::Context,
    id: Id,
    op: Lifecycle,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    let (id, info) = zones::find_zone(state, id).await?;
    let current = info.summary.state;
    let running = matches!(current, ZoneState::Running | ZoneState::Ready);
    match op {
        Lifecycle::Start if running => {
            return Err(busy(format!("zone is already {current}")));
        }
        Lifecycle::Start if !matches!(current, ZoneState::Installed | ZoneState::Down) => {
            return Err(busy(format!("zone is {current}; install it first")));
        }
        Lifecycle::Stop { .. } | Lifecycle::Restart if !running => {
            return Err(busy(format!("zone is {current}, not running")));
        }
        _ => {}
    }
    let job = zones::start_lifecycle(state, id, &info.summary.name, op, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            ctx,
            Record::ok(op.kind(), ObjectRef::new("zone", id, &info.summary.name))
                .after(json!({ "from": current, "job": job.id })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// `POST /zones/{id}/start`.
pub async fn start_zone(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Start).await
}

/// `POST /zones/{id}/stop`.
pub async fn stop_zone(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    body: Option<Json<ZoneStop>>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    let force = body.is_some_and(|Json(b)| b.force);
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Stop { force }).await
}

/// `POST /zones/{id}/restart`.
pub async fn restart_zone(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Restart).await
}
