//! Zones in the daemon (ADR-0012): identity through the `mandrake-id`
//! attribute, the view joined from `zoneadm list` and `zonecfg export`,
//! and the jobs that install, boot, stop, and remove zones. VMs are
//! bhyve-brand zones and share the cache, the ids, and the lifecycle
//! jobs; `family` names which API the object belongs to.

use std::{collections::BTreeMap, time::Duration};

use axum::http::StatusCode;
use mandrake_core::{
    Actor, Id,
    api::{Job, Metadata, ObjectRef},
    storage::DatasetKind,
    zone::{Zone, ZoneBrand, ZoneNic, ZoneState},
};
use mandrake_zfs::DatasetSpec;
use mandrake_zones::{
    HOSTNAME_ATTR, ID_ATTR, IMAGE_ATTR, InstallSource, RESOLVERS_ATTR, ZoneConfig, ZoneSpec,
    ZoneSummary,
};
use serde_json::json;

use crate::{
    app::AppState,
    error::{ApiError, ApiResult},
};

/// How long a lifecycle job waits for `zoneadm` to reach the state.
const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(600);

/// A zone as the drivers see it: the list row and the configuration.
#[derive(Debug, Clone)]
pub struct ZoneInfo {
    /// From `zoneadm list`.
    pub summary: ZoneSummary,
    /// From `zonecfg export`.
    pub config: ZoneConfig,
}

/// Every zone with a readable configuration, any brand, cached briefly.
pub async fn cached_zones(state: &AppState) -> ApiResult<Vec<ZoneInfo>> {
    let zones = state.zones.clone();
    Ok(state
        .zones_cache
        .get_or(|| async move {
            let mut out = Vec::new();
            for summary in zones.list().await? {
                match zones.config(&summary.name).await {
                    Ok(config) => out.push(ZoneInfo { summary, config }),
                    Err(e) => {
                        tracing::warn!(zone = %summary.name, error = %e, "cannot read zone config");
                    }
                }
            }
            Ok::<_, mandrake_zones::ZoneError>(out)
        })
        .await?)
}

/// The zones of the brands `/zones` manages.
pub async fn all_zones(state: &AppState) -> ApiResult<Vec<ZoneInfo>> {
    Ok(cached_zones(state)
        .await?
        .into_iter()
        .filter(|z| ZoneBrand::from_brand(&z.summary.brand).is_some())
        .collect())
}

/// The bhyve-brand zones, which `/vms` manages.
pub async fn all_vms(state: &AppState) -> ApiResult<Vec<ZoneInfo>> {
    Ok(cached_zones(state)
        .await?
        .into_iter()
        .filter(|z| z.summary.brand == mandrake_bhyve::BRAND)
        .collect())
}

/// Forget cached zone listings after a change.
pub fn invalidate(state: &AppState) {
    state.zones_cache.clear();
}

/// The id stored on a zone, if any.
pub fn stored_id(config: &ZoneConfig) -> Option<Id> {
    config.attrs.get(ID_ATTR).and_then(|s| s.parse().ok())
}

/// The id for one zone: stored, else assigned now, else derived from the
/// name so reads never fail.
pub async fn ensure_id(state: &AppState, info: &ZoneInfo) -> Id {
    if let Some(id) = stored_id(&info.config) {
        return id;
    }
    let id = Id::new();
    match state
        .zones
        .set_attr(&info.summary.name, ID_ATTR, &id.to_string())
        .await
    {
        Ok(()) => {
            invalidate(state);
            id
        }
        Err(e) => {
            tracing::warn!(zone = %info.summary.name, error = %e, "cannot store mandrake-id; using a derived id");
            Id::derived(state.host_id, "zone", &info.summary.name)
        }
    }
}

/// Ids for a listing, in order.
pub async fn ids_for(state: &AppState, infos: &[ZoneInfo]) -> Vec<Id> {
    let mut ids = Vec::with_capacity(infos.len());
    for info in infos {
        ids.push(ensure_id(state, info).await);
    }
    ids
}

/// One zone by id.
pub async fn find_zone(state: &AppState, id: Id) -> ApiResult<(Id, ZoneInfo)> {
    let infos = all_zones(state).await?;
    let ids = ids_for(state, &infos).await;
    ids.into_iter()
        .zip(infos)
        .find(|(zid, _)| *zid == id)
        .ok_or_else(|| ApiError::not_found("zone"))
}

/// `/tank/zones/web` to `(tank, tank/zones/web)`.
pub fn dataset_of(zonepath: &str) -> Option<(String, String)> {
    let dataset = zonepath.trim_start_matches('/');
    let pool = dataset.split('/').next()?;
    (!pool.is_empty()).then(|| (pool.to_owned(), dataset.to_owned()))
}

/// The zonepath for a zone dataset.
pub fn zonepath_for(pool: &str, name: &str) -> String {
    format!("/{pool}/zones/{name}")
}

/// The wire view. `None` for brands not managed here.
pub fn to_zone(info: &ZoneInfo, id: Id, metadata: Option<Metadata>) -> Option<Zone> {
    let brand = ZoneBrand::from_brand(&info.summary.brand)?;
    let (pool, dataset) =
        dataset_of(&info.summary.zonepath).map_or((None, None), |(p, d)| (Some(p), Some(d)));
    let resolvers = info
        .config
        .attrs
        .get(RESOLVERS_ATTR)
        .map(|r| {
            r.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Some(Zone {
        id,
        name: info.summary.name.clone(),
        brand,
        state: info.summary.state,
        image_id: info
            .config
            .attrs
            .get(IMAGE_ATTR)
            .and_then(|s| s.parse().ok()),
        pool,
        dataset,
        zonepath: info.summary.zonepath.clone(),
        nics: info.config.nics.clone(),
        cpu_cap: info.config.cpu_cap,
        memory_cap_bytes: info.config.memory_cap,
        autoboot: info.config.autoboot,
        hostname: info.config.attrs.get(HOSTNAME_ATTR).cloned(),
        resolvers,
        created_at: None,
        metadata,
    })
}

/// The managed attributes for a spec.
pub fn attrs(
    id: Id,
    image: Option<Id>,
    hostname: Option<&str>,
    resolvers: &[String],
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    attrs.insert(ID_ATTR.to_owned(), id.to_string());
    if let Some(image) = image {
        attrs.insert(IMAGE_ATTR.to_owned(), image.to_string());
    }
    if let Some(h) = hostname {
        attrs.insert(HOSTNAME_ATTR.to_owned(), h.to_owned());
    }
    if !resolvers.is_empty() {
        attrs.insert(RESOLVERS_ATTR.to_owned(), resolvers.join(","));
    }
    attrs
}

/// A spec from an existing configuration, keeping the unmanaged
/// attributes it already has.
pub fn spec_from(config: &ZoneConfig) -> ZoneSpec {
    ZoneSpec {
        name: config.name.clone(),
        brand: config.brand.clone(),
        zonepath: config.zonepath.clone(),
        autoboot: config.autoboot,
        nics: config.nics.clone(),
        cpu_cap: config.cpu_cap,
        memory_cap: config.memory_cap,
        devices: config.devices.clone(),
        fs: config.fs.clone(),
        attrs: config.attrs.clone(),
    }
}

/// Publish `<family>.state`.
pub async fn emit_state(state: &AppState, family: &str, id: Id, name: &str, zone_state: ZoneState) {
    state
        .emit(
            &format!("{family}.state"),
            ObjectRef::new(family, id, name),
            None,
            Some(json!({ "state": zone_state })),
        )
        .await;
}

/// The current state of a zone by name, uncached.
pub async fn current_state(state: &AppState, name: &str) -> ApiResult<Option<ZoneState>> {
    let list = state.zones.list().await?;
    Ok(list.into_iter().find(|z| z.name == name).map(|z| z.state))
}

/// Poll until the zone reaches one of `want`, or is gone when `want` is
/// empty.
pub async fn wait_for(
    state: &AppState,
    name: &str,
    want: &[ZoneState],
) -> ApiResult<Option<ZoneState>> {
    let poll = state.scan_poll;
    let deadline = tokio::time::Instant::now() + LIFECYCLE_TIMEOUT;
    loop {
        let now = current_state(state, name).await?;
        match now {
            None if want.is_empty() => return Ok(None),
            Some(s) if want.contains(&s) => return Ok(Some(s)),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::typed(StatusCode::CONFLICT, "timeout", "Conflict")
                .detail(format!("zone {name} did not reach {want:?} in time")));
        }
        tokio::time::sleep(poll).await;
    }
}

/// What the install job does after the zonecfg exists.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// The zone dataset to create.
    pub dataset: String,
    /// Clone this snapshot into it, else create it empty.
    pub clone_from: Option<String>,
    /// Install source.
    pub source: InstallSource,
    /// Boot afterwards.
    pub boot: bool,
}

/// Run the install job: dataset, `zoneadm install`, optional boot. On
/// failure the zone stays configured for the operator to inspect or
/// delete; a dataset created here is removed.
pub async fn start_install(
    state: &AppState,
    id: Id,
    name: &str,
    plan: InstallPlan,
    actor: &Actor,
) -> ApiResult<Job> {
    let target = ObjectRef::new("zone", id, name);
    let job_state = state.clone();
    let name = name.to_owned();
    state
        .start_job(
            "zone.install",
            Some(target),
            Some(actor),
            move |job| async move {
                job.progress(0.1, "creating dataset").await;
                match &plan.clone_from {
                    Some(snapshot) => {
                        job_state
                            .zfs
                            .clone_snapshot(snapshot, &plan.dataset)
                            .await?;
                    }
                    None => {
                        job_state
                            .zfs
                            .create_dataset(&DatasetSpec {
                                name: plan.dataset.clone(),
                                kind: DatasetKind::Filesystem,
                                volsize: None,
                                sparse: false,
                                create_parents: true,
                                properties: Vec::new(),
                            })
                            .await?;
                    }
                }
                job_state.datasets_cache.clear();
                job_state.snapshots_cache.clear();
                job.progress(0.3, "installing").await;
                if let Err(e) = job_state.zones.install(&name, &plan.source).await {
                    let _ = job_state.zfs.destroy_dataset(&plan.dataset, true).await;
                    job_state.datasets_cache.clear();
                    invalidate(&job_state);
                    return Err(ApiError::from(e));
                }
                invalidate(&job_state);
                emit_state(&job_state, "zone", id, &name, ZoneState::Installed).await;
                if plan.boot {
                    job.progress(0.8, "booting").await;
                    job_state.zones.boot(&name).await?;
                    wait_for(&job_state, &name, &[ZoneState::Running]).await?;
                    invalidate(&job_state);
                    emit_state(&job_state, "zone", id, &name, ZoneState::Running).await;
                    return Ok("installed and running".to_owned());
                }
                Ok("installed".to_owned())
            },
        )
        .await
}

/// A lifecycle change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// `zoneadm boot`.
    Start,
    /// `zoneadm shutdown`, or `halt` when forced.
    Stop {
        /// Halt instead of a clean shutdown.
        force: bool,
    },
    /// `zoneadm shutdown -r`.
    Restart,
    /// `zoneadm halt` then `boot`, without asking the guest.
    Reset,
}

impl Lifecycle {
    /// The verb.
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop { .. } => "stop",
            Self::Restart => "restart",
            Self::Reset => "reset",
        }
    }

    /// The job and audit kind for a family: `zone.start`, `vm.reset`, ...
    pub fn kind_for(self, family: &str) -> String {
        format!("{family}.{}", self.verb())
    }
}

/// Run a lifecycle job and wait for the resulting state.
pub async fn start_lifecycle(
    state: &AppState,
    family: &'static str,
    id: Id,
    name: &str,
    op: Lifecycle,
    actor: &Actor,
) -> ApiResult<Job> {
    let target = ObjectRef::new(family, id, name);
    let job_state = state.clone();
    let name = name.to_owned();
    state
        .start_job(
            &op.kind_for(family),
            Some(target),
            Some(actor),
            move |job| async move {
                let (message, want) = match op {
                    Lifecycle::Start => {
                        job.progress(0.2, "booting").await;
                        job_state.zones.boot(&name).await?;
                        ("running", ZoneState::Running)
                    }
                    Lifecycle::Stop { force: true } => {
                        job.progress(0.2, "halting").await;
                        job_state.zones.halt(&name).await?;
                        ("halted", ZoneState::Installed)
                    }
                    Lifecycle::Stop { force: false } => {
                        job.progress(0.2, "shutting down").await;
                        job_state.zones.shutdown(&name, false).await?;
                        ("shut down", ZoneState::Installed)
                    }
                    Lifecycle::Restart => {
                        job.progress(0.2, "rebooting").await;
                        job_state.zones.shutdown(&name, true).await?;
                        ("running", ZoneState::Running)
                    }
                    Lifecycle::Reset => {
                        job.progress(0.2, "halting").await;
                        job_state.zones.halt(&name).await?;
                        wait_for(&job_state, &name, &[ZoneState::Installed]).await?;
                        job.progress(0.6, "booting").await;
                        job_state.zones.boot(&name).await?;
                        ("running", ZoneState::Running)
                    }
                };
                invalidate(&job_state);
                let reached = wait_for(&job_state, &name, &[want]).await?;
                invalidate(&job_state);
                if let Some(s) = reached {
                    emit_state(&job_state, family, id, &name, s).await;
                }
                Ok(message.to_owned())
            },
        )
        .await
}

/// Run the delete job: halt, uninstall, remove the configuration, and
/// with `purge` the dataset too.
pub async fn start_delete(
    state: &AppState,
    family: &'static str,
    id: Id,
    info: &ZoneInfo,
    purge: bool,
    actor: &Actor,
) -> ApiResult<Job> {
    let name = info.summary.name.clone();
    let target = ObjectRef::new(family, id, &name);
    let dataset = dataset_of(&info.summary.zonepath).map(|(_, d)| d);
    let job_state = state.clone();
    state
        .start_job(
            &format!("{family}.delete"),
            Some(target),
            Some(actor),
            move |job| async move {
                let current = current_state(&job_state, &name).await?;
                if matches!(
                    current,
                    Some(
                        ZoneState::Running
                            | ZoneState::Ready
                            | ZoneState::ShuttingDown
                            | ZoneState::Down
                    )
                ) {
                    job.progress(0.1, "halting").await;
                    job_state.zones.halt(&name).await?;
                    wait_for(&job_state, &name, &[ZoneState::Installed]).await?;
                }
                let current = current_state(&job_state, &name).await?;
                if matches!(current, Some(ZoneState::Installed | ZoneState::Incomplete)) {
                    job.progress(0.4, "uninstalling").await;
                    job_state.zones.uninstall(&name).await?;
                }
                job.progress(0.7, "removing configuration").await;
                job_state.zones.delete(&name).await?;
                invalidate(&job_state);
                if purge && let Some(ds) = &dataset {
                    job.progress(0.9, "destroying dataset").await;
                    match job_state.zfs.destroy_dataset(ds, true).await {
                        Ok(()) => {}
                        Err(e) if e.kind() == mandrake_core::shell::FailureKind::NotFound => {}
                        Err(e) => return Err(ApiError::from(e)),
                    }
                    job_state.datasets_cache.clear();
                    job_state.snapshots_cache.clear();
                }
                let _ = job_state
                    .db
                    .call(move |conn| crate::metadata::remove(conn, id))
                    .await;
                job_state
                    .emit(
                        &format!("{family}.deleted"),
                        ObjectRef::new(family, id, &name),
                        None,
                        Some(json!({ "purged": purge })),
                    )
                    .await;
                Ok(if purge {
                    "deleted with datasets".to_owned()
                } else {
                    "deleted; datasets kept".to_owned()
                })
            },
        )
        .await
}

/// A NIC list rendered for audit.
pub fn nics_summary(nics: &[ZoneNic]) -> serde_json::Value {
    json!(
        nics.iter()
            .map(|n| json!({ "name": n.name, "over": n.over, "vid": n.vid, "address": n.address }))
            .collect::<Vec<_>>()
    )
}
