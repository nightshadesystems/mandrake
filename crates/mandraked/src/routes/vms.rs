//! `/vms/*`: bhyve VMs (ADR-0013).

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use mandrake_bhyve::DiskSpec;
use mandrake_core::{
    Id, Role,
    api::{Job, Metadata, ObjectRef, Page},
    image::{ImageState, ImageType},
    vm::{
        Bootrom, Vm, VmCdromAttach, VmCreate, VmDiskAdd, VmDiskResize, VmSnapshot,
        VmSnapshotCreate, VmUpdate,
    },
    zone::{ZoneState, ZoneStop},
};
use mandrake_zones::parse;
use serde::Deserialize;
use serde_json::json;

use super::{Ctx, zones::validate_nics};
use crate::{
    app::AppState,
    audit::Record,
    auth::Auth,
    cursor::{self, Pagination},
    error::{ApiError, ApiResult},
    images, metadata,
    vms::{self, CreatePlan, DiskPlan, FAMILY, VmInfo},
    vnc, zone_console,
    zones::{self, Lifecycle},
};

/// The VM routes, mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vms", get(list_vms).post(create_vm))
        .route("/vms/{id}", get(get_vm).patch(update_vm).delete(delete_vm))
        .route("/vms/{id}/start", post(start_vm))
        .route("/vms/{id}/stop", post(stop_vm))
        .route("/vms/{id}/restart", post(restart_vm))
        .route("/vms/{id}/reset", post(reset_vm))
        .route("/vms/{id}/serial", get(zone_console::attach_vm))
        .route("/vms/{id}/vnc", get(vnc::attach))
        .route("/vms/{id}/disks", post(add_disk))
        .route(
            "/vms/{id}/disks/{index}",
            axum::routing::patch(resize_disk).delete(remove_disk),
        )
        .route("/vms/{id}/cdroms", post(attach_cdrom))
        .route(
            "/vms/{id}/cdroms/{index}",
            axum::routing::delete(detach_cdrom),
        )
        .route(
            "/vms/{id}/snapshots",
            get(list_snapshots).post(create_snapshot),
        )
        .route(
            "/vms/{id}/snapshots/{snapshot}",
            axum::routing::delete(delete_snapshot),
        )
        .route(
            "/vms/{id}/snapshots/{snapshot}/rollback",
            post(rollback_snapshot),
        )
}

fn busy(detail: impl Into<String>) -> ApiError {
    ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(detail)
}

const MIN_MEMORY: u64 = 128 << 20;
const MIN_DISK: u64 = 1 << 20;

// ------------------------------------------------------------ views

async fn metadata_for(state: &AppState, ids: &[Id]) -> ApiResult<HashMap<Id, Metadata>> {
    let ids = ids.to_vec();
    state
        .db
        .call(move |conn| metadata::get_many(conn, &ids))
        .await
}

async fn vm_view(state: &AppState, id: Id) -> ApiResult<Vm> {
    let (id, info) = vms::find_vm(state, id).await?;
    let meta = state.db.call(move |conn| metadata::get(conn, id)).await?;
    vms::to_vm(state, &info, id, meta).await
}

/// The dataset a VM lives on, or 422 when its zonepath is not ours.
fn dataset_of(info: &VmInfo) -> ApiResult<(String, String)> {
    zones::dataset_of(&info.zone.summary.zonepath)
        .ok_or_else(|| ApiError::unprocessable("the VM's zonepath has no dataset"))
}

/// A ready image of `wanted` type: its pool and either the clone source
/// (`dataset@image`) or the ISO path.
async fn image_for(
    state: &AppState,
    image_id: Id,
    wanted: ImageType,
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
    if image.type_ != wanted {
        return Err(ApiError::unprocessable(&format!(
            "image {}@{} is {}, not {wanted}",
            image.name, image.version, image.type_
        )));
    }
    let pool = image.pool.clone().unwrap_or_default();
    match wanted {
        ImageType::VmIso => image
            .path
            .map(|p| (pool, p))
            .ok_or_else(|| ApiError::unprocessable("image has no file")),
        _ => image
            .dataset
            .map(|d| (pool, mandrake_images::import::clone_source(&d)))
            .ok_or_else(|| ApiError::unprocessable("image has no dataset")),
    }
}

// ------------------------------------------------------------ list / get

/// Query for `GET /vms`.
#[derive(Debug, Default, Deserialize)]
pub struct VmFilter {
    /// State.
    pub state: Option<ZoneState>,
    /// Paging.
    #[serde(flatten)]
    pub paging: Pagination,
}

/// `GET /vms`.
pub async fn list_vms(
    State(state): State<AppState>,
    auth: Auth,
    Query(filter): Query<VmFilter>,
) -> ApiResult<Json<Page<Vm>>> {
    auth.require(Role::Viewer)?;
    let infos = vms::all_vms(&state).await?;
    let ids = vms::ids_for(&state, &infos).await;
    let mut meta = metadata_for(&state, &ids).await?;
    let mut items = Vec::with_capacity(infos.len());
    for (info, id) in infos.iter().zip(ids) {
        if filter.state.is_none_or(|s| info.zone.summary.state == s) {
            items.push(vms::to_vm(&state, info, id, meta.remove(&id)).await?);
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let after = filter.paging.after().unwrap_or_default();
    let limit = filter.paging.limit();
    let rows: Vec<Vm> = items
        .into_iter()
        .filter(|v| v.name > after)
        .take(usize::try_from(limit).unwrap_or(usize::MAX) + 1)
        .collect();
    let (items, next_cursor) = cursor::page(rows, limit, |v| v.name.clone());
    Ok(Json(Page { items, next_cursor }))
}

/// `GET /vms/{id}`.
pub async fn get_vm(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Vm>> {
    auth.require(Role::Viewer)?;
    Ok(Json(vm_view(&state, id).await?))
}

// ------------------------------------------------------------ create

fn validate_sizing(vcpus: u32, memory: u64) -> ApiResult<()> {
    if !(1..=128).contains(&vcpus) {
        return Err(ApiError::unprocessable("vcpus must be between 1 and 128"));
    }
    if memory < MIN_MEMORY {
        return Err(ApiError::unprocessable(
            "memory_bytes must be at least 128 MiB",
        ));
    }
    Ok(())
}

/// What a create request resolved to: the pool, which disk boots and
/// from which image, each disk's source, and the ISO paths.
struct Resolved {
    pool: String,
    boot_index: usize,
    boot_image: Option<Id>,
    disk_sources: Vec<(Option<String>, Option<u64>)>,
    cdrom_paths: Vec<String>,
}

async fn resolve_create(state: &AppState, body: &VmCreate) -> ApiResult<Resolved> {
    // Disks: the boot disk is the flagged one, else the first.
    let boot_index = body.disks.iter().position(|d| d.boot).unwrap_or(0);
    let mut image_pool: Option<String> = None;
    let mut boot_image: Option<Id> = None;
    let mut disk_sources: Vec<(Option<String>, Option<u64>)> = Vec::new();
    for (i, d) in body.disks.iter().enumerate() {
        match (d.image_id, d.size_bytes) {
            (Some(image_id), _) => {
                let (pool, source) = image_for(state, image_id, ImageType::VmRaw).await?;
                if let Some(p) = &image_pool
                    && p != &pool
                {
                    return Err(ApiError::unprocessable(
                        "every image-backed disk must come from the same pool",
                    ));
                }
                image_pool = Some(pool);
                if i == boot_index {
                    boot_image = Some(image_id);
                }
                disk_sources.push((Some(source), None));
            }
            (None, Some(size)) if size >= MIN_DISK => disk_sources.push((None, Some(size))),
            (None, _) => {
                return Err(ApiError::unprocessable(&format!(
                    "disk {i} needs an image_id or a size_bytes of at least 1 MiB"
                )));
            }
        }
    }
    let mut cdrom_paths = Vec::new();
    for image_id in &body.cdroms {
        let (_, path) = image_for(state, *image_id, ImageType::VmIso).await?;
        cdrom_paths.push(path);
    }
    let pool = match (&body.pool, &image_pool) {
        (Some(p), _) => {
            if !images::pool_exists(state, p).await? {
                return Err(ApiError::not_found(&format!("pool `{p}`")));
            }
            p.clone()
        }
        (None, Some(p)) => p.clone(),
        (None, None) => images::default_pool(state).await?,
    };
    if let Some(p) = &image_pool
        && p != &pool
    {
        return Err(ApiError::unprocessable(&format!(
            "the image lives in pool `{p}`; a clone cannot cross pools"
        )));
    }
    Ok(Resolved {
        pool,
        boot_index,
        boot_image,
        disk_sources,
        cdrom_paths,
    })
}

/// Merge a metadata patch when one was given.
async fn merge_metadata(state: &AppState, id: Id, patch: Option<&Metadata>) -> ApiResult<()> {
    if let Some(m) = patch
        && !m.is_empty()
    {
        let m = m.clone();
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    Ok(())
}

/// `POST /vms`.
pub async fn create_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<VmCreate>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    if !parse::valid_zone_name(&body.name) {
        return Err(ApiError::unprocessable(
            "invalid VM name; letters, digits, _ . - and not `global`",
        ));
    }
    validate_sizing(body.vcpus, body.memory_bytes)?;
    if body.disks.is_empty() {
        return Err(ApiError::unprocessable("a VM needs at least one disk"));
    }
    if body.disks.iter().filter(|d| d.boot).count() > 1 {
        return Err(ApiError::unprocessable("only one disk can boot"));
    }
    validate_nics(&state, &body.nics).await?;
    if zones::cached_zones(&state)
        .await?
        .iter()
        .any(|z| z.summary.name == body.name)
    {
        return Err(ApiError::conflict(&format!(
            "a zone or VM named `{}` exists",
            body.name
        )));
    }
    let Resolved {
        pool,
        boot_index,
        boot_image,
        disk_sources,
        cdrom_paths,
    } = resolve_create(&state, &body).await?;
    let id = Id::new();
    let dataset = vms::dataset_for(&pool, &body.name);
    let disks: Vec<DiskPlan> = disk_sources
        .into_iter()
        .enumerate()
        .map(|(i, (clone_from, size))| DiskPlan {
            zvol: vms::zvol_for(&dataset, i),
            clone_from,
            size,
        })
        .collect();
    let spec = mandrake_bhyve::VmSpec {
        name: body.name.clone(),
        zonepath: vms::zonepath_for(&pool, &body.name),
        vcpus: body.vcpus,
        memory_bytes: body.memory_bytes,
        bootrom: body.bootrom.unwrap_or(Bootrom::Uefi),
        acpi: body.acpi,
        vnc: body.vnc,
        autoboot: body.autoboot,
        disks: disks
            .iter()
            .enumerate()
            .map(|(i, d)| DiskSpec {
                zvol: d.zvol.clone(),
                boot: i == boot_index,
            })
            .collect(),
        cdroms: cdrom_paths,
        nics: body.nics.clone(),
        attrs: zones::attrs(id, boot_image, None, &[]),
    };
    state
        .zones
        .create(&mandrake_bhyve::to_zone_spec(&spec))
        .await?;
    zones::invalidate(&state);
    merge_metadata(&state, id, body.metadata.as_ref()).await?;
    let plan = CreatePlan {
        dataset,
        disks,
        boot: body.start,
    };
    let job = vms::start_create(&state, id, &body.name, plan, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.create", ObjectRef::new(FAMILY, id, &body.name)).after(json!({
                "vcpus": body.vcpus,
                "memory_bytes": body.memory_bytes,
                "bootrom": spec.bootrom,
                "pool": pool,
                "disks": body.disks.len(),
                "cdroms": body.cdroms.len(),
                "nics": zones::nics_summary(&body.nics),
                "job": job.id,
            })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

// ------------------------------------------------------------ update

/// `PATCH /vms/{id}`.
pub async fn update_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(patch): Json<VmUpdate>,
) -> ApiResult<Json<Vm>> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let mut spec = vms::spec_from(&info);
    let mut changed = false;
    if let Some(v) = patch.vcpus {
        spec.vcpus = v;
        changed = true;
    }
    if let Some(m) = patch.memory_bytes {
        spec.memory_bytes = m;
        changed = true;
    }
    validate_sizing(spec.vcpus, spec.memory_bytes)?;
    if let Some(b) = patch.bootrom {
        spec.bootrom = b;
        changed = true;
    }
    if let Some(a) = patch.acpi {
        spec.acpi = a;
        changed = true;
    }
    if let Some(v) = patch.vnc {
        spec.vnc = v;
        changed = true;
    }
    if let Some(a) = patch.autoboot {
        spec.autoboot = a;
        changed = true;
    }
    if let Some(nics) = &patch.nics {
        validate_nics(&state, nics).await?;
        spec.nics.clone_from(nics);
        changed = true;
    }
    if changed {
        vms::write_spec(&state, &spec).await?;
    }
    merge_metadata(&state, id, patch.metadata.as_ref()).await?;
    let vm = vm_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.update", ObjectRef::new(FAMILY, id, &vm.name)).after(json!({
                "vcpus": vm.vcpus,
                "memory_bytes": vm.memory_bytes,
                "bootrom": vm.bootrom,
                "acpi": vm.acpi,
                "vnc": vm.vnc,
                "autoboot": vm.autoboot,
                "nics": zones::nics_summary(&vm.nics),
                "metadata": vm.metadata,
                "applies": if vm.state == ZoneState::Running { "next boot" } else { "now" },
            })),
        )
        .await?;
    Ok(Json(vm))
}

// ------------------------------------------------------------ delete / lifecycle

/// Query for `DELETE /vms/{id}`.
#[derive(Debug, Default, Deserialize)]
pub struct PurgeQuery {
    /// Destroy datasets too.
    #[serde(default)]
    pub purge: bool,
}

/// `DELETE /vms/{id}`.
pub async fn delete_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Query(q): Query<PurgeQuery>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let name = info.zone.summary.name.clone();
    if state.console_sessions.contains(&name) || state.vnc_sessions.contains(&name) {
        return Err(busy("a console session is attached; close it first"));
    }
    let job = zones::start_delete(&state, FAMILY, id, &info.zone, q.purge, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.delete", ObjectRef::new(FAMILY, id, &name)).before(json!({
                "state": info.zone.summary.state,
                "zonepath": info.zone.summary.zonepath,
                "purge": q.purge,
                "job": job.id,
            })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn lifecycle(
    state: &AppState,
    auth: &Auth,
    ctx: &crate::audit::Context,
    id: Id,
    op: Lifecycle,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(state, id).await?;
    let current = info.zone.summary.state;
    let running = matches!(current, ZoneState::Running | ZoneState::Ready);
    match op {
        Lifecycle::Start if running => return Err(busy(format!("VM is already {current}"))),
        Lifecycle::Start if !matches!(current, ZoneState::Installed | ZoneState::Down) => {
            return Err(busy(format!("VM is {current}")));
        }
        Lifecycle::Stop { .. } | Lifecycle::Restart | Lifecycle::Reset if !running => {
            return Err(busy(format!("VM is {current}, not running")));
        }
        _ => {}
    }
    let name = info.zone.summary.name.clone();
    let job = zones::start_lifecycle(state, FAMILY, id, &name, op, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            ctx,
            Record::ok(&op.kind_for(FAMILY), ObjectRef::new(FAMILY, id, &name))
                .after(json!({ "from": current, "job": job.id })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// `POST /vms/{id}/start`.
pub async fn start_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Start).await
}

/// `POST /vms/{id}/stop`.
pub async fn stop_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    body: Option<Json<ZoneStop>>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    let force = body.is_some_and(|Json(b)| b.force);
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Stop { force }).await
}

/// `POST /vms/{id}/restart`.
pub async fn restart_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Restart).await
}

/// `POST /vms/{id}/reset`.
pub async fn reset_vm(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    lifecycle(&state, &auth, &ctx, id, Lifecycle::Reset).await
}

// ------------------------------------------------------------ disks

/// `POST /vms/{id}/disks`.
pub async fn add_disk(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<VmDiskAdd>,
) -> ApiResult<(StatusCode, Json<Vm>)> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let (pool, dataset) = dataset_of(&info)?;
    let plan = match (body.image_id, body.size_bytes) {
        (Some(image_id), _) => {
            let (image_pool, source) = image_for(&state, image_id, ImageType::VmRaw).await?;
            if image_pool != pool {
                return Err(ApiError::unprocessable(&format!(
                    "the image lives in pool `{image_pool}`; a clone cannot cross pools"
                )));
            }
            (Some(source), None)
        }
        (None, Some(size)) if size >= MIN_DISK => (None, Some(size)),
        (None, _) => {
            return Err(ApiError::unprocessable(
                "give image_id, or size_bytes of at least 1 MiB",
            ));
        }
    };
    let mut spec = vms::spec_from(&info);
    let index = spec.disks.len();
    let zvol = vms::zvol_for(&dataset, index);
    vms::make_disk(
        &state,
        &DiskPlan {
            zvol: zvol.clone(),
            clone_from: plan.0,
            size: plan.1,
        },
    )
    .await?;
    spec.disks.push(DiskSpec {
        zvol: zvol.clone(),
        boot: false,
    });
    vms::write_spec(&state, &spec).await?;
    let vm = vm_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.disk.add", ObjectRef::new(FAMILY, id, &vm.name))
                .after(json!({ "index": index, "zvol": zvol, "image_id": body.image_id, "size_bytes": body.size_bytes })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(vm)))
}

fn disk_at(spec: &mandrake_bhyve::VmSpec, index: usize) -> ApiResult<&DiskSpec> {
    spec.disks
        .get(index)
        .ok_or_else(|| ApiError::not_found(&format!("disk {index}")))
}

/// `PATCH /vms/{id}/disks/{index}`.
pub async fn resize_disk(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path((id, index)): Path<(Id, usize)>,
    Json(body): Json<VmDiskResize>,
) -> ApiResult<Json<Vm>> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let spec = vms::spec_from(&info);
    let disk = disk_at(&spec, index)?.clone();
    let current = vm_view(&state, id)
        .await?
        .disks
        .into_iter()
        .find(|d| d.dataset == disk.zvol)
        .map_or(0, |d| d.size_bytes);
    if body.size_bytes <= current {
        return Err(ApiError::unprocessable(&format!(
            "size_bytes must exceed the current {current}; volumes only grow"
        )));
    }
    state
        .zfs
        .set_properties(
            &disk.zvol,
            &[("volsize".to_owned(), body.size_bytes.to_string())],
        )
        .await?;
    state.datasets_cache.clear();
    let vm = vm_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.disk.resize", ObjectRef::new(FAMILY, id, &vm.name))
                .before(json!({ "index": index, "size_bytes": current }))
                .after(json!({ "index": index, "size_bytes": body.size_bytes })),
        )
        .await?;
    Ok(Json(vm))
}

/// `DELETE /vms/{id}/disks/{index}`.
pub async fn remove_disk(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path((id, index)): Path<(Id, usize)>,
    Query(q): Query<PurgeQuery>,
) -> ApiResult<Json<Vm>> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let mut spec = vms::spec_from(&info);
    let disk = disk_at(&spec, index)?.clone();
    if disk.boot {
        return Err(busy("the boot disk cannot be removed"));
    }
    spec.disks.remove(index);
    vms::write_spec(&state, &spec).await?;
    if q.purge {
        state.zfs.destroy_dataset(&disk.zvol, true).await?;
        state.datasets_cache.clear();
        state.snapshots_cache.clear();
    }
    let vm = vm_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.disk.remove", ObjectRef::new(FAMILY, id, &vm.name))
                .before(json!({ "index": index, "zvol": disk.zvol, "purge": q.purge })),
        )
        .await?;
    Ok(Json(vm))
}

// ------------------------------------------------------------ cdroms

/// `POST /vms/{id}/cdroms`.
pub async fn attach_cdrom(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<VmCdromAttach>,
) -> ApiResult<(StatusCode, Json<Vm>)> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let (_, path) = image_for(&state, body.image_id, ImageType::VmIso).await?;
    let mut spec = vms::spec_from(&info);
    if spec.cdroms.contains(&path) {
        return Err(ApiError::conflict("that ISO is already attached"));
    }
    spec.cdroms.push(path.clone());
    vms::write_spec(&state, &spec).await?;
    let vm = vm_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.cdrom.attach", ObjectRef::new(FAMILY, id, &vm.name))
                .after(json!({ "image_id": body.image_id, "path": path })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(vm)))
}

/// `DELETE /vms/{id}/cdroms/{index}`.
pub async fn detach_cdrom(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path((id, index)): Path<(Id, usize)>,
) -> ApiResult<Json<Vm>> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let mut spec = vms::spec_from(&info);
    if index >= spec.cdroms.len() {
        return Err(ApiError::not_found(&format!("cdrom {index}")));
    }
    let path = spec.cdroms.remove(index);
    vms::write_spec(&state, &spec).await?;
    let vm = vm_view(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.cdrom.detach", ObjectRef::new(FAMILY, id, &vm.name))
                .before(json!({ "index": index, "path": path })),
        )
        .await?;
    Ok(Json(vm))
}

// ------------------------------------------------------------ snapshots

/// `{ "items": [...] }`.
#[derive(Debug, serde::Serialize)]
pub struct Items<T> {
    /// The items.
    pub items: Vec<T>,
}

/// `GET /vms/{id}/snapshots`.
pub async fn list_snapshots(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Items<VmSnapshot>>> {
    auth.require(Role::Viewer)?;
    let (_, info) = vms::find_vm(&state, id).await?;
    let (_, dataset) = dataset_of(&info)?;
    Ok(Json(Items {
        items: vms::snapshots(&state, &dataset).await?,
    }))
}

fn valid_snapshot_name(name: &str) -> bool {
    let b = name.as_bytes();
    (1..=255).contains(&b.len())
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b':' | b'-'))
}

/// `POST /vms/{id}/snapshots`.
pub async fn create_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<VmSnapshotCreate>,
) -> ApiResult<(StatusCode, Json<VmSnapshot>)> {
    auth.require(Role::Operator)?;
    if !valid_snapshot_name(&body.name) {
        return Err(ApiError::unprocessable("invalid snapshot name"));
    }
    let (id, info) = vms::find_vm(&state, id).await?;
    let (_, dataset) = dataset_of(&info)?;
    state
        .zfs
        .create_snapshot(&dataset, &body.name, true)
        .await?;
    state.snapshots_cache.clear();
    let snapshot = vms::snapshots(&state, &dataset)
        .await?
        .into_iter()
        .find(|s| s.name == body.name)
        .ok_or_else(|| ApiError::internal("snapshot vanished after creation"))?;
    let snapshot = match &body.metadata {
        Some(m) if !m.is_empty() => {
            let m = m.clone();
            let sid = snapshot.id;
            let meta = state
                .db
                .call(move |conn| metadata::merge(conn, sid, &m))
                .await?;
            VmSnapshot {
                metadata: Some(meta),
                ..snapshot
            }
        }
        _ => snapshot,
    };
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("vm.snapshot", ObjectRef::new(FAMILY, id, &info.zone.summary.name))
                .after(json!({
                    "snapshot": snapshot.id,
                    "name": snapshot.name,
                    "consistency": if info.zone.summary.state == ZoneState::Running { "crash" } else { "clean" },
                })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(snapshot)))
}

async fn find_snapshot(state: &AppState, dataset: &str, snapshot: Id) -> ApiResult<VmSnapshot> {
    vms::snapshots(state, dataset)
        .await?
        .into_iter()
        .find(|s| s.id == snapshot)
        .ok_or_else(|| ApiError::not_found("snapshot"))
}

/// `DELETE /vms/{id}/snapshots/{snapshot}`.
pub async fn delete_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path((id, snapshot)): Path<(Id, Id)>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let (_, dataset) = dataset_of(&info)?;
    let snap = find_snapshot(&state, &dataset, snapshot).await?;
    state
        .zfs
        .destroy_snapshot_recursive(&format!("{dataset}@{}", snap.name))
        .await?;
    state.snapshots_cache.clear();
    let _ = state
        .db
        .call(move |conn| metadata::remove(conn, snapshot))
        .await;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "vm.snapshot.delete",
                ObjectRef::new(FAMILY, id, &info.zone.summary.name),
            )
            .before(json!({ "snapshot": snapshot, "name": snap.name })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /vms/{id}/snapshots/{snapshot}/rollback`.
pub async fn rollback_snapshot(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path((id, snapshot)): Path<(Id, Id)>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    if matches!(
        info.zone.summary.state,
        ZoneState::Running | ZoneState::Ready | ZoneState::ShuttingDown
    ) {
        return Err(busy("stop the VM before rolling back"));
    }
    let (_, dataset) = dataset_of(&info)?;
    let snap = find_snapshot(&state, &dataset, snapshot).await?;
    let disks: Vec<DiskSpec> = info.config.disks.iter().map(|(_, d)| d.clone()).collect();
    for member in vms::snapshot_members(&dataset, &disks) {
        state
            .zfs
            .rollback(&format!("{member}@{}", snap.name), true)
            .await?;
    }
    state.snapshots_cache.clear();
    state.datasets_cache.clear();
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "vm.snapshot.rollback",
                ObjectRef::new(FAMILY, id, &info.zone.summary.name),
            )
            .after(json!({ "snapshot": snapshot, "name": snap.name })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
