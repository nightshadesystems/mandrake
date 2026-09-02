//! An in-memory ZFS with the same observable behaviour as the real one,
//! for route tests and for developing the console away from illumos.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use mandrake_core::{
    Timestamp,
    shell::ShellError,
    storage::{
        Bytes, DatasetKind, PoolHealth, ScanFunction, ScanState, ScanStatus, Vdev, VdevSpecType,
        VdevType,
    },
};

use crate::{
    BoxFuture, DatasetInfo, DatasetSpec, DeviceInfo, ID_PROPERTY, PoolInfo, PoolSpec, Result,
    SnapshotInfo, Zfs, ZfsError,
};

#[derive(Default)]
struct State {
    pools: BTreeMap<String, PoolInfo>,
    datasets: BTreeMap<String, DatasetInfo>,
    snapshots: BTreeMap<String, SnapshotInfo>,
    devices: Vec<DeviceInfo>,
}

/// The fake driver. Clone to share.
#[derive(Clone, Default)]
pub struct FakeZfs {
    state: Arc<Mutex<State>>,
}

fn tool_error(message: &str) -> ZfsError {
    ZfsError::Command(ShellError::Failed {
        command: "fake".to_owned(),
        status: 1,
        stderr: message.to_owned(),
    })
}

fn disk(name: &str) -> Vdev {
    Vdev {
        name: name.to_owned(),
        type_: VdevType::Disk,
        state: PoolHealth::Online,
        read_errors: Some(0),
        write_errors: Some(0),
        checksum_errors: Some(0),
        note: None,
        children: Vec::new(),
    }
}

fn dataset(name: &str, kind: DatasetKind) -> DatasetInfo {
    let is_fs = kind == DatasetKind::Filesystem;
    DatasetInfo {
        name: name.to_owned(),
        kind,
        mountpoint: is_fs.then(|| format!("/{name}")),
        mounted: is_fs,
        used: 98_304,
        available: 1 << 40,
        referenced: 98_304,
        logical_used: Some(98_304),
        quota: None,
        reservation: None,
        compression: Some("lz4".to_owned()),
        compress_ratio: Some(1.0),
        atime: is_fs.then_some(true),
        recordsize: is_fs.then_some(131_072),
        volsize: None,
        volblocksize: (!is_fs).then_some(8192),
        origin: None,
        created_at: Timestamp::now(),
        mandrake_id: None,
    }
}

impl FakeZfs {
    /// An empty host with no pools.
    pub fn new() -> Self {
        Self::default()
    }

    /// A typical host: `rpool` on `c1t0d0` with a boot environment, four
    /// free disks, and no data pool.
    pub fn typical() -> Self {
        let fake = Self::new();
        fake.add_pool("rpool", &[&["c1t0d0"]], 64 << 30);
        fake.add_dataset("rpool/ROOT", DatasetKind::Filesystem);
        fake.add_dataset("rpool/ROOT/omnios", DatasetKind::Filesystem);
        fake.add_dataset("rpool/mandrake", DatasetKind::Filesystem);
        fake.add_dataset("rpool/mandrake/var", DatasetKind::Filesystem);
        for (i, name) in ["c1t0d0", "c1t1d0", "c1t2d0", "c1t3d0", "c1t4d0"]
            .iter()
            .enumerate()
        {
            fake.add_device(name, if i == 0 { 64 << 30 } else { 4_000_787_030_016 });
        }
        fake
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Add a pool of mirrors (each inner slice is one mirror or a single disk).
    pub fn add_pool(&self, name: &str, mirrors: &[&[&str]], size: Bytes) {
        let children = mirrors
            .iter()
            .enumerate()
            .map(|(i, devs)| {
                if devs.len() == 1 {
                    disk(devs[0])
                } else {
                    Vdev {
                        name: format!("mirror-{i}"),
                        type_: VdevType::Mirror,
                        state: PoolHealth::Online,
                        read_errors: Some(0),
                        write_errors: Some(0),
                        checksum_errors: Some(0),
                        note: None,
                        children: devs.iter().map(|d| disk(d)).collect(),
                    }
                }
            })
            .collect();
        let pool = PoolInfo {
            name: name.to_owned(),
            health: PoolHealth::Online,
            size,
            allocated: size / 8,
            free: size - size / 8,
            fragmentation: Some(1),
            capacity: Some(12),
            dedup_ratio: Some(1.0),
            vdevs: Vdev {
                name: name.to_owned(),
                type_: VdevType::Root,
                state: PoolHealth::Online,
                read_errors: Some(0),
                write_errors: Some(0),
                checksum_errors: Some(0),
                note: None,
                children,
            },
            scan: None,
            status_text: None,
        };
        let mut s = self.lock();
        s.pools.insert(name.to_owned(), pool);
        s.datasets
            .insert(name.to_owned(), dataset(name, DatasetKind::Filesystem));
    }

    /// Add a dataset.
    pub fn add_dataset(&self, name: &str, kind: DatasetKind) {
        self.lock()
            .datasets
            .insert(name.to_owned(), dataset(name, kind));
    }

    /// Add a device.
    pub fn add_device(&self, name: &str, size: Bytes) {
        self.lock().devices.push(DeviceInfo {
            name: name.to_owned(),
            bus: Some("SATA".to_owned()),
            vendor: Some("Fake".to_owned()),
            product: Some("Disk".to_owned()),
            serial: None,
            size,
            removable: false,
            solid_state: Some(false),
        });
    }

    /// Finish a scrub instantly (tests drive the job to completion).
    pub fn finish_scrub(&self, pool: &str) {
        if let Some(p) = self.lock().pools.get_mut(pool) {
            if let Some(scan) = p.scan.as_mut() {
                scan.state = ScanState::Finished;
                scan.progress = Some(1.0);
                scan.finished_at = Some(Timestamp::now());
            }
        }
    }

    fn require_dataset(s: &State, name: &str) -> Result<()> {
        if s.datasets.contains_key(name) {
            Ok(())
        } else {
            Err(tool_error(&format!(
                "cannot open '{name}': dataset does not exist"
            )))
        }
    }

    fn children_of<'a>(s: &'a State, name: &str) -> Vec<&'a str> {
        let prefix = format!("{name}/");
        s.datasets
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(String::as_str)
            .collect()
    }

    fn snapshots_of<'a>(s: &'a State, name: &str) -> Vec<&'a str> {
        let prefix = format!("{name}@");
        s.snapshots
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(String::as_str)
            .collect()
    }
}

impl Zfs for FakeZfs {
    fn list_pools(&self) -> BoxFuture<'_, Result<Vec<PoolInfo>>> {
        Box::pin(async move { Ok(self.lock().pools.values().cloned().collect()) })
    }

    fn pool<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<PoolInfo>> {
        Box::pin(async move {
            self.lock()
                .pools
                .get(name)
                .cloned()
                .ok_or_else(|| tool_error(&format!("cannot open '{name}': no such pool")))
        })
    }

    fn create_pool<'a>(&'a self, spec: &'a PoolSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            {
                let s = self.lock();
                if s.pools.contains_key(&spec.name) {
                    return Err(tool_error(&format!(
                        "cannot create '{}': pool already exists",
                        spec.name
                    )));
                }
                for v in &spec.vdevs {
                    for d in &v.devices {
                        if !s.devices.iter().any(|dev| dev.name == *d) {
                            return Err(tool_error(&format!(
                                "cannot open '{d}': no such device in /dev/dsk"
                            )));
                        }
                        let in_use = s
                            .pools
                            .values()
                            .any(|p| p.vdevs.leaves().contains(&d.as_str()));
                        if in_use && !spec.force {
                            return Err(tool_error(&format!(
                                "invalid vdev specification\nuse '-f' to override the following errors:\n/dev/dsk/{d}s0 is part of active pool"
                            )));
                        }
                    }
                    if v.type_ == VdevSpecType::Mirror && v.devices.len() < 2 {
                        return Err(tool_error(
                            "invalid vdev specification: mirror requires at least 2 devices",
                        ));
                    }
                }
            }
            let size: Bytes = {
                let s = self.lock();
                spec.vdevs
                    .iter()
                    .filter(|v| {
                        !matches!(
                            v.type_,
                            VdevSpecType::Log | VdevSpecType::Cache | VdevSpecType::Spare
                        )
                    })
                    .map(|v| {
                        let sizes: Vec<Bytes> = v
                            .devices
                            .iter()
                            .filter_map(|d| {
                                s.devices
                                    .iter()
                                    .find(|dev| dev.name == *d)
                                    .map(|dev| dev.size)
                            })
                            .collect();
                        match v.type_ {
                            VdevSpecType::Stripe => sizes.iter().sum(),
                            _ => sizes.iter().copied().min().unwrap_or(0),
                        }
                    })
                    .sum()
            };
            let groups: Vec<Vec<&str>> = spec
                .vdevs
                .iter()
                .map(|v| v.devices.iter().map(String::as_str).collect())
                .collect();
            let refs: Vec<&[&str]> = groups.iter().map(Vec::as_slice).collect();
            self.add_pool(&spec.name, &refs, size);
            if let Some(id) = spec.root_properties.iter().find(|(k, _)| k == ID_PROPERTY) {
                if let Some(d) = self.lock().datasets.get_mut(&spec.name) {
                    d.mandrake_id = Some(id.1.clone());
                }
            }
            if let Some(c) = &spec.compression {
                if let Some(d) = self.lock().datasets.get_mut(&spec.name) {
                    d.compression = Some(c.clone());
                }
            }
            Ok(())
        })
    }

    fn destroy_pool<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            if s.pools.remove(name).is_none() {
                return Err(tool_error(&format!("cannot open '{name}': no such pool")));
            }
            let prefix = format!("{name}/");
            s.datasets
                .retain(|k, _| k != name && !k.starts_with(&prefix));
            s.snapshots
                .retain(|k, _| !k.starts_with(&prefix) && !k.starts_with(&format!("{name}@")));
            Ok(())
        })
    }

    fn scrub<'a>(&'a self, name: &'a str, stop: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let Some(pool) = s.pools.get_mut(name) else {
                return Err(tool_error(&format!("cannot open '{name}': no such pool")));
            };
            let running = pool
                .scan
                .as_ref()
                .is_some_and(|sc| sc.state == ScanState::InProgress);
            if stop {
                if !running {
                    return Err(tool_error(&format!(
                        "cannot cancel scrubbing {name}: there is no active scrub"
                    )));
                }
                if let Some(sc) = pool.scan.as_mut() {
                    sc.state = ScanState::Canceled;
                    sc.finished_at = Some(Timestamp::now());
                }
                return Ok(());
            }
            if running {
                return Err(tool_error(&format!(
                    "cannot scrub {name}: currently scrubbing; use 'zpool scrub -s' to cancel current scrub"
                )));
            }
            pool.scan = Some(ScanStatus {
                function: ScanFunction::Scrub,
                state: ScanState::InProgress,
                progress: Some(0.0),
                started_at: Some(Timestamp::now()),
                finished_at: None,
                errors: Some(0),
                rate_bytes_per_second: None,
                summary: "scrub in progress (fake)".to_owned(),
            });
            Ok(())
        })
    }

    fn list_datasets(&self) -> BoxFuture<'_, Result<Vec<DatasetInfo>>> {
        Box::pin(async move { Ok(self.lock().datasets.values().cloned().collect()) })
    }

    fn dataset<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<DatasetInfo>> {
        Box::pin(async move {
            self.lock()
                .datasets
                .get(name)
                .cloned()
                .ok_or_else(|| tool_error(&format!("cannot open '{name}': dataset does not exist")))
        })
    }

    fn create_dataset<'a>(&'a self, spec: &'a DatasetSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            if s.datasets.contains_key(&spec.name) {
                return Err(tool_error(&format!(
                    "cannot create '{}': dataset already exists",
                    spec.name
                )));
            }
            let Some((parent, _)) = spec.name.rsplit_once('/') else {
                return Err(tool_error(&format!(
                    "cannot create '{}': missing dataset name",
                    spec.name
                )));
            };
            if !s.datasets.contains_key(parent) {
                if !spec.create_parents {
                    return Err(tool_error(&format!(
                        "cannot create '{}': parent does not exist",
                        spec.name
                    )));
                }
                let mut path = String::new();
                for part in parent.split('/') {
                    if !path.is_empty() {
                        path.push('/');
                    }
                    path.push_str(part);
                    if !s.datasets.contains_key(&path) {
                        let d = dataset(&path, DatasetKind::Filesystem);
                        s.datasets.insert(path.clone(), d);
                    }
                }
            }
            let mut d = dataset(&spec.name, spec.kind);
            if spec.kind == DatasetKind::Volume {
                d.volsize = spec.volsize;
                d.referenced = if spec.sparse {
                    0
                } else {
                    spec.volsize.unwrap_or(0)
                };
                d.used = d.referenced;
            }
            for (k, v) in &spec.properties {
                apply_property(&mut d, k, v);
            }
            s.datasets.insert(spec.name.clone(), d);
            Ok(())
        })
    }

    fn set_properties<'a>(
        &'a self,
        name: &'a str,
        props: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            if let Some(d) = s.datasets.get_mut(name) {
                for (k, v) in props {
                    if k == "volsize" {
                        let new: Bytes = v.parse().unwrap_or(0);
                        if d.volsize.is_some_and(|old| new < old) {
                            return Err(tool_error(&format!(
                                "cannot set property for '{name}': 'volsize' must be a multiple of volume block size and cannot shrink"
                            )));
                        }
                    }
                    apply_property(d, k, v);
                }
                return Ok(());
            }
            if let Some(snap) = s.snapshots.get_mut(name) {
                for (k, v) in props {
                    if k == ID_PROPERTY {
                        snap.mandrake_id = Some(v.clone());
                    }
                }
                return Ok(());
            }
            Err(tool_error(&format!(
                "cannot open '{name}': dataset does not exist"
            )))
        })
    }

    fn destroy_dataset<'a>(&'a self, name: &'a str, recursive: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_dataset(&s, name)?;
            let children: Vec<String> = Self::children_of(&s, name)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let snaps: Vec<String> = Self::snapshots_of(&s, name)
                .into_iter()
                .map(str::to_owned)
                .collect();
            if !recursive && !children.is_empty() {
                return Err(tool_error(&format!(
                    "cannot destroy '{name}': filesystem has children\nuse '-r' to destroy the following datasets:\n{}",
                    children.join("\n")
                )));
            }
            if !recursive && !snaps.is_empty() {
                return Err(tool_error(&format!(
                    "cannot destroy '{name}': filesystem has snapshots"
                )));
            }
            if s.snapshots.values().any(|sn| {
                sn.clones
                    .iter()
                    .any(|c| c == name || c.starts_with(&format!("{name}/")))
            }) {
                // clones of snapshots elsewhere referencing this: fine to destroy the clone
            }
            for snap in &snaps {
                if let Some(sn) = s.snapshots.get(snap) {
                    if !sn.clones.is_empty() {
                        return Err(tool_error(&format!(
                            "cannot destroy '{snap}': snapshot has dependent clones"
                        )));
                    }
                }
            }
            s.datasets.remove(name);
            for c in children {
                s.datasets.remove(&c);
                let prefix = format!("{c}@");
                s.snapshots.retain(|k, _| !k.starts_with(&prefix));
            }
            for snap in snaps {
                s.snapshots.remove(&snap);
            }
            Ok(())
        })
    }

    fn list_snapshots<'a>(
        &'a self,
        dataset: Option<&'a str>,
        recursive: bool,
    ) -> BoxFuture<'a, Result<Vec<SnapshotInfo>>> {
        Box::pin(async move {
            let s = self.lock();
            let mut out: Vec<SnapshotInfo> = s
                .snapshots
                .values()
                .filter(|sn| match dataset {
                    None => true,
                    Some(ds) if recursive => {
                        sn.dataset() == ds || sn.dataset().starts_with(&format!("{ds}/"))
                    }
                    Some(ds) => sn.dataset() == ds,
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.name.cmp(&b.name)));
            Ok(out)
        })
    }

    fn snapshot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<SnapshotInfo>> {
        Box::pin(async move {
            self.lock()
                .snapshots
                .get(name)
                .cloned()
                .ok_or_else(|| tool_error(&format!("cannot open '{name}': dataset does not exist")))
        })
    }

    fn create_snapshot<'a>(
        &'a self,
        dataset: &'a str,
        name: &'a str,
        recursive: bool,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_dataset(&s, dataset)?;
            let mut targets = vec![dataset.to_owned()];
            if recursive {
                targets.extend(
                    Self::children_of(&s, dataset)
                        .into_iter()
                        .map(str::to_owned),
                );
            }
            for t in &targets {
                let full = format!("{t}@{name}");
                if s.snapshots.contains_key(&full) {
                    return Err(tool_error(&format!(
                        "cannot create snapshot '{full}': dataset already exists"
                    )));
                }
            }
            for t in targets {
                let full = format!("{t}@{name}");
                let referenced = s.datasets.get(&t).map_or(0, |d| d.referenced);
                s.snapshots.insert(
                    full.clone(),
                    SnapshotInfo {
                        name: full,
                        used: 0,
                        referenced,
                        created_at: Timestamp::now(),
                        clones: Vec::new(),
                        mandrake_id: None,
                    },
                );
            }
            Ok(())
        })
    }

    fn destroy_snapshot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let Some(snap) = s.snapshots.get(name) else {
                return Err(tool_error(
                    "could not find any snapshots to destroy; check snapshot names.",
                ));
            };
            if !snap.clones.is_empty() {
                return Err(tool_error(&format!(
                    "cannot destroy '{name}': snapshot has dependent clones"
                )));
            }
            s.snapshots.remove(name);
            Ok(())
        })
    }

    fn rollback<'a>(&'a self, name: &'a str, discard_newer: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let Some(target) = s.snapshots.get(name).cloned() else {
                return Err(tool_error(&format!(
                    "cannot open '{name}': dataset does not exist"
                )));
            };
            let newer: Vec<String> = Self::snapshots_of(&s, target.dataset())
                .into_iter()
                .filter(|n| {
                    s.snapshots
                        .get(*n)
                        .is_some_and(|sn| sn.created_at > target.created_at)
                })
                .map(str::to_owned)
                .collect();
            if !newer.is_empty() && !discard_newer {
                return Err(tool_error(&format!(
                    "cannot rollback to '{name}': more recent snapshots exist\nuse '-r' to force deletion of the following snapshots:\n{}",
                    newer.join("\n")
                )));
            }
            for n in newer {
                s.snapshots.remove(&n);
            }
            Ok(())
        })
    }

    fn clone_snapshot<'a>(
        &'a self,
        snapshot: &'a str,
        target: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let Some(src) = s.snapshots.get(snapshot).cloned() else {
                return Err(tool_error(&format!(
                    "cannot open '{snapshot}': dataset does not exist"
                )));
            };
            if s.datasets.contains_key(target) {
                return Err(tool_error(&format!(
                    "cannot create '{target}': dataset already exists"
                )));
            }
            let kind = s
                .datasets
                .get(src.dataset())
                .map_or(DatasetKind::Filesystem, |d| d.kind);
            let mut d = dataset(target, kind);
            d.origin = Some(snapshot.to_owned());
            d.referenced = src.referenced;
            s.datasets.insert(target.to_owned(), d);
            if let Some(sn) = s.snapshots.get_mut(snapshot) {
                sn.clones.push(target.to_owned());
            }
            Ok(())
        })
    }

    fn list_devices(&self) -> BoxFuture<'_, Result<Vec<DeviceInfo>>> {
        Box::pin(async move { Ok(self.lock().devices.clone()) })
    }
}

fn apply_property(d: &mut DatasetInfo, key: &str, value: &str) {
    let bytes = |v: &str| v.parse::<Bytes>().ok().or_else(|| crate::parse::size(v));
    match key {
        "compression" => d.compression = Some(value.to_owned()),
        "quota" => d.quota = if value == "none" { None } else { bytes(value) },
        "reservation" | "refreservation" => {
            d.reservation = if value == "none" { None } else { bytes(value) }
        }
        "mountpoint" => {
            d.mountpoint = if value == "none" {
                None
            } else {
                Some(value.to_owned())
            }
        }
        "atime" => d.atime = Some(value == "on"),
        "recordsize" => d.recordsize = bytes(value),
        "volblocksize" => d.volblocksize = bytes(value),
        "volsize" => d.volsize = bytes(value),
        k if k == ID_PROPERTY => d.mandrake_id = Some(value.to_owned()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::panic,
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::err_expect
    )]

    use mandrake_core::storage::VdevSpec;

    use super::*;

    #[tokio::test]
    async fn create_snapshot_clone_rollback_and_destroy() {
        let z = FakeZfs::typical();
        let spec = PoolSpec {
            name: "tank".to_owned(),
            vdevs: vec![VdevSpec {
                type_: VdevSpecType::Mirror,
                devices: vec!["c1t1d0".to_owned(), "c1t2d0".to_owned()],
            }],
            ashift: None,
            compression: None,
            force: false,
            root_properties: vec![(ID_PROPERTY.to_owned(), "id-1".to_owned())],
        };
        z.create_pool(&spec).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            z.dataset("tank")
                .await
                .ok()
                .and_then(|d| d.mandrake_id)
                .as_deref(),
            Some("id-1")
        );
        // Reusing a disk without force is refused.
        assert_eq!(
            z.create_pool(&PoolSpec {
                name: "other".to_owned(),
                ..spec.clone()
            })
            .await
            .err()
            .map(|e| e.kind()),
            Some(crate::FailureKind::Invalid)
        );

        let ds = DatasetSpec {
            name: "tank/vms/disk0".to_owned(),
            kind: DatasetKind::Volume,
            volsize: Some(1 << 30),
            sparse: true,
            create_parents: true,
            properties: vec![],
        };
        z.create_dataset(&ds)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(z.dataset("tank/vms").await.is_ok());
        z.create_snapshot("tank/vms/disk0", "s1", false)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        z.clone_snapshot("tank/vms/disk0@s1", "tank/vms/disk1")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            z.destroy_snapshot("tank/vms/disk0@s1")
                .await
                .err()
                .map(|e| e.kind()),
            Some(crate::FailureKind::Conflict)
        );
        assert_eq!(
            z.destroy_dataset("tank/vms", false)
                .await
                .err()
                .map(|e| e.kind()),
            Some(crate::FailureKind::Conflict)
        );
        z.destroy_dataset("tank/vms/disk1", false)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        // The clone is gone but the snapshot still records it in this fake; real ZFS drops it.
        z.set_properties("tank/vms/disk0", &[("volsize".to_owned(), "1".to_owned())])
            .await
            .err()
            .expect("shrink refused");
        z.scrub("tank", false)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            z.scrub("tank", false).await.err().map(|e| e.kind()),
            Some(crate::FailureKind::Conflict)
        );
        z.finish_scrub("tank");
        assert_eq!(
            z.pool("tank")
                .await
                .ok()
                .and_then(|p| p.scan)
                .map(|s| s.state),
            Some(ScanState::Finished)
        );
        z.destroy_pool("tank")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(z.dataset("tank/vms/disk0").await.is_err());
    }
}
