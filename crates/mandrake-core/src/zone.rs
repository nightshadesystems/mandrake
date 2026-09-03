//! Zone wire types mirroring the `zones` family in `api/openapi.yaml`.

// `Option<Option<T>>` is the contract's three-state PATCH field: absent,
// explicit null (clear), or a value.
#![allow(clippy::option_option)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp, api::Metadata};

/// OmniOS zone brands Mandrake manages here; `ipkg`, `lipkg`, and
/// `sparse` are native, `bhyve` is the VM family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneBrand {
    /// Full native zone with its own packages.
    Ipkg,
    /// Native zone linked to the global zone's packages.
    Lipkg,
    /// Native zone sharing the global zone's `/usr`.
    Sparse,
    /// Linux.
    Lx,
}

impl ZoneBrand {
    /// From the `zoneadm`/`zonecfg` brand name.
    pub fn from_brand(s: &str) -> Option<Self> {
        match s {
            "ipkg" => Some(Self::Ipkg),
            "lipkg" => Some(Self::Lipkg),
            "sparse" => Some(Self::Sparse),
            "lx" => Some(Self::Lx),
            _ => None,
        }
    }

    /// As illumos spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ipkg => "ipkg",
            Self::Lipkg => "lipkg",
            Self::Sparse => "sparse",
            Self::Lx => "lx",
        }
    }

    /// Native brands install from packages; lx needs an image.
    pub const fn is_native(self) -> bool {
        !matches!(self, Self::Lx)
    }
}

impl fmt::Display for ZoneBrand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Zone state as `zoneadm list` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneState {
    /// Configured, not installed.
    Configured,
    /// Install in progress or failed.
    Incomplete,
    /// Installed, not running.
    Installed,
    /// Ready, not booted.
    Ready,
    /// Running.
    Running,
    /// Shutting down.
    ShuttingDown,
    /// Down.
    Down,
    /// Unavailable.
    Unavailable,
}

impl ZoneState {
    /// From the `zoneadm` state column; `mounted` counts as installed.
    pub fn from_zoneadm(s: &str) -> Self {
        match s {
            "configured" => Self::Configured,
            "incomplete" => Self::Incomplete,
            "installed" | "mounted" => Self::Installed,
            "ready" => Self::Ready,
            "running" => Self::Running,
            "shutting_down" => Self::ShuttingDown,
            "down" => Self::Down,
            _ => Self::Unavailable,
        }
    }

    /// As `zoneadm` spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Incomplete => "incomplete",
            Self::Installed => "installed",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::ShuttingDown => "shutting_down",
            Self::Down => "down",
            Self::Unavailable => "unavailable",
        }
    }
}

impl fmt::Display for ZoneState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A zonecfg `anet` resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneNic {
    /// Link name inside the zone.
    pub name: String,
    /// Link beneath it.
    pub over: String,
    /// Pinned MAC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    /// VLAN tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<u16>,
    /// Address with prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Default router.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
}

/// A zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    /// Id (the `mandrake-id` attribute).
    pub id: Id,
    /// Name.
    pub name: String,
    /// Brand.
    pub brand: ZoneBrand,
    /// State.
    pub state: ZoneState,
    /// Image it was cloned from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
    /// Pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Dataset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<String>,
    /// Zonepath.
    pub zonepath: String,
    /// NICs.
    pub nics: Vec<ZoneNic>,
    /// CPU cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cap: Option<f64>,
    /// Memory cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_cap_bytes: Option<u64>,
    /// Autoboot.
    pub autoboot: bool,
    /// Hostname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Resolvers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolvers: Vec<String>,
    /// Created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// Mandrake metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /zones`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneCreate {
    /// Name.
    pub name: String,
    /// Brand.
    pub brand: ZoneBrand,
    /// Image to clone; required for lx.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
    /// Pool for the zone dataset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// NICs.
    #[serde(default)]
    pub nics: Vec<ZoneNic>,
    /// CPU cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cap: Option<f64>,
    /// Memory cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_cap_bytes: Option<u64>,
    /// Autoboot.
    #[serde(default = "default_true")]
    pub autoboot: bool,
    /// Boot once installed.
    #[serde(default = "default_true")]
    pub start: bool,
    /// Hostname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Resolvers.
    #[serde(default)]
    pub resolvers: Vec<String>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

fn default_true() -> bool {
    true
}

/// `PATCH /zones/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ZoneUpdate {
    /// Replaces the NIC list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nics: Option<Vec<ZoneNic>>,
    /// CPU cap; explicit null removes it.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu_cap: Option<Option<f64>>,
    /// Memory cap; explicit null removes it.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub memory_cap_bytes: Option<Option<u64>>,
    /// Autoboot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoboot: Option<bool>,
    /// Hostname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Resolvers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolvers: Option<Vec<String>>,
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

/// `POST /zones/{id}/stop`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneStop {
    /// `zoneadm halt` instead of a clean shutdown.
    #[serde(default)]
    pub force: bool,
}
