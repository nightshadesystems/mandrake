//! VM wire types mirroring the `vms` family in `api/openapi.yaml`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    Id, Timestamp,
    api::Metadata,
    zone::{ZoneNic, ZoneState},
};

/// Guest firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Bootrom {
    /// UEFI.
    Uefi,
    /// UEFI with the legacy BIOS shim.
    UefiCsm,
}

impl Bootrom {
    /// As the API spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uefi => "uefi",
            Self::UefiCsm => "uefi-csm",
        }
    }
}

impl fmt::Display for Bootrom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One zvol-backed disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmDisk {
    /// Slot.
    pub index: u32,
    /// Zvol.
    pub dataset: String,
    /// Device path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Size.
    pub size_bytes: u64,
    /// Boots.
    pub boot: bool,
    /// Image it was cloned from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
}

/// One attached ISO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmCdrom {
    /// Slot.
    pub index: u32,
    /// Image.
    pub image_id: Id,
    /// File path.
    pub path: String,
}

/// A VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vm {
    /// Id.
    pub id: Id,
    /// Name.
    pub name: String,
    /// State.
    pub state: ZoneState,
    /// vCPUs.
    pub vcpus: u32,
    /// Memory.
    pub memory_bytes: u64,
    /// Firmware.
    pub bootrom: Bootrom,
    /// ACPI.
    pub acpi: bool,
    /// Disks.
    pub disks: Vec<VmDisk>,
    /// Cdroms.
    pub cdroms: Vec<VmCdrom>,
    /// NICs.
    pub nics: Vec<ZoneNic>,
    /// VNC server on.
    pub vnc: bool,
    /// Autoboot.
    pub autoboot: bool,
    /// Pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Dataset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<String>,
    /// Zonepath.
    pub zonepath: String,
    /// Boot image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
    /// Created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// A disk to create: blank, or a clone of a `vm-raw` image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmDiskSpec {
    /// Size for a blank disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Image to clone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
    /// Boots.
    #[serde(default)]
    pub boot: bool,
}

/// `POST /vms`.
// The contract has four independent switches; a state machine would only
// obscure it.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmCreate {
    /// Name.
    pub name: String,
    /// vCPUs.
    pub vcpus: u32,
    /// Memory.
    pub memory_bytes: u64,
    /// Firmware; default `uefi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootrom: Option<Bootrom>,
    /// ACPI.
    #[serde(default = "default_true")]
    pub acpi: bool,
    /// Pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Disks; the first boots unless another has `boot`.
    pub disks: Vec<VmDiskSpec>,
    /// ISOs to attach.
    #[serde(default)]
    pub cdroms: Vec<Id>,
    /// NICs.
    #[serde(default)]
    pub nics: Vec<ZoneNic>,
    /// VNC.
    #[serde(default = "default_true")]
    pub vnc: bool,
    /// Autoboot.
    #[serde(default = "default_true")]
    pub autoboot: bool,
    /// Boot once created.
    #[serde(default = "default_true")]
    pub start: bool,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

fn default_true() -> bool {
    true
}

/// `PATCH /vms/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VmUpdate {
    /// vCPUs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcpus: Option<u32>,
    /// Memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    /// Firmware.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootrom: Option<Bootrom>,
    /// ACPI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acpi: Option<bool>,
    /// VNC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vnc: Option<bool>,
    /// Autoboot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoboot: Option<bool>,
    /// Replaces the NICs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nics: Option<Vec<ZoneNic>>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /vms/{id}/disks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmDiskAdd {
    /// Size for a blank disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Image to clone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
}

/// `PATCH /vms/{id}/disks/{index}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmDiskResize {
    /// New size.
    pub size_bytes: u64,
}

/// `POST /vms/{id}/cdroms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmCdromAttach {
    /// Image.
    pub image_id: Id,
}

/// A VM snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmSnapshot {
    /// Id (the storage snapshot's).
    pub id: Id,
    /// Name after `@`.
    pub name: String,
    /// Created.
    pub created_at: Timestamp,
    /// Used across every disk.
    pub used_bytes: u64,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /vms/{id}/snapshots`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmSnapshotCreate {
    /// Name.
    pub name: String,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}
