//! VMs in the daemon (ADR-0013): the view over bhyve-brand zones, disk
//! and snapshot helpers, and the create job. Lifecycle and delete jobs
//! are shared with zones.

use std::collections::HashMap;

use mandrake_bhyve::{DiskSpec, VmConfig, VmSpec, from_zone_config, to_zone_spec, zvol_device};
use mandrake_core::{
    Actor, Id,
    api::{Job, Metadata, ObjectRef},
    storage::DatasetKind,
    vm::{Vm, VmCdrom, VmDisk, VmSnapshot},
    zone::ZoneState,
};
use mandrake_zfs::{DatasetInfo, DatasetSpec, SnapshotInfo};
use mandrake_zones::{IMAGE_ATTR, InstallSource};

use crate::{
    app::AppState,
    error::{ApiError, ApiResult},
    zones::{self, ZoneInfo},
};

/// The API family, for jobs, events, and audit.
pub const FAMILY: &str = "vm";

/// A VM as the drivers see it.
#[derive(Debug, Clone)]
pub struct VmInfo {
    /// The zone.
    pub zone: ZoneInfo,
    /// The brand attributes read back.
    pub config: VmConfig,
}

/// Every bhyve zone whose configuration reads as a VM.
pub async fn all_vms(state: &AppState) -> ApiResult<Vec<VmInfo>> {
    Ok(zones::all_vms(state)
        .await?
        .into_iter()
        .filter_map(|zone| match from_zone_config(&zone.config) {
            Ok(config) => Some(VmInfo { zone, config }),
            Err(e) => {
                tracing::warn!(zone = %zone.summary.name, error = %e, "bhyve zone is not a VM");
                None
            }
        })
        .collect())
}

/// Ids for a listing, in order.
pub async fn ids_for(state: &AppState, infos: &[VmInfo]) -> Vec<Id> {
    let mut ids = Vec::with_capacity(infos.len());
    for info in infos {
        ids.push(zones::ensure_id(state, &info.zone).await);
    }
    ids
}

/// One VM by id.
pub async fn find_vm(state: &AppState, id: Id) -> ApiResult<(Id, VmInfo)> {
    let infos = all_vms(state).await?;
    let ids = ids_for(state, &infos).await;
    ids.into_iter()
        .zip(infos)
        .find(|(vid, _)| *vid == id)
        .ok_or_else(|| ApiError::not_found("vm"))
}

/// The VM dataset.
pub fn dataset_for(pool: &str, name: &str) -> String {
    format!("{pool}/vms/{name}")
}

/// The zonepath.
pub fn zonepath_for(pool: &str, name: &str) -> String {
    format!("/{pool}/vms/{name}")
}

/// The zvol for a disk slot.
pub fn zvol_for(dataset: &str, index: usize) -> String {
    format!("{dataset}/disk{index}")
}

/// The image id an ISO path under `images/iso` carries.
pub fn cdrom_image_id(path: &str) -> Option<Id> {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse().ok())
}

/// Volumes on the host, cached, for disk sizes.
async fn volumes(state: &AppState) -> ApiResult<Vec<DatasetInfo>> {
    let zfs = state.zfs.clone();
    Ok(state
        .datasets_cache
        .get_or(|| async move { zfs.list_datasets().await })
        .await?
        .into_iter()
        .filter(|d| d.kind == DatasetKind::Volume)
        .collect())
}

/// The wire view.
pub async fn to_vm(
    state: &AppState,
    info: &VmInfo,
    id: Id,
    metadata: Option<Metadata>,
) -> ApiResult<Vm> {
    let vols = volumes(state).await?;
    let sizes: HashMap<&str, u64> = vols
        .iter()
        .map(|v| (v.name.as_str(), v.volsize.unwrap_or(v.referenced)))
        .collect();
    let image_id: Option<Id> = info
        .config
        .attrs
        .get(IMAGE_ATTR)
        .and_then(|s| s.parse().ok());
    let disks = info
        .config
        .disks
        .iter()
        .map(|(slot, d)| VmDisk {
            index: *slot,
            dataset: d.zvol.clone(),
            device: Some(zvol_device(&d.zvol)),
            size_bytes: sizes.get(d.zvol.as_str()).copied().unwrap_or(0),
            boot: d.boot,
            image_id: if d.boot { image_id } else { None },
        })
        .collect();
    let cdroms = info
        .config
        .cdroms
        .iter()
        .filter_map(|(slot, path)| {
            cdrom_image_id(path).map(|image_id| VmCdrom {
                index: *slot,
                image_id,
                path: path.clone(),
            })
        })
        .collect();
    let (pool, dataset) = zones::dataset_of(&info.zone.summary.zonepath)
        .map_or((None, None), |(p, d)| (Some(p), Some(d)));
    Ok(Vm {
        id,
        name: info.zone.summary.name.clone(),
        state: info.zone.summary.state,
        vcpus: info.config.vcpus,
        memory_bytes: info.config.memory_bytes,
        bootrom: info.config.bootrom,
        acpi: info.config.acpi,
        disks,
        cdroms,
        nics: info.config.nics.clone(),
        vnc: info.config.vnc,
        autoboot: info.config.autoboot,
        pool,
        dataset,
        zonepath: info.zone.summary.zonepath.clone(),
        image_id,
        created_at: None,
        metadata,
    })
}

/// A spec from the configuration read back, with disks and cdroms in
/// slot order (slots are renumbered from 0 on write).
pub fn spec_from(info: &VmInfo) -> VmSpec {
    VmSpec {
        name: info.config.name.clone(),
        zonepath: info.config.zonepath.clone(),
        vcpus: info.config.vcpus,
        memory_bytes: info.config.memory_bytes,
        bootrom: info.config.bootrom,
        acpi: info.config.acpi,
        vnc: info.config.vnc,
        autoboot: info.config.autoboot,
        disks: info.config.disks.iter().map(|(_, d)| d.clone()).collect(),
        cdroms: info.config.cdroms.iter().map(|(_, p)| p.clone()).collect(),
        nics: info.config.nics.clone(),
        attrs: info.config.attrs.clone(),
    }
}

/// Write a spec to the zonecfg and forget the cache.
pub async fn write_spec(state: &AppState, spec: &VmSpec) -> ApiResult<()> {
    state.zones.update(&to_zone_spec(spec)).await?;
    zones::invalidate(state);
    Ok(())
}

/// How one disk comes to be.
#[derive(Debug, Clone)]
pub struct DiskPlan {
    /// The zvol.
    pub zvol: String,
    /// Clone this snapshot, else create blank.
    pub clone_from: Option<String>,
    /// Size for a blank disk.
    pub size: Option<u64>,
}

/// Create one disk.
pub async fn make_disk(state: &AppState, plan: &DiskPlan) -> ApiResult<()> {
    match &plan.clone_from {
        Some(snapshot) => state.zfs.clone_snapshot(snapshot, &plan.zvol).await?,
        None => {
            state
                .zfs
                .create_dataset(&DatasetSpec {
                    name: plan.zvol.clone(),
                    kind: DatasetKind::Volume,
                    volsize: plan.size,
                    sparse: true,
                    create_parents: true,
                    properties: Vec::new(),
                })
                .await?;
        }
    }
    state.datasets_cache.clear();
    state.snapshots_cache.clear();
    Ok(())
}

/// What the create job does after the zonecfg exists.
#[derive(Debug, Clone)]
pub struct CreatePlan {
    /// The VM dataset.
    pub dataset: String,
    /// Disks in slot order.
    pub disks: Vec<DiskPlan>,
    /// Boot afterwards.
    pub boot: bool,
}

/// Run the create job: dataset and disks, `zoneadm install`, optional
/// boot. On failure the configuration stays and the dataset is removed.
pub async fn start_create(
    state: &AppState,
    id: Id,
    name: &str,
    plan: CreatePlan,
    actor: &Actor,
) -> ApiResult<Job> {
    let target = ObjectRef::new(FAMILY, id, name);
    let job_state = state.clone();
    let name = name.to_owned();
    state
        .start_job(
            "vm.create",
            Some(target),
            Some(actor),
            move |job| async move {
                let outcome = async {
                    job.progress(0.1, "creating dataset").await;
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
                    for (i, disk) in plan.disks.iter().enumerate() {
                        job.progress(0.2, format!("creating disk {i}")).await;
                        make_disk(&job_state, disk).await?;
                    }
                    job.progress(0.5, "installing").await;
                    job_state
                        .zones
                        .install(&name, &InstallSource::Packages)
                        .await?;
                    Ok::<(), ApiError>(())
                }
                .await;
                if let Err(e) = outcome {
                    let _ = job_state.zfs.destroy_dataset(&plan.dataset, true).await;
                    job_state.datasets_cache.clear();
                    zones::invalidate(&job_state);
                    return Err(e);
                }
                zones::invalidate(&job_state);
                zones::emit_state(&job_state, FAMILY, id, &name, ZoneState::Installed).await;
                if plan.boot {
                    job.progress(0.8, "booting").await;
                    job_state.zones.boot(&name).await?;
                    zones::wait_for(&job_state, &name, &[ZoneState::Running]).await?;
                    zones::invalidate(&job_state);
                    zones::emit_state(&job_state, FAMILY, id, &name, ZoneState::Running).await;
                    return Ok("created and running".to_owned());
                }
                Ok("created".to_owned())
            },
        )
        .await
}

/// The VM's snapshots: those of the VM dataset, with the usage of every
/// disk's snapshot of the same name added in.
pub async fn snapshots(state: &AppState, dataset: &str) -> ApiResult<Vec<VmSnapshot>> {
    let all: Vec<SnapshotInfo> = state.zfs.list_snapshots(Some(dataset), true).await?;
    let tops: Vec<SnapshotInfo> = all
        .iter()
        .filter(|s| s.dataset() == dataset)
        .cloned()
        .collect();
    let ids = crate::routes::storage::snapshot_ids(state, &tops).await;
    let mut meta = crate::routes::storage::metadata_for(state, &ids).await?;
    Ok(tops
        .into_iter()
        .zip(ids)
        .map(|(top, id)| {
            let name = top.short_name().to_owned();
            let used = all
                .iter()
                .filter(|s| s.short_name() == name)
                .map(|s| s.used)
                .sum();
            VmSnapshot {
                id,
                name,
                created_at: top.created_at,
                used_bytes: used,
                metadata: meta.remove(&id),
            }
        })
        .collect())
}

/// The datasets a recursive snapshot of `dataset` covers: itself and its
/// disks.
pub fn snapshot_members(dataset: &str, disks: &[DiskSpec]) -> Vec<String> {
    let mut members = vec![dataset.to_owned()];
    members.extend(disks.iter().map(|d| d.zvol.clone()));
    members
}
