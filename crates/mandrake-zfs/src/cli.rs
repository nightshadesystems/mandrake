//! The real driver: `zpool`, `zfs`, and `diskinfo` through a [`Runner`].

use std::sync::Arc;

use mandrake_core::{
    shell::{Command, Runner},
    storage::{DatasetKind, VdevSpecType},
};

use crate::{
    BoxFuture, DatasetInfo, DatasetSpec, DeviceInfo, PoolInfo, PoolSpec, Result, SnapshotInfo, Zfs,
    ZfsError, parse,
};

/// Shells out to the illumos tools.
#[derive(Clone)]
pub struct ZfsCli {
    runner: Arc<dyn Runner>,
}

impl ZfsCli {
    /// A driver over `runner`.
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }

    async fn stdout(&self, cmd: Command) -> Result<String> {
        Ok(self.runner.run(&cmd).await?.stdout)
    }

    async fn run(&self, cmd: Command) -> Result<()> {
        self.runner.run(&cmd).await?;
        Ok(())
    }

    async fn pool_info(&self, row: parse::PoolListRow) -> Result<PoolInfo> {
        let status = self
            .stdout(Command::new("zpool").args(["status", &row.name]))
            .await?;
        let status = parse::zpool_status(&status)?;
        Ok(PoolInfo {
            name: row.name,
            health: row.health,
            size: row.size,
            allocated: row.allocated,
            free: row.free,
            fragmentation: row.fragmentation,
            capacity: row.capacity,
            dedup_ratio: row.dedup_ratio,
            vdevs: status.vdevs,
            scan: status.scan,
            status_text: status.status_text,
        })
    }
}

/// `zpool create` arguments for a layout, data vdevs first.
pub fn vdev_args(vdevs: &[mandrake_core::storage::VdevSpec]) -> Vec<String> {
    let mut out = Vec::new();
    let word = |t: VdevSpecType| match t {
        VdevSpecType::Stripe => None,
        VdevSpecType::Mirror => Some("mirror"),
        VdevSpecType::Raidz1 => Some("raidz1"),
        VdevSpecType::Raidz2 => Some("raidz2"),
        VdevSpecType::Raidz3 => Some("raidz3"),
        VdevSpecType::Log => Some("log"),
        VdevSpecType::Cache => Some("cache"),
        VdevSpecType::Spare => Some("spare"),
    };
    let is_aux = |t: VdevSpecType| {
        matches!(
            t,
            VdevSpecType::Log | VdevSpecType::Cache | VdevSpecType::Spare
        )
    };
    for v in vdevs.iter().filter(|v| !is_aux(v.type_)) {
        if let Some(w) = word(v.type_) {
            out.push(w.to_owned());
        }
        out.extend(v.devices.iter().cloned());
    }
    for v in vdevs.iter().filter(|v| is_aux(v.type_)) {
        if let Some(w) = word(v.type_) {
            out.push(w.to_owned());
        }
        out.extend(v.devices.iter().cloned());
    }
    out
}

impl Zfs for ZfsCli {
    fn list_pools(&self) -> BoxFuture<'_, Result<Vec<PoolInfo>>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zpool").args([
                    "list",
                    "-Hp",
                    "-o",
                    parse::ZPOOL_LIST_COLUMNS,
                ]))
                .await?;
            let mut pools = Vec::new();
            for row in parse::zpool_list(&out)? {
                pools.push(self.pool_info(row).await?);
            }
            Ok(pools)
        })
    }

    fn pool<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<PoolInfo>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zpool").args([
                    "list",
                    "-Hp",
                    "-o",
                    parse::ZPOOL_LIST_COLUMNS,
                    name,
                ]))
                .await?;
            let row = parse::zpool_list(&out)?
                .into_iter()
                .next()
                .ok_or_else(|| ZfsError::NotFound(name.to_owned()))?;
            self.pool_info(row).await
        })
    }

    fn create_pool<'a>(&'a self, spec: &'a PoolSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("zpool").arg("create").privileged();
            if spec.force {
                cmd = cmd.arg("-f");
            }
            cmd = cmd.args(["-o", &format!("ashift={}", spec.ashift.unwrap_or(12))]);
            cmd = cmd.args([
                "-O",
                &format!(
                    "compression={}",
                    spec.compression.as_deref().unwrap_or("lz4")
                ),
            ]);
            for (k, v) in &spec.root_properties {
                cmd = cmd.args(["-O", &format!("{k}={v}")]);
            }
            cmd = cmd.arg(&spec.name).args(vdev_args(&spec.vdevs));
            self.run(cmd).await
        })
    }

    fn destroy_pool<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(Command::new("zpool").args(["destroy", name]).privileged())
                .await
        })
    }

    fn scrub<'a>(&'a self, name: &'a str, stop: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("zpool").arg("scrub").privileged();
            if stop {
                cmd = cmd.arg("-s");
            }
            self.run(cmd.arg(name)).await
        })
    }

    fn list_datasets(&self) -> BoxFuture<'_, Result<Vec<DatasetInfo>>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zfs").args([
                    "list",
                    "-Hp",
                    "-t",
                    "filesystem,volume",
                    "-s",
                    "name",
                    "-o",
                    parse::ZFS_LIST_COLUMNS,
                ]))
                .await?;
            parse::zfs_list(&out)
        })
    }

    fn dataset<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<DatasetInfo>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zfs").args([
                    "list",
                    "-Hp",
                    "-t",
                    "filesystem,volume",
                    "-o",
                    parse::ZFS_LIST_COLUMNS,
                    name,
                ]))
                .await?;
            parse::zfs_list(&out)?
                .into_iter()
                .next()
                .ok_or_else(|| ZfsError::NotFound(name.to_owned()))
        })
    }

    fn create_dataset<'a>(&'a self, spec: &'a DatasetSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("zfs").arg("create").privileged();
            if spec.create_parents {
                cmd = cmd.arg("-p");
            }
            for (k, v) in &spec.properties {
                cmd = cmd.args(["-o", &format!("{k}={v}")]);
            }
            if spec.kind == DatasetKind::Volume {
                if spec.sparse {
                    cmd = cmd.arg("-s");
                }
                cmd = cmd.args(["-V", &spec.volsize.unwrap_or(0).to_string()]);
            }
            self.run(cmd.arg(&spec.name)).await
        })
    }

    fn set_properties<'a>(
        &'a self,
        name: &'a str,
        props: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if props.is_empty() {
                return Ok(());
            }
            let mut cmd = Command::new("zfs").arg("set").privileged();
            for (k, v) in props {
                cmd = cmd.arg(format!("{k}={v}"));
            }
            self.run(cmd.arg(name)).await
        })
    }

    fn destroy_dataset<'a>(&'a self, name: &'a str, recursive: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("zfs").arg("destroy").privileged();
            if recursive {
                cmd = cmd.arg("-r");
            }
            self.run(cmd.arg(name)).await
        })
    }

    fn list_snapshots<'a>(
        &'a self,
        dataset: Option<&'a str>,
        recursive: bool,
    ) -> BoxFuture<'a, Result<Vec<SnapshotInfo>>> {
        Box::pin(async move {
            let mut cmd = Command::new("zfs").args([
                "list",
                "-Hp",
                "-t",
                "snapshot",
                "-s",
                "creation",
                "-o",
                parse::ZFS_SNAPSHOT_COLUMNS,
            ]);
            if let Some(ds) = dataset {
                cmd = if recursive {
                    cmd.arg("-r")
                } else {
                    cmd.args(["-d", "1"])
                };
                cmd = cmd.arg(ds);
            }
            let out = self.stdout(cmd).await?;
            parse::zfs_snapshots(&out)
        })
    }

    fn snapshot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<SnapshotInfo>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zfs").args([
                    "list",
                    "-Hp",
                    "-t",
                    "snapshot",
                    "-o",
                    parse::ZFS_SNAPSHOT_COLUMNS,
                    name,
                ]))
                .await?;
            parse::zfs_snapshots(&out)?
                .into_iter()
                .next()
                .ok_or_else(|| ZfsError::NotFound(name.to_owned()))
        })
    }

    fn create_snapshot<'a>(
        &'a self,
        dataset: &'a str,
        name: &'a str,
        recursive: bool,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("zfs").arg("snapshot").privileged();
            if recursive {
                cmd = cmd.arg("-r");
            }
            self.run(cmd.arg(format!("{dataset}@{name}"))).await
        })
    }

    fn destroy_snapshot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(Command::new("zfs").args(["destroy", name]).privileged())
                .await
        })
    }

    fn destroy_snapshot_recursive<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("zfs")
                    .args(["destroy", "-r", name])
                    .privileged(),
            )
            .await
        })
    }

    fn rollback<'a>(&'a self, name: &'a str, discard_newer: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("zfs").arg("rollback").privileged();
            if discard_newer {
                cmd = cmd.arg("-r");
            }
            self.run(cmd.arg(name)).await
        })
    }

    fn clone_snapshot<'a>(
        &'a self,
        snapshot: &'a str,
        target: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("zfs")
                    .args(["clone", snapshot, target])
                    .privileged(),
            )
            .await
        })
    }

    fn list_devices(&self) -> BoxFuture<'_, Result<Vec<DeviceInfo>>> {
        Box::pin(async move {
            let out = self.stdout(Command::new("diskinfo").args(["-Hp"])).await?;
            Ok(parse::diskinfo(&out))
        })
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

    use mandrake_core::{shell::ScriptedRunner, storage::VdevSpec};

    use super::*;

    fn driver(r: Arc<ScriptedRunner>) -> ZfsCli {
        ZfsCli::new(r)
    }

    #[tokio::test]
    async fn create_pool_builds_the_expected_command() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok("zpool create", "");
        let spec = PoolSpec {
            name: "tank".to_owned(),
            vdevs: vec![
                VdevSpec {
                    type_: VdevSpecType::Log,
                    devices: vec!["c2t0d0".to_owned()],
                },
                VdevSpec {
                    type_: VdevSpecType::Mirror,
                    devices: vec!["c1t1d0".to_owned(), "c1t2d0".to_owned()],
                },
                VdevSpec {
                    type_: VdevSpecType::Mirror,
                    devices: vec!["c1t3d0".to_owned(), "c1t4d0".to_owned()],
                },
            ],
            ashift: None,
            compression: None,
            force: true,
            root_properties: vec![(
                "nightshade.systems:mandrake-id".to_owned(),
                "abc".to_owned(),
            )],
        };
        driver(Arc::clone(&r))
            .create_pool(&spec)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            r.lines(),
            vec![
                "pfexec zpool create -f -o ashift=12 -O compression=lz4 -O nightshade.systems:mandrake-id=abc \
                 tank mirror c1t1d0 c1t2d0 mirror c1t3d0 c1t4d0 log c2t0d0"
            ]
        );
    }

    #[tokio::test]
    async fn dataset_and_snapshot_commands() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok("zfs", "");
        let d = driver(Arc::clone(&r));
        let spec = DatasetSpec {
            name: "tank/vms/disk0".to_owned(),
            kind: DatasetKind::Volume,
            volsize: Some(1024),
            sparse: true,
            create_parents: true,
            properties: vec![("volblocksize".to_owned(), "8192".to_owned())],
        };
        d.create_dataset(&spec)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        d.set_properties("tank/a", &[("quota".to_owned(), "10G".to_owned())])
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        d.destroy_dataset("tank/a", true)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        d.create_snapshot("tank/a", "s1", false)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        d.rollback("tank/a@s1", true)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        d.clone_snapshot("tank/a@s1", "tank/b")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let _ = d.list_snapshots(Some("tank/a"), false).await;
        let _ = d.list_snapshots(Some("tank"), true).await;
        assert_eq!(
            r.lines(),
            vec![
                "pfexec zfs create -p -o volblocksize=8192 -s -V 1024 tank/vms/disk0",
                "pfexec zfs set quota=10G tank/a",
                "pfexec zfs destroy -r tank/a",
                "pfexec zfs snapshot tank/a@s1",
                "pfexec zfs rollback -r tank/a@s1",
                "pfexec zfs clone tank/a@s1 tank/b",
                "zfs list -Hp -t snapshot -s creation -o name,used,referenced,creation,clones,nightshade.systems:mandrake-id -d 1 tank/a",
                "zfs list -Hp -t snapshot -s creation -o name,used,referenced,creation,clones,nightshade.systems:mandrake-id -r tank",
            ]
        );
    }

    #[tokio::test]
    async fn pools_join_list_and_status() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok(
            "zpool list",
            include_str!("../testdata/zpool-list-Hp.synthetic.txt"),
        )
        .ok(
            "zpool status rpool",
            include_str!("../testdata/zpool-status.rpool.synthetic.txt"),
        )
        .ok(
            "zpool status tank",
            include_str!("../testdata/zpool-status.tank.synthetic.txt"),
        );
        let pools = driver(r)
            .list_pools()
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(pools.len(), 2);
        assert_eq!(pools[1].name, "tank");
        assert_eq!(pools[1].vdevs.children.len(), 5);
        assert_eq!(pools[1].capacity, Some(12));
    }

    #[tokio::test]
    async fn failures_classify() {
        let r = Arc::new(ScriptedRunner::new());
        r.fail(
            "zfs destroy",
            1,
            "cannot destroy 'tank/a': filesystem has children",
        );
        let err = driver(r)
            .destroy_dataset("tank/a", false)
            .await
            .err()
            .expect("error");
        assert_eq!(err.kind(), crate::FailureKind::Conflict);
    }
}
