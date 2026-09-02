//! What the driver reads and accepts. Wire shapes live in
//! `mandrake_core::storage`; these carry only what illumos knows.

use mandrake_core::{
    Timestamp,
    shell::ShellError,
    storage::{Bytes, DatasetKind, PoolHealth, ScanStatus, Vdev, VdevSpec},
};

/// Driver result.
pub type Result<T> = std::result::Result<T, ZfsError>;

/// Why an operation failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ZfsError {
    /// The tool failed; `stderr` is its message.
    #[error(transparent)]
    Command(#[from] ShellError),
    /// The tool's output was not understood.
    #[error("cannot parse `{command}` output: {detail}")]
    Parse {
        /// Which command.
        command: String,
        /// What was wrong.
        detail: String,
    },
    /// The named object is not there.
    #[error("{0} does not exist")]
    NotFound(String),
}

/// A coarse classification of a failure, for HTTP mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The object does not exist.
    NotFound,
    /// The object already exists.
    Exists,
    /// Busy, has dependents, or otherwise refused for now.
    Conflict,
    /// Not permitted.
    Forbidden,
    /// Bad arguments.
    Invalid,
    /// Anything else.
    Other,
}

impl ZfsError {
    /// Classify from the tool's message.
    pub fn kind(&self) -> FailureKind {
        let text = match self {
            Self::NotFound(_) => return FailureKind::NotFound,
            Self::Parse { .. } => return FailureKind::Other,
            Self::Command(e) => e.stderr().to_ascii_lowercase(),
        };
        if text.contains("does not exist") || text.contains("no such pool") {
            FailureKind::NotFound
        } else if text.contains("already exists") || text.contains("dataset already exists") {
            FailureKind::Exists
        } else if text.contains("busy")
            || text.contains("has children")
            || text.contains("dependent clones")
            || text.contains("more recent snapshots")
            || text.contains("is in use")
            || text.contains("currently scrubbing")
            || text.contains("no active scrub")
        {
            FailureKind::Conflict
        } else if text.contains("permission denied") || text.contains("insufficient privileges") {
            FailureKind::Forbidden
        } else if text.contains("invalid") || text.contains("must be") || text.contains("usage:") {
            FailureKind::Invalid
        } else {
            FailureKind::Other
        }
    }
}

/// A pool as `zpool list` and `zpool status` describe it.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolInfo {
    /// Name.
    pub name: String,
    /// Health.
    pub health: PoolHealth,
    /// Size.
    pub size: Bytes,
    /// Allocated.
    pub allocated: Bytes,
    /// Free.
    pub free: Bytes,
    /// Fragmentation percent.
    pub fragmentation: Option<u32>,
    /// Capacity percent.
    pub capacity: Option<u32>,
    /// Dedup ratio.
    pub dedup_ratio: Option<f64>,
    /// Vdev tree.
    pub vdevs: Vdev,
    /// Scan.
    pub scan: Option<ScanStatus>,
    /// `status:` and `action:` text.
    pub status_text: Option<String>,
}

/// A dataset as `zfs list` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetInfo {
    /// Full name.
    pub name: String,
    /// Kind.
    pub kind: DatasetKind,
    /// Mountpoint, `None` for `none`, `legacy`, or volumes.
    pub mountpoint: Option<String>,
    /// Mounted.
    pub mounted: bool,
    /// Used.
    pub used: Bytes,
    /// Available.
    pub available: Bytes,
    /// Referenced.
    pub referenced: Bytes,
    /// Logical used.
    pub logical_used: Option<Bytes>,
    /// Quota.
    pub quota: Option<Bytes>,
    /// Reservation.
    pub reservation: Option<Bytes>,
    /// Compression.
    pub compression: Option<String>,
    /// Compress ratio.
    pub compress_ratio: Option<f64>,
    /// atime.
    pub atime: Option<bool>,
    /// Record size.
    pub recordsize: Option<Bytes>,
    /// Volume size.
    pub volsize: Option<Bytes>,
    /// Volume block size.
    pub volblocksize: Option<Bytes>,
    /// Clone origin.
    pub origin: Option<String>,
    /// Created.
    pub created_at: Timestamp,
    /// The Mandrake id property, if set.
    pub mandrake_id: Option<String>,
}

impl DatasetInfo {
    /// The pool name.
    pub fn pool(&self) -> &str {
        self.name.split('/').next().unwrap_or(&self.name)
    }
}

/// A snapshot as `zfs list -t snapshot` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotInfo {
    /// `dataset@snap`.
    pub name: String,
    /// Used.
    pub used: Bytes,
    /// Referenced.
    pub referenced: Bytes,
    /// Created.
    pub created_at: Timestamp,
    /// Clones.
    pub clones: Vec<String>,
    /// The Mandrake id property, if set.
    pub mandrake_id: Option<String>,
}

impl SnapshotInfo {
    /// The dataset part.
    pub fn dataset(&self) -> &str {
        self.name.split('@').next().unwrap_or(&self.name)
    }

    /// The part after `@`.
    pub fn short_name(&self) -> &str {
        self.name.split('@').nth(1).unwrap_or("")
    }
}

/// A disk as `diskinfo` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// `c1t0d0`.
    pub name: String,
    /// Bus type.
    pub bus: Option<String>,
    /// Vendor.
    pub vendor: Option<String>,
    /// Product.
    pub product: Option<String>,
    /// Serial.
    pub serial: Option<String>,
    /// Size.
    pub size: Bytes,
    /// Removable.
    pub removable: bool,
    /// SSD.
    pub solid_state: Option<bool>,
}

/// What to create a pool from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolSpec {
    /// Name.
    pub name: String,
    /// Layout.
    pub vdevs: Vec<VdevSpec>,
    /// ashift.
    pub ashift: Option<u32>,
    /// Root dataset compression.
    pub compression: Option<String>,
    /// `-f`.
    pub force: bool,
    /// Root dataset user properties (`-O`), for the Mandrake id.
    pub root_properties: Vec<(String, String)>,
}

/// What to create a dataset from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetSpec {
    /// Full name.
    pub name: String,
    /// Kind.
    pub kind: DatasetKind,
    /// Volume size.
    pub volsize: Option<Bytes>,
    /// Sparse volume.
    pub sparse: bool,
    /// `-p`.
    pub create_parents: bool,
    /// `-o` properties in order.
    pub properties: Vec<(String, String)>,
}
