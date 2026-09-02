//! Storage wire types mirroring the `storage` family in `api/openapi.yaml`.

// `Option<Option<T>>` is the contract's three-state PATCH field: absent,
// explicit null (clear), or a value.
#![allow(clippy::option_option)]

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp, api::Metadata};

/// A size in bytes.
pub type Bytes = u64;

/// Health of a pool or a vdev, as `zpool` prints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PoolHealth {
    /// Healthy.
    Online,
    /// Working with reduced redundancy.
    Degraded,
    /// Unusable.
    Faulted,
    /// Administratively offline.
    Offline,
    /// Cannot be opened.
    Unavail,
    /// Device removed.
    Removed,
    /// I/O suspended.
    Suspended,
    /// A spare that is available.
    Avail,
    /// A spare that is in use.
    Inuse,
}

impl PoolHealth {
    /// As `zpool` prints it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Online => "ONLINE",
            Self::Degraded => "DEGRADED",
            Self::Faulted => "FAULTED",
            Self::Offline => "OFFLINE",
            Self::Unavail => "UNAVAIL",
            Self::Removed => "REMOVED",
            Self::Suspended => "SUSPENDED",
            Self::Avail => "AVAIL",
            Self::Inuse => "INUSE",
        }
    }
}

impl fmt::Display for PoolHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unknown health word.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown pool state `{0}`")]
pub struct UnknownHealth(pub String);

impl FromStr for PoolHealth {
    type Err = UnknownHealth;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "ONLINE" => Self::Online,
            "DEGRADED" => Self::Degraded,
            "FAULTED" => Self::Faulted,
            "OFFLINE" => Self::Offline,
            "UNAVAIL" => Self::Unavail,
            "REMOVED" => Self::Removed,
            "SUSPENDED" => Self::Suspended,
            "AVAIL" => Self::Avail,
            "INUSE" => Self::Inuse,
            other => return Err(UnknownHealth(other.to_owned())),
        })
    }
}

/// What a vdev node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VdevType {
    /// The pool itself.
    Root,
    /// A whole disk or slice.
    Disk,
    /// A file-backed vdev.
    File,
    /// N-way mirror.
    Mirror,
    /// Single parity.
    Raidz1,
    /// Double parity.
    Raidz2,
    /// Triple parity.
    Raidz3,
    /// The `logs` group.
    Log,
    /// The `cache` group.
    Cache,
    /// The `spares` group.
    Spare,
    /// A device being replaced.
    Replacing,
    /// A spare that has kicked in for a device.
    SpareGroup,
}

/// One node of a pool's vdev tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vdev {
    /// As `zpool status` prints it.
    pub name: String,
    /// Node kind.
    #[serde(rename = "type")]
    pub type_: VdevType,
    /// State.
    pub state: PoolHealth,
    /// Read errors, when shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_errors: Option<u64>,
    /// Write errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_errors: Option<u64>,
    /// Checksum errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_errors: Option<u64>,
    /// Trailing note such as `(resilvering)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Children.
    pub children: Vec<Vdev>,
}

impl Vdev {
    /// Every leaf device name in the tree.
    pub fn leaves(&self) -> Vec<&str> {
        if self.children.is_empty() && matches!(self.type_, VdevType::Disk | VdevType::File) {
            return vec![self.name.as_str()];
        }
        self.children.iter().flat_map(Vdev::leaves).collect()
    }
}

/// Scrub or resilver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanFunction {
    /// Scrub.
    Scrub,
    /// Resilver.
    Resilver,
}

/// Where a scan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanState {
    /// Running.
    InProgress,
    /// Done.
    Finished,
    /// Stopped by the administrator.
    Canceled,
}

/// The current or last scan of a pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanStatus {
    /// What kind.
    pub function: ScanFunction,
    /// Where it is.
    pub state: ScanState,
    /// Fraction done, when running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<Timestamp>,
    /// Finished or canceled.
    pub finished_at: Option<Timestamp>,
    /// Errors found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<u64>,
    /// Throughput.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_bytes_per_second: Option<u64>,
    /// The `scan:` text verbatim.
    pub summary: String,
}

/// A pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pool {
    /// Id (the root dataset's).
    pub id: Id,
    /// Name.
    pub name: String,
    /// Health.
    pub health: PoolHealth,
    /// Size.
    pub size_bytes: Bytes,
    /// Allocated.
    pub allocated_bytes: Bytes,
    /// Free.
    pub free_bytes: Bytes,
    /// Fragmentation, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragmentation_percent: Option<u32>,
    /// Capacity used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_percent: Option<u32>,
    /// Dedup ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup_ratio: Option<f64>,
    /// `rpool`.
    pub protected: bool,
    /// Vdev tree rooted at the pool.
    pub vdevs: Vdev,
    /// Scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<ScanStatus>,
    /// `status:` and `action:` text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    /// Mandrake metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Top-level vdev kinds a request can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VdevSpecType {
    /// Plain striped devices.
    Stripe,
    /// Mirror.
    Mirror,
    /// Single parity.
    Raidz1,
    /// Double parity.
    Raidz2,
    /// Triple parity.
    Raidz3,
    /// Log devices.
    Log,
    /// Cache devices.
    Cache,
    /// Hot spares.
    Spare,
}

/// One top-level vdev to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VdevSpec {
    /// Kind.
    #[serde(rename = "type")]
    pub type_: VdevSpecType,
    /// Device names.
    pub devices: Vec<String>,
}

/// `POST /storage/pools`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolCreate {
    /// Name.
    pub name: String,
    /// Layout.
    pub vdevs: Vec<VdevSpec>,
    /// ashift, default 12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ashift: Option<u32>,
    /// Root dataset compression, default lz4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    /// `zpool create -f`.
    #[serde(default)]
    pub force: bool,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `DELETE /storage/pools/{id}` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolDestroy {
    /// Must equal the pool name.
    pub name: String,
}

/// Filesystem or volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatasetKind {
    /// A filesystem.
    Filesystem,
    /// A zvol.
    Volume,
}

impl DatasetKind {
    /// As `zfs list -o type` prints it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Volume => "volume",
        }
    }
}

/// A dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dataset {
    /// Id.
    pub id: Id,
    /// Full name.
    pub name: String,
    /// Pool.
    pub pool: String,
    /// Kind.
    pub kind: DatasetKind,
    /// Mountpoint.
    pub mountpoint: Option<String>,
    /// Mounted now.
    #[serde(default)]
    pub mounted: bool,
    /// Used.
    pub used_bytes: Bytes,
    /// Available.
    pub available_bytes: Bytes,
    /// Referenced.
    pub referenced_bytes: Bytes,
    /// Logical used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_used_bytes: Option<Bytes>,
    /// Quota.
    pub quota_bytes: Option<Bytes>,
    /// Reservation.
    pub reservation_bytes: Option<Bytes>,
    /// Compression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    /// Compression ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compress_ratio: Option<f64>,
    /// atime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atime: Option<bool>,
    /// Record size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recordsize_bytes: Option<Bytes>,
    /// Volume size.
    pub volsize_bytes: Option<Bytes>,
    /// Volume block size.
    pub volblocksize_bytes: Option<Bytes>,
    /// Clone origin.
    pub origin: Option<String>,
    /// Protected.
    pub protected: bool,
    /// Created.
    pub created_at: Timestamp,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /storage/datasets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetCreate {
    /// Full name.
    pub name: String,
    /// Kind.
    pub kind: DatasetKind,
    /// Volume size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volsize_bytes: Option<Bytes>,
    /// Volume block size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volblocksize_bytes: Option<Bytes>,
    /// Compression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    /// Quota.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_bytes: Option<Bytes>,
    /// Reservation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation_bytes: Option<Bytes>,
    /// Mountpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<String>,
    /// atime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atime: Option<bool>,
    /// Record size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recordsize_bytes: Option<Bytes>,
    /// Sparse volume.
    #[serde(default)]
    pub sparse: bool,
    /// `zfs create -p`.
    #[serde(default)]
    pub create_parents: bool,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `PATCH /storage/datasets/{id}`. `Some(None)` clears a clearable property.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DatasetUpdate {
    /// Grow a volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volsize_bytes: Option<Bytes>,
    /// Compression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    /// Quota; explicit null clears.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub quota_bytes: Option<Option<Bytes>>,
    /// Reservation; explicit null clears.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub reservation_bytes: Option<Option<Bytes>>,
    /// Mountpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<String>,
    /// atime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atime: Option<bool>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// A snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Id.
    pub id: Id,
    /// `dataset@snap`.
    pub name: String,
    /// Dataset.
    pub dataset: String,
    /// After the `@`.
    pub short_name: String,
    /// Used.
    pub used_bytes: Bytes,
    /// Referenced.
    pub referenced_bytes: Bytes,
    /// Clones.
    #[serde(default)]
    pub clones: Vec<String>,
    /// Created.
    pub created_at: Timestamp,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /storage/snapshots`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotCreate {
    /// Dataset.
    pub dataset: String,
    /// After the `@`.
    pub name: String,
    /// Recursive.
    #[serde(default)]
    pub recursive: bool,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// A disk as `diskinfo` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// `c1t0d0`.
    pub name: String,
    /// Vendor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// Product.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// Serial.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Size.
    pub size_bytes: Bytes,
    /// Removable.
    pub removable: bool,
    /// SSD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solid_state: Option<bool>,
    /// Pool using it.
    pub pool: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_round_trips() {
        for h in [PoolHealth::Online, PoolHealth::Degraded, PoolHealth::Avail] {
            assert_eq!(h.as_str().parse::<PoolHealth>().ok(), Some(h));
            let json = serde_json::to_string(&h).unwrap_or_default();
            assert_eq!(json, format!("\"{}\"", h.as_str()));
        }
    }

    #[test]
    fn vdev_leaves() {
        let leaf = |n: &str| Vdev {
            name: n.to_owned(),
            type_: VdevType::Disk,
            state: PoolHealth::Online,
            read_errors: None,
            write_errors: None,
            checksum_errors: None,
            note: None,
            children: vec![],
        };
        let root = Vdev {
            name: "tank".to_owned(),
            type_: VdevType::Root,
            state: PoolHealth::Online,
            read_errors: None,
            write_errors: None,
            checksum_errors: None,
            note: None,
            children: vec![Vdev {
                name: "mirror-0".to_owned(),
                type_: VdevType::Mirror,
                state: PoolHealth::Online,
                read_errors: None,
                write_errors: None,
                checksum_errors: None,
                note: None,
                children: vec![leaf("c1t1d0"), leaf("c1t2d0")],
            }],
        };
        assert_eq!(root.leaves(), vec!["c1t1d0", "c1t2d0"]);
    }

    #[test]
    fn update_distinguishes_absent_from_null() {
        let u: DatasetUpdate = serde_json::from_str(r#"{"quota_bytes":null}"#).unwrap_or_default();
        assert_eq!(u.quota_bytes, Some(None));
        assert_eq!(u.reservation_bytes, None);
        let u: DatasetUpdate = serde_json::from_str(r#"{"quota_bytes":10}"#).unwrap_or_default();
        assert_eq!(u.quota_bytes, Some(Some(10)));
    }
}
