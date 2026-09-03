//! `/storage/*`: devices, pools, datasets, volumes, snapshots (ADR-0011).
//!
//! Illumos is the source of truth; every read goes through the driver
//! (cached briefly) and every object's id comes from its ZFS user
//! property, assigned on first sight. Protection rules and role checks
//! live here, not in the driver.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use mandrake_core::{
    Id, Role,
    api::{Metadata, ObjectRef, Page},
    storage::{
        Dataset, DatasetCreate, DatasetKind, DatasetUpdate, Device, Pool, PoolCreate, PoolDestroy,
        ScanState, Snapshot, SnapshotCreate,
    },
};
use mandrake_zfs::{DatasetInfo, DatasetSpec, ID_PROPERTY, PoolInfo, PoolSpec, SnapshotInfo};
use serde::Deserialize;
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::Auth,
    cursor::{self, Pagination},
    error::{ApiError, ApiResult},
    metadata,
};

/// The storage routes, mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/storage/devices", get(list_devices))
        .route("/storage/pools", get(list_pools).post(create_pool))
        .route(
            "/storage/pools/{id}",
            get(get_pool).patch(update_pool).delete(destroy_pool),
        )
        .route(
            "/storage/pools/{id}/scrub",
            post(start_scrub).delete(stop_scrub),
        )
        .route("/storage/datasets", get(list_datasets).post(create_dataset))
        .route(
            "/storage/datasets/{id}",
            get(get_dataset)
                .patch(update_dataset)
                .delete(destroy_dataset),
        )
        .route("/storage/volumes", get(list_volumes))
        .route(
            "/storage/snapshots",
            get(list_snapshots).post(create_snapshot),
        )
        .route(
            "/storage/snapshots/{id}",
            get(get_snapshot).delete(destroy_snapshot),
        )
        .route("/storage/snapshots/{id}/rollback", post(rollback_snapshot))
        .route("/storage/snapshots/{id}/clone", post(clone_snapshot))
}

// ------------------------------------------------------------ protection

/// Whether a pool refuses destroy and vdev changes (spec §7).
fn pool_protected(name: &str) -> bool {
    name == "rpool"
}

/// Whether a dataset refuses destroy and property changes (ADR-0011).
fn dataset_protected(name: &str) -> bool {
    !name.contains('/')
        || name == "rpool/ROOT"
        || name.starts_with("rpool/ROOT/")
        || name == "rpool/mandrake"
        || name.starts_with("rpool/mandrake/")
        || name == "rpool/dump"
        || name == "rpool/swap"
}

fn protected_error(what: &str) -> ApiError {
    ApiError::typed(StatusCode::FORBIDDEN, "protected", "Forbidden").detail(format!(
        "{what} is protected and cannot be changed through the API"
    ))
}

// ------------------------------------------------------------ identity

/// The id for one object: its stored property when set, else a fresh id
/// stored now. If storing fails the id is derived from the name so reads
/// never fail. Returns whether a store happened.
async fn ensure_id(state: &AppState, kind: &str, name: &str, stored: Option<&str>) -> (Id, bool) {
    if let Some(id) = stored.and_then(|s| s.parse::<Id>().ok()) {
        return (id, false);
    }
    let id = Id::new();
    let props = [(ID_PROPERTY.to_owned(), id.to_string())];
    match state.zfs.set_properties(name, &props).await {
        Ok(()) => (id, true),
        Err(e) => {
            tracing::warn!(%kind, %name, error = %e, "cannot store mandrake-id; using a derived id");
            (Id::derived(state.host_id, kind, name), false)
        }
    }
}

async fn dataset_ids(state: &AppState, infos: &[DatasetInfo]) -> Vec<Id> {
    let mut ids = Vec::with_capacity(infos.len());
    let mut assigned = false;
    for info in infos {
        let (id, stored) =
            ensure_id(state, "dataset", &info.name, info.mandrake_id.as_deref()).await;
        assigned |= stored;
        ids.push(id);
    }
    if assigned {
        state.datasets_cache.clear();
    }
    ids
}

pub(crate) async fn snapshot_ids(state: &AppState, infos: &[SnapshotInfo]) -> Vec<Id> {
    let mut ids = Vec::with_capacity(infos.len());
    let mut assigned = false;
    for info in infos {
        let (id, stored) =
            ensure_id(state, "snapshot", &info.name, info.mandrake_id.as_deref()).await;
        assigned |= stored;
        ids.push(id);
    }
    if assigned {
        state.snapshots_cache.clear();
    }
    ids
}

pub(crate) async fn metadata_for(state: &AppState, ids: &[Id]) -> ApiResult<HashMap<Id, Metadata>> {
    let ids = ids.to_vec();
    state
        .db
        .call(move |conn| metadata::get_many(conn, &ids))
        .await
}

// ------------------------------------------------------------ listings

async fn all_datasets(state: &AppState) -> ApiResult<Vec<(Id, DatasetInfo)>> {
    let zfs = state.zfs.clone();
    let infos = state
        .datasets_cache
        .get_or(|| async move { zfs.list_datasets().await })
        .await?;
    let ids = dataset_ids(state, &infos).await;
    Ok(ids.into_iter().zip(infos).collect())
}

async fn all_pools(state: &AppState) -> ApiResult<Vec<PoolInfo>> {
    let zfs = state.zfs.clone();
    Ok(state
        .pools_cache
        .get_or(|| async move { zfs.list_pools().await })
        .await?)
}

async fn all_snapshots(state: &AppState) -> ApiResult<Vec<(Id, SnapshotInfo)>> {
    let zfs = state.zfs.clone();
    let infos = state
        .snapshots_cache
        .get_or(|| async move { zfs.list_snapshots(None, false).await })
        .await?;
    let ids = snapshot_ids(state, &infos).await;
    Ok(ids.into_iter().zip(infos).collect())
}

fn invalidate(state: &AppState) {
    state.pools_cache.clear();
    state.datasets_cache.clear();
    state.snapshots_cache.clear();
}

fn to_pool(info: PoolInfo, id: Id, metadata: Option<Metadata>) -> Pool {
    Pool {
        id,
        protected: pool_protected(&info.name),
        name: info.name,
        health: info.health,
        size_bytes: info.size,
        allocated_bytes: info.allocated,
        free_bytes: info.free,
        fragmentation_percent: info.fragmentation,
        capacity_percent: info.capacity,
        dedup_ratio: info.dedup_ratio,
        vdevs: info.vdevs,
        scan: info.scan,
        status_text: info.status_text,
        metadata,
    }
}

fn to_dataset(info: DatasetInfo, id: Id, metadata: Option<Metadata>) -> Dataset {
    Dataset {
        id,
        pool: info.pool().to_owned(),
        protected: dataset_protected(&info.name),
        name: info.name,
        kind: info.kind,
        mountpoint: info.mountpoint,
        mounted: info.mounted,
        used_bytes: info.used,
        available_bytes: info.available,
        referenced_bytes: info.referenced,
        logical_used_bytes: info.logical_used,
        quota_bytes: info.quota,
        reservation_bytes: info.reservation,
        compression: info.compression,
        compress_ratio: info.compress_ratio,
        atime: info.atime,
        recordsize_bytes: info.recordsize,
        volsize_bytes: info.volsize,
        volblocksize_bytes: info.volblocksize,
        origin: info.origin,
        created_at: info.created_at,
        metadata,
    }
}

fn to_snapshot(info: SnapshotInfo, id: Id, metadata: Option<Metadata>) -> Snapshot {
    Snapshot {
        id,
        dataset: info.dataset().to_owned(),
        short_name: info.short_name().to_owned(),
        name: info.name,
        used_bytes: info.used,
        referenced_bytes: info.referenced,
        clones: info.clones,
        created_at: info.created_at,
        metadata,
    }
}

/// Pools with ids: a pool's id is its root dataset's.
async fn pools_with_ids(state: &AppState) -> ApiResult<Vec<(Id, PoolInfo)>> {
    let pools = all_pools(state).await?;
    let datasets = all_datasets(state).await?;
    let mut out = Vec::with_capacity(pools.len());
    for p in pools {
        let id = datasets.iter().find(|(_, d)| d.name == p.name).map_or_else(
            || Id::derived(state.host_id, "pool", &p.name),
            |(id, _)| *id,
        );
        out.push((id, p));
    }
    Ok(out)
}

async fn find_pool(state: &AppState, id: Id) -> ApiResult<(Id, PoolInfo)> {
    pools_with_ids(state)
        .await?
        .into_iter()
        .find(|(pid, _)| *pid == id)
        .ok_or_else(|| ApiError::not_found("pool"))
}

async fn find_dataset(state: &AppState, id: Id) -> ApiResult<(Id, DatasetInfo)> {
    all_datasets(state)
        .await?
        .into_iter()
        .find(|(did, _)| *did == id)
        .ok_or_else(|| ApiError::not_found("dataset"))
}

async fn find_snapshot(state: &AppState, id: Id) -> ApiResult<(Id, SnapshotInfo)> {
    all_snapshots(state)
        .await?
        .into_iter()
        .find(|(sid, _)| *sid == id)
        .ok_or_else(|| ApiError::not_found("snapshot"))
}

/// Page a name-sorted list with a name cursor.
fn page_by_name<T>(items: Vec<T>, p: &Pagination, name: impl Fn(&T) -> &str) -> Page<T> {
    let after = p.after().unwrap_or_default();
    let rows: Vec<T> = items
        .into_iter()
        .filter(|i| name(i) > after.as_str())
        .collect();
    let limit = p.limit();
    let rows: Vec<T> = rows
        .into_iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX) + 1)
        .collect();
    let (items, next_cursor) = cursor::page(rows, limit, |i| name(i).to_owned());
    Page { items, next_cursor }
}

fn summary_dataset(d: &Dataset) -> serde_json::Value {
    json!({
        "name": d.name,
        "kind": d.kind,
        "quota_bytes": d.quota_bytes,
        "reservation_bytes": d.reservation_bytes,
        "compression": d.compression,
        "volsize_bytes": d.volsize_bytes,
        "mountpoint": d.mountpoint,
    })
}

// ------------------------------------------------------------ devices

/// `GET /storage/devices`.
pub async fn list_devices(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<DeviceListBody>> {
    auth.require(Role::Viewer)?;
    let pools = all_pools(&state).await?;
    let devices = state.zfs.list_devices().await?;
    let items = devices
        .into_iter()
        .map(|d| {
            let pool = pools
                .iter()
                .find(|p| {
                    p.vdevs.leaves().iter().any(|leaf| {
                        leaf.trim_end_matches(|c: char| c.is_ascii_digit())
                            .trim_end_matches('s')
                            == d.name
                            || *leaf == d.name
                    })
                })
                .map(|p| p.name.clone());
            Device {
                name: d.name,
                vendor: d.vendor,
                product: d.product,
                serial: d.serial,
                size_bytes: d.size,
                removable: d.removable,
                solid_state: d.solid_state,
                pool,
            }
        })
        .collect();
    Ok(Json(DeviceListBody { items }))
}

/// `GET /storage/devices` body.
#[derive(Debug, serde::Serialize)]
pub struct DeviceListBody {
    /// Devices.
    pub items: Vec<Device>,
}

// ------------------------------------------------------------ pools

/// `GET /storage/pools`.
pub async fn list_pools(
    State(state): State<AppState>,
    auth: Auth,
    Query(p): Query<Pagination>,
) -> ApiResult<Json<Page<Pool>>> {
    auth.require(Role::Viewer)?;
    let pools = pools_with_ids(&state).await?;
    let ids: Vec<Id> = pools.iter().map(|(id, _)| *id).collect();
    let mut meta = metadata_for(&state, &ids).await?;
    let mut items: Vec<Pool> = pools
        .into_iter()
        .map(|(id, info)| to_pool(info, id, meta.remove(&id)))
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(page_by_name(items, &p, |x| &x.name)))
}

/// `GET /storage/pools/{id}`.
pub async fn get_pool(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Pool>> {
    auth.require(Role::Viewer)?;
    let (id, info) = find_pool(&state, id).await?;
    let meta = state.db.call(move |conn| metadata::get(conn, id)).await?;
    Ok(Json(to_pool(info, id, meta)))
}

fn valid_pool_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.len() <= 255
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        && !matches!(
            name,
            "mirror" | "raidz" | "raidz1" | "raidz2" | "raidz3" | "spare" | "log" | "cache"
        )
}

/// `POST /storage/pools`.
pub async fn create_pool(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<PoolCreate>,
) -> ApiResult<(StatusCode, Json<Pool>)> {
    auth.require(Role::Operator)?;
    if !valid_pool_name(&body.name) {
        return Err(ApiError::unprocessable("invalid pool name"));
    }
    if body.vdevs.is_empty() || body.vdevs.iter().any(|v| v.devices.is_empty()) {
        return Err(ApiError::unprocessable(
            "at least one vdev with at least one device is required",
        ));
    }
    if body.name == "rpool" || all_pools(&state).await?.iter().any(|p| p.name == body.name) {
        return Err(ApiError::conflict(&format!(
            "pool `{}` already exists",
            body.name
        )));
    }
    let id = Id::new();
    let spec = PoolSpec {
        name: body.name.clone(),
        vdevs: body.vdevs.clone(),
        ashift: body.ashift,
        compression: body.compression.clone(),
        force: body.force,
        root_properties: vec![(ID_PROPERTY.to_owned(), id.to_string())],
    };
    state.zfs.create_pool(&spec).await?;
    invalidate(&state);
    let meta = match &body.metadata {
        Some(m) if !m.is_empty() => {
            let m = m.clone();
            Some(
                state
                    .db
                    .call(move |conn| metadata::merge(conn, id, &m))
                    .await?,
            )
        }
        _ => None,
    };
    let info = state.zfs.pool(&body.name).await?;
    let pool = to_pool(info, id, meta);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("pool.create", ObjectRef::new("pool", id, &pool.name)).after(json!({
                "name": pool.name,
                "vdevs": body.vdevs,
                "size_bytes": pool.size_bytes,
            })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(pool)))
}

/// `PATCH /storage/pools/{id}`: metadata only.
pub async fn update_pool(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(patch): Json<Metadata>,
) -> ApiResult<Json<Pool>> {
    auth.require(Role::Operator)?;
    let (id, info) = find_pool(&state, id).await?;
    let patch_copy = patch.clone();
    let meta = state
        .db
        .call(move |conn| metadata::merge(conn, id, &patch_copy))
        .await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("pool.update", ObjectRef::new("pool", id, &info.name))
                .after(serde_json::to_value(&meta).unwrap_or_default()),
        )
        .await?;
    Ok(Json(to_pool(info, id, Some(meta))))
}

/// `DELETE /storage/pools/{id}`.
pub async fn destroy_pool(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<PoolDestroy>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Admin)?;
    let (id, info) = find_pool(&state, id).await?;
    if pool_protected(&info.name) {
        return Err(protected_error(&format!("pool `{}`", info.name)));
    }
    if body.name != info.name {
        return Err(ApiError::unprocessable(
            "the request body must echo the pool name",
        ));
    }
    state.zfs.destroy_pool(&info.name).await?;
    invalidate(&state);
    let _ = state.db.call(move |conn| metadata::remove(conn, id)).await;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("pool.destroy", ObjectRef::new("pool", id, &info.name)).before(json!({
                "name": info.name,
                "size_bytes": info.size,
            })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /storage/pools/{id}/scrub`.
pub async fn start_scrub(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<(StatusCode, Json<mandrake_core::api::Job>)> {
    auth.require(Role::Operator)?;
    let (id, info) = find_pool(&state, id).await?;
    if info
        .scan
        .as_ref()
        .is_some_and(|s| s.state == ScanState::InProgress)
    {
        return Err(ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict")
            .detail("a scrub or resilver is already running"));
    }
    state.zfs.scrub(&info.name, false).await?;
    invalidate(&state);
    let name = info.name.clone();
    let target = ObjectRef::new("pool", id, &name);
    state
        .record(&auth.actor, &ctx, Record::ok("pool.scrub", target.clone()))
        .await?;
    let interval = state.scan_poll;
    let job_state = state.clone();
    let job = state
        .start_job(
            "pool.scrub",
            Some(target),
            Some(&auth.actor),
            move |job| async move {
                loop {
                    tokio::time::sleep(interval).await;
                    let pool = job_state.zfs.pool(&name).await?;
                    let Some(scan) = pool.scan else {
                        return Ok("scrub finished".to_owned());
                    };
                    match scan.state {
                        ScanState::InProgress => {
                            job.progress(
                                scan.progress.unwrap_or(0.0),
                                scan.summary.lines().next().unwrap_or("").to_owned(),
                            )
                            .await;
                        }
                        ScanState::Finished => {
                            job_state.pools_cache.clear();
                            return Ok(scan
                                .summary
                                .lines()
                                .next()
                                .unwrap_or("scrub finished")
                                .to_owned());
                        }
                        ScanState::Canceled => {
                            job_state.pools_cache.clear();
                            return Err(ApiError::typed(
                                StatusCode::CONFLICT,
                                "canceled",
                                "Conflict",
                            )
                            .detail("scrub was canceled"));
                        }
                    }
                }
            },
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// `DELETE /storage/pools/{id}/scrub`.
pub async fn stop_scrub(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let (id, info) = find_pool(&state, id).await?;
    state.zfs.scrub(&info.name, true).await?;
    invalidate(&state);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("pool.scrub-stop", ObjectRef::new("pool", id, &info.name)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ datasets

/// `GET /storage/datasets` query.
#[derive(Debug, Default, Deserialize)]
pub struct DatasetQuery {
    /// Only this pool.
    pub pool: Option<String>,
    /// Only direct children of this dataset.
    pub parent: Option<String>,
    /// Only this kind.
    pub kind: Option<DatasetKind>,
    /// Cursor.
    pub cursor: Option<String>,
    /// Page size.
    pub limit: Option<u32>,
}

async fn datasets_page(
    state: &AppState,
    q: DatasetQuery,
    force_kind: Option<DatasetKind>,
) -> ApiResult<Page<Dataset>> {
    let kind = force_kind.or(q.kind);
    let all = all_datasets(state).await?;
    let selected: Vec<(Id, DatasetInfo)> = all
        .into_iter()
        .filter(|(_, d)| kind.is_none_or(|k| d.kind == k))
        .filter(|(_, d)| q.pool.as_deref().is_none_or(|p| d.pool() == p))
        .filter(|(_, d)| {
            q.parent.as_deref().is_none_or(|parent| {
                d.name
                    .strip_prefix(parent)
                    .and_then(|rest| rest.strip_prefix('/'))
                    .is_some_and(|rest| !rest.contains('/'))
            })
        })
        .collect();
    let ids: Vec<Id> = selected.iter().map(|(id, _)| *id).collect();
    let mut meta = metadata_for(state, &ids).await?;
    let items: Vec<Dataset> = selected
        .into_iter()
        .map(|(id, info)| to_dataset(info, id, meta.remove(&id)))
        .collect();
    let p = Pagination {
        cursor: q.cursor,
        limit: q.limit,
    };
    Ok(page_by_name(items, &p, |d| &d.name))
}

/// `GET /storage/datasets`.
pub async fn list_datasets(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<DatasetQuery>,
) -> ApiResult<Json<Page<Dataset>>> {
    auth.require(Role::Viewer)?;
    Ok(Json(datasets_page(&state, q, None).await?))
}

/// `GET /storage/volumes`.
pub async fn list_volumes(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<DatasetQuery>,
) -> ApiResult<Json<Page<Dataset>>> {
    auth.require(Role::Viewer)?;
    Ok(Json(
        datasets_page(&state, q, Some(DatasetKind::Volume)).await?,
    ))
}

/// `GET /storage/datasets/{id}`.
pub async fn get_dataset(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Dataset>> {
    auth.require(Role::Viewer)?;
    let (id, info) = find_dataset(&state, id).await?;
    let meta = state.db.call(move |conn| metadata::get(conn, id)).await?;
    Ok(Json(to_dataset(info, id, meta)))
}

fn valid_dataset_name(name: &str) -> bool {
    name.contains('/')
        && !name.ends_with('/')
        && !name.contains("//")
        && !name.contains('@')
        && name.len() <= 255
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-' | '/'))
}

fn property_args(body: &DatasetCreate) -> Vec<(String, String)> {
    let mut props = Vec::new();
    if let Some(c) = &body.compression {
        props.push(("compression".to_owned(), c.clone()));
    }
    match body.kind {
        DatasetKind::Filesystem => {
            if let Some(q) = body.quota_bytes {
                props.push(("quota".to_owned(), q.to_string()));
            }
            if let Some(r) = body.reservation_bytes {
                props.push(("reservation".to_owned(), r.to_string()));
            }
            if let Some(m) = &body.mountpoint {
                props.push(("mountpoint".to_owned(), m.clone()));
            }
            if let Some(a) = body.atime {
                props.push(("atime".to_owned(), if a { "on" } else { "off" }.to_owned()));
            }
            if let Some(r) = body.recordsize_bytes {
                props.push(("recordsize".to_owned(), r.to_string()));
            }
        }
        DatasetKind::Volume => {
            if let Some(b) = body.volblocksize_bytes {
                props.push(("volblocksize".to_owned(), b.to_string()));
            }
            if let Some(r) = body.reservation_bytes {
                props.push(("refreservation".to_owned(), r.to_string()));
            }
        }
    }
    props
}

/// `POST /storage/datasets`.
pub async fn create_dataset(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<DatasetCreate>,
) -> ApiResult<(StatusCode, Json<Dataset>)> {
    auth.require(Role::Operator)?;
    if !valid_dataset_name(&body.name) {
        return Err(ApiError::unprocessable(
            "invalid dataset name; use pool/path with no `@`",
        ));
    }
    if body.kind == DatasetKind::Volume && body.volsize_bytes.is_none_or(|s| s == 0) {
        return Err(ApiError::unprocessable("volumes need a volsize_bytes"));
    }
    let pool = body.name.split('/').next().unwrap_or_default().to_owned();
    if !all_pools(&state).await?.iter().any(|p| p.name == pool) {
        return Err(ApiError::not_found(&format!("pool `{pool}`")));
    }
    if dataset_protected(&body.name) {
        return Err(protected_error(&format!("dataset `{}`", body.name)));
    }
    let id = Id::new();
    let mut props = property_args(&body);
    props.push((ID_PROPERTY.to_owned(), id.to_string()));
    let spec = DatasetSpec {
        name: body.name.clone(),
        kind: body.kind,
        volsize: body.volsize_bytes,
        sparse: body.sparse,
        create_parents: body.create_parents,
        properties: props,
    };
    state.zfs.create_dataset(&spec).await?;
    invalidate(&state);
    let meta = match &body.metadata {
        Some(m) if !m.is_empty() => {
            let m = m.clone();
            Some(
                state
                    .db
                    .call(move |conn| metadata::merge(conn, id, &m))
                    .await?,
            )
        }
        _ => None,
    };
    let info = state.zfs.dataset(&body.name).await?;
    let dataset = to_dataset(info, id, meta);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "dataset.create",
                ObjectRef::new("dataset", id, &dataset.name),
            )
            .after(summary_dataset(&dataset)),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(dataset)))
}

/// `PATCH /storage/datasets/{id}`.
pub async fn update_dataset(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<DatasetUpdate>,
) -> ApiResult<Json<Dataset>> {
    auth.require(Role::Operator)?;
    let (id, info) = find_dataset(&state, id).await?;
    let before_meta = state.db.call(move |conn| metadata::get(conn, id)).await?;
    let before = to_dataset(info.clone(), id, before_meta);

    let mut props: Vec<(String, String)> = Vec::new();
    if let Some(v) = body.volsize_bytes {
        if info.kind != DatasetKind::Volume {
            return Err(ApiError::unprocessable(
                "volsize_bytes applies to volumes only",
            ));
        }
        if info.volsize.is_some_and(|old| v < old) {
            return Err(ApiError::unprocessable("volumes can only grow"));
        }
        props.push(("volsize".to_owned(), v.to_string()));
    }
    if let Some(c) = &body.compression {
        props.push(("compression".to_owned(), c.clone()));
    }
    if let Some(q) = body.quota_bytes {
        props.push((
            "quota".to_owned(),
            q.map_or_else(|| "none".to_owned(), |v| v.to_string()),
        ));
    }
    if let Some(r) = body.reservation_bytes {
        props.push((
            "reservation".to_owned(),
            r.map_or_else(|| "none".to_owned(), |v| v.to_string()),
        ));
    }
    if let Some(m) = &body.mountpoint {
        props.push(("mountpoint".to_owned(), m.clone()));
    }
    if let Some(a) = body.atime {
        props.push(("atime".to_owned(), if a { "on" } else { "off" }.to_owned()));
    }
    if !props.is_empty() && dataset_protected(&info.name) {
        return Err(protected_error(&format!("dataset `{}`", info.name)));
    }
    if !props.is_empty() {
        state.zfs.set_properties(&info.name, &props).await?;
        invalidate(&state);
    }
    let meta = match &body.metadata {
        Some(m) if !m.is_empty() => {
            let m = m.clone();
            Some(
                state
                    .db
                    .call(move |conn| metadata::merge(conn, id, &m))
                    .await?,
            )
        }
        _ => before.metadata.clone(),
    };
    let info = state.zfs.dataset(&info.name).await?;
    let after = to_dataset(info, id, meta);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("dataset.update", ObjectRef::new("dataset", id, &after.name))
                .before(summary_dataset(&before))
                .after(summary_dataset(&after)),
        )
        .await?;
    Ok(Json(after))
}

/// `DELETE /storage/datasets/{id}` query.
#[derive(Debug, Default, Deserialize)]
pub struct DestroyQuery {
    /// `zfs destroy -r`.
    #[serde(default)]
    pub recursive: bool,
}

/// `DELETE /storage/datasets/{id}`.
pub async fn destroy_dataset(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Query(q): Query<DestroyQuery>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let (id, info) = find_dataset(&state, id).await?;
    if dataset_protected(&info.name) {
        return Err(protected_error(&format!("dataset `{}`", info.name)));
    }
    let children: Vec<Id> = all_datasets(&state)
        .await?
        .into_iter()
        .filter(|(_, d)| d.name.starts_with(&format!("{}/", info.name)))
        .map(|(cid, _)| cid)
        .collect();
    if !children.is_empty() && !q.recursive {
        return Err(
            ApiError::typed(StatusCode::CONFLICT, "has-children", "Conflict")
                .detail("dataset has children; pass recursive=true to destroy them too"),
        );
    }
    state.zfs.destroy_dataset(&info.name, q.recursive).await?;
    invalidate(&state);
    let _ = state
        .db
        .call(move |conn| {
            metadata::remove(conn, id)?;
            for c in children {
                metadata::remove(conn, c)?;
            }
            Ok(())
        })
        .await;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("dataset.destroy", ObjectRef::new("dataset", id, &info.name)).before(
                json!({
                    "name": info.name,
                    "kind": info.kind,
                    "recursive": q.recursive,
                }),
            ),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ snapshots

/// `GET /storage/snapshots` query.
#[derive(Debug, Default, Deserialize)]
pub struct SnapshotQuery {
    /// Only this dataset.
    pub dataset: Option<String>,
    /// With `dataset`: descendants too.
    #[serde(default)]
    pub recursive: bool,
    /// Cursor.
    pub cursor: Option<String>,
    /// Page size.
    pub limit: Option<u32>,
}

/// `GET /storage/snapshots`.
pub async fn list_snapshots(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<SnapshotQuery>,
) -> ApiResult<Json<Page<Snapshot>>> {
    auth.require(Role::Viewer)?;
    let selected: Vec<(Id, SnapshotInfo)> = all_snapshots(&state)
        .await?
        .into_iter()
        .filter(|(_, s)| match q.dataset.as_deref() {
            None => true,
            Some(ds) if q.recursive => {
                s.dataset() == ds || s.dataset().starts_with(&format!("{ds}/"))
            }
            Some(ds) => s.dataset() == ds,
        })
        .collect();
    let ids: Vec<Id> = selected.iter().map(|(id, _)| *id).collect();
    let mut meta = metadata_for(&state, &ids).await?;
    let mut items: Vec<Snapshot> = selected
        .into_iter()
        .map(|(id, info)| to_snapshot(info, id, meta.remove(&id)))
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let p = Pagination {
        cursor: q.cursor,
        limit: q.limit,
    };
    Ok(Json(page_by_name(items, &p, |s| &s.name)))
}

/// `GET /storage/snapshots/{id}`.
pub async fn get_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Snapshot>> {
    auth.require(Role::Viewer)?;
    let (id, info) = find_snapshot(&state, id).await?;
    let meta = state.db.call(move |conn| metadata::get(conn, id)).await?;
    Ok(Json(to_snapshot(info, id, meta)))
}

fn valid_snapshot_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

/// `POST /storage/snapshots`.
pub async fn create_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<SnapshotCreate>,
) -> ApiResult<(StatusCode, Json<Snapshot>)> {
    auth.require(Role::Operator)?;
    if !valid_snapshot_name(&body.name) {
        return Err(ApiError::unprocessable("invalid snapshot name"));
    }
    if !all_datasets(&state)
        .await?
        .iter()
        .any(|(_, d)| d.name == body.dataset)
    {
        return Err(ApiError::not_found(&format!("dataset `{}`", body.dataset)));
    }
    state
        .zfs
        .create_snapshot(&body.dataset, &body.name, body.recursive)
        .await?;
    invalidate(&state);
    let full = format!("{}@{}", body.dataset, body.name);
    let id = Id::new();
    state
        .zfs
        .set_properties(&full, &[(ID_PROPERTY.to_owned(), id.to_string())])
        .await?;
    let meta = match &body.metadata {
        Some(m) if !m.is_empty() => {
            let m = m.clone();
            Some(
                state
                    .db
                    .call(move |conn| metadata::merge(conn, id, &m))
                    .await?,
            )
        }
        _ => None,
    };
    let info = state.zfs.snapshot(&full).await?;
    let snap = to_snapshot(info, id, meta);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "snapshot.create",
                ObjectRef::new("snapshot", id, &snap.name),
            )
            .after(json!({
                "name": snap.name,
                "recursive": body.recursive,
            })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(snap)))
}

/// `DELETE /storage/snapshots/{id}`.
pub async fn destroy_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let (id, info) = find_snapshot(&state, id).await?;
    if dataset_protected(info.dataset()) && info.dataset().starts_with("rpool/ROOT") {
        return Err(protected_error(&format!("snapshot `{}`", info.name)));
    }
    state.zfs.destroy_snapshot(&info.name).await?;
    invalidate(&state);
    let _ = state.db.call(move |conn| metadata::remove(conn, id)).await;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "snapshot.destroy",
                ObjectRef::new("snapshot", id, &info.name),
            ),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /storage/snapshots/{id}/rollback` body.
#[derive(Debug, Default, Deserialize)]
pub struct RollbackBody {
    /// `zfs rollback -r`.
    #[serde(default)]
    pub discard_newer: bool,
}

/// `POST /storage/snapshots/{id}/rollback`.
pub async fn rollback_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    body: Option<Json<RollbackBody>>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let discard_newer = body.is_some_and(|Json(b)| b.discard_newer);
    let (id, info) = find_snapshot(&state, id).await?;
    if dataset_protected(info.dataset()) {
        return Err(protected_error(&format!("dataset `{}`", info.dataset())));
    }
    state.zfs.rollback(&info.name, discard_newer).await?;
    invalidate(&state);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "snapshot.rollback",
                ObjectRef::new("snapshot", id, &info.name),
            )
            .after(json!({
                "dataset": info.dataset(),
                "discard_newer": discard_newer,
            })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /storage/snapshots/{id}/clone` body.
#[derive(Debug, Deserialize)]
pub struct CloneBody {
    /// Full name of the new dataset.
    pub name: String,
}

/// `POST /storage/snapshots/{id}/clone`.
pub async fn clone_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<CloneBody>,
) -> ApiResult<(StatusCode, Json<Dataset>)> {
    auth.require(Role::Operator)?;
    if !valid_dataset_name(&body.name) {
        return Err(ApiError::unprocessable("invalid dataset name"));
    }
    if dataset_protected(&body.name) {
        return Err(protected_error(&format!("dataset `{}`", body.name)));
    }
    let (snap_id, info) = find_snapshot(&state, id).await?;
    state.zfs.clone_snapshot(&info.name, &body.name).await?;
    invalidate(&state);
    let new_id = Id::new();
    state
        .zfs
        .set_properties(&body.name, &[(ID_PROPERTY.to_owned(), new_id.to_string())])
        .await?;
    let created = state.zfs.dataset(&body.name).await?;
    let dataset = to_dataset(created, new_id, None);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "snapshot.clone",
                ObjectRef::new("snapshot", snap_id, &info.name),
            )
            .after(json!({
                "clone": dataset.name,
                "clone_id": new_id,
            })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(dataset)))
}
