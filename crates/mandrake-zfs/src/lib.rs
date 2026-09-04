//! Driver for ZFS storage and boot environments via `zfs`, `zpool`, and
//! `beadm` (ADR-0003, ADR-0011).
//!
//! [`Zfs`] is the typed operation surface. [`ZfsCli`] implements it by
//! shelling out through a [`mandrake_core::shell::Runner`]; [`FakeZfs`]
//! implements it in memory with the same observable behaviour so the
//! daemon's routes are testable anywhere. Parsers live in [`parse`] as pure
//! functions over `&str`.
//!
//! `rpool` is observed and protected; data pools are fully managed. The
//! protection rules themselves are the daemon's (ADR-0011); this crate
//! executes what it is asked.

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod beadm;
pub mod cli;
pub mod fake;
pub mod parse;
pub mod types;

pub use beadm::{BeadmCli, BootEnvs, FakeBeadm};
pub use cli::ZfsCli;
pub use fake::FakeZfs;
pub use mandrake_core::shell::BoxFuture;
pub use types::*;

/// Typed ZFS operations. Names are full dataset or pool names.
pub trait Zfs: Send + Sync {
    /// Every pool with its vdev tree, health, and scan status.
    fn list_pools(&self) -> BoxFuture<'_, Result<Vec<PoolInfo>>>;
    /// One pool.
    fn pool<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<PoolInfo>>;
    /// `zpool create`.
    fn create_pool<'a>(&'a self, spec: &'a PoolSpec) -> BoxFuture<'a, Result<()>>;
    /// `zpool destroy`.
    fn destroy_pool<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zpool scrub`, or `zpool scrub -s` to stop.
    fn scrub<'a>(&'a self, name: &'a str, stop: bool) -> BoxFuture<'a, Result<()>>;

    /// Filesystems and volumes, sorted by name.
    fn list_datasets(&self) -> BoxFuture<'_, Result<Vec<DatasetInfo>>>;
    /// One dataset.
    fn dataset<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<DatasetInfo>>;
    /// `zfs create`.
    fn create_dataset<'a>(&'a self, spec: &'a DatasetSpec) -> BoxFuture<'a, Result<()>>;
    /// `zfs set k=v ...`.
    fn set_properties<'a>(
        &'a self,
        name: &'a str,
        props: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<()>>;
    /// `zfs destroy [-r]`.
    fn destroy_dataset<'a>(&'a self, name: &'a str, recursive: bool) -> BoxFuture<'a, Result<()>>;

    /// Snapshots of one dataset (`-d 1`), of it and its descendants
    /// (`recursive`), or of everything when `dataset` is `None`.
    fn list_snapshots<'a>(
        &'a self,
        dataset: Option<&'a str>,
        recursive: bool,
    ) -> BoxFuture<'a, Result<Vec<SnapshotInfo>>>;
    /// One snapshot by full name.
    fn snapshot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<SnapshotInfo>>;
    /// `zfs snapshot [-r] dataset@name`.
    fn create_snapshot<'a>(
        &'a self,
        dataset: &'a str,
        name: &'a str,
        recursive: bool,
    ) -> BoxFuture<'a, Result<()>>;
    /// `zfs destroy dataset@name`.
    fn destroy_snapshot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zfs destroy -r dataset@name`: the snapshot on every descendant too.
    fn destroy_snapshot_recursive<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zfs rollback [-r]`.
    fn rollback<'a>(&'a self, name: &'a str, discard_newer: bool) -> BoxFuture<'a, Result<()>>;
    /// `zfs clone snapshot target`.
    fn clone_snapshot<'a>(
        &'a self,
        snapshot: &'a str,
        target: &'a str,
    ) -> BoxFuture<'a, Result<()>>;

    /// Disks on the host, without pool membership (the caller joins that).
    fn list_devices(&self) -> BoxFuture<'_, Result<Vec<DeviceInfo>>>;
}

/// The ZFS user property carrying a Mandrake id (ADR-0002).
pub const ID_PROPERTY: &str = "nightshade.systems:mandrake-id";
