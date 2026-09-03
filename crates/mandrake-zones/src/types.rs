//! What the driver reads and accepts. Wire shapes live in
//! `mandrake_core::zone`; these carry what `zonecfg` and `zoneadm` know.

use std::collections::BTreeMap;

use mandrake_core::{
    shell::ShellError,
    zone::{ZoneNic, ZoneState},
};

pub use mandrake_core::shell::FailureKind;

/// Driver result.
pub type Result<T> = std::result::Result<T, ZoneError>;

/// Why an operation failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ZoneError {
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
    /// The named zone is not there.
    #[error("zone {0} does not exist")]
    NotFound(String),
    /// The request cannot be expressed to the tools.
    #[error("{0}")]
    Unsupported(String),
}

impl ZoneError {
    /// Classify from the tool's message.
    pub fn kind(&self) -> FailureKind {
        let text = match self {
            Self::NotFound(_) => return FailureKind::NotFound,
            Self::Parse { .. } => return FailureKind::Other,
            Self::Unsupported(_) => return FailureKind::Invalid,
            Self::Command(e) => e.stderr().to_ascii_lowercase(),
        };
        if text.contains("no such zone") || text.contains("does not exist") {
            FailureKind::NotFound
        } else if text.contains("already exists")
            || text.contains("already installed")
            || text.contains("already running")
            || text.contains("already booted")
        {
            FailureKind::Exists
        } else if text.contains("is running")
            || text.contains("must be halted")
            || text.contains("not installed")
            || text.contains("is installed")
            || text.contains("cannot boot")
            || text.contains("busy")
            || text.contains("in use")
        {
            FailureKind::Conflict
        } else if text.contains("permission denied")
            || text.contains("not authorized")
            || text.contains("must be root")
            || text.contains("insufficient privileges")
        {
            FailureKind::Forbidden
        } else if text.contains("invalid")
            || text.contains("usage:")
            || text.contains("bad ")
            || text.contains("unknown")
            || text.contains("no such property")
        {
            FailureKind::Invalid
        } else {
            FailureKind::Other
        }
    }
}

/// One `zoneadm list -pc` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneSummary {
    /// Name.
    pub name: String,
    /// State.
    pub state: ZoneState,
    /// Brand as illumos spells it (`ipkg`, `lx`, `bhyve`, ...).
    pub brand: String,
    /// Zonepath.
    pub zonepath: String,
    /// Zone UUID from illumos, if assigned.
    pub uuid: Option<String>,
    /// Exclusive IP stack.
    pub exclusive_ip: bool,
}

/// A zone's configuration as `zonecfg export` describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZoneConfig {
    /// Name.
    pub name: String,
    /// Brand.
    pub brand: String,
    /// Zonepath.
    pub zonepath: String,
    /// Autoboot.
    pub autoboot: bool,
    /// `ip-type`.
    pub ip_type: String,
    /// `anet` resources.
    pub nics: Vec<ZoneNic>,
    /// `capped-cpu` ncpus.
    pub cpu_cap: Option<f64>,
    /// `capped-memory` physical, in bytes.
    pub memory_cap: Option<u64>,
    /// String attributes by name.
    pub attrs: BTreeMap<String, String>,
    /// Delegated datasets.
    pub datasets: Vec<String>,
    /// Resources Mandrake does not model, kept verbatim for reference.
    pub other: Vec<String>,
}

/// What to configure a zone from; the managed subset of a configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneSpec {
    /// Name.
    pub name: String,
    /// Brand.
    pub brand: String,
    /// Zonepath.
    pub zonepath: String,
    /// Autoboot.
    pub autoboot: bool,
    /// NICs.
    pub nics: Vec<ZoneNic>,
    /// CPU cap.
    pub cpu_cap: Option<f64>,
    /// Memory cap in bytes.
    pub memory_cap: Option<u64>,
    /// String attributes to set. On update, managed keys absent here are
    /// removed (see [`crate::parse::MANAGED_ATTRS`]).
    pub attrs: BTreeMap<String, String>,
}

/// Where `zoneadm install` gets the zone's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// From the global zone's packages (native brands).
    Packages,
    /// From a tarball or ZFS stream file (`-s`).
    Archive(String),
    /// The zonepath dataset is already populated (a clone of an image).
    Prepared,
}
