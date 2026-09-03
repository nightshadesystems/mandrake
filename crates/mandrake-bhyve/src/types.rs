//! What the mapping reads and writes. Wire shapes live in
//! `mandrake_core::vm`; these carry what the bhyve brand knows.

use std::collections::BTreeMap;

use mandrake_core::{vm::Bootrom, zone::ZoneNic};

/// Crate result.
pub type Result<T> = std::result::Result<T, BhyveError>;

/// Why a mapping failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BhyveError {
    /// A zone configuration is not a VM this crate understands.
    #[error("not a bhyve VM: {0}")]
    NotBhyve(String),
    /// An attribute did not parse.
    #[error("bad {attr} attribute: {value}")]
    Attr {
        /// Which attribute.
        attr: String,
        /// Its value.
        value: String,
    },
}

/// One disk of a VM: a zvol, by slot order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskSpec {
    /// The zvol, for example `tank/vms/vm0/disk0`.
    pub zvol: String,
    /// Boots the guest.
    pub boot: bool,
}

/// What to configure a VM from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSpec {
    /// Zone name.
    pub name: String,
    /// Zonepath.
    pub zonepath: String,
    /// vCPUs.
    pub vcpus: u32,
    /// Memory.
    pub memory_bytes: u64,
    /// Firmware.
    pub bootrom: Bootrom,
    /// ACPI.
    pub acpi: bool,
    /// VNC server on.
    pub vnc: bool,
    /// Autoboot.
    pub autoboot: bool,
    /// Disks in slot order; slot 0 first.
    pub disks: Vec<DiskSpec>,
    /// ISO file paths on the host, in slot order.
    pub cdroms: Vec<String>,
    /// NICs.
    pub nics: Vec<ZoneNic>,
    /// Other attributes to carry (`mandrake-id`, `mandrake-image`).
    pub attrs: BTreeMap<String, String>,
}

/// A VM as read back from its zone configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmConfig {
    /// Zone name.
    pub name: String,
    /// Zonepath.
    pub zonepath: String,
    /// vCPUs.
    pub vcpus: u32,
    /// Memory.
    pub memory_bytes: u64,
    /// Firmware.
    pub bootrom: Bootrom,
    /// ACPI.
    pub acpi: bool,
    /// VNC on.
    pub vnc: bool,
    /// Autoboot.
    pub autoboot: bool,
    /// Disks by slot; slots may have gaps after removals.
    pub disks: Vec<(u32, DiskSpec)>,
    /// ISO paths by slot.
    pub cdroms: Vec<(u32, String)>,
    /// NICs.
    pub nics: Vec<ZoneNic>,
    /// Attributes that are not the brand's (`mandrake-id`, ...).
    pub attrs: BTreeMap<String, String>,
}
