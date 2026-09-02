//! Network wire types mirroring the `network` family in `api/openapi.yaml`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, api::Metadata};

/// What a datalink is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    /// Physical port.
    Phys,
    /// Link aggregation.
    Aggr,
    /// VLAN on another link.
    Vlan,
    /// Virtual switch.
    Etherstub,
    /// Virtual NIC.
    Vnic,
    /// Anything else `dladm` knows (bridge, iptun, part, ...).
    Other,
}

impl LinkKind {
    /// From the `dladm show-link` CLASS column.
    pub fn from_class(class: &str) -> Self {
        match class {
            "phys" => Self::Phys,
            "aggr" => Self::Aggr,
            "vlan" => Self::Vlan,
            "etherstub" => Self::Etherstub,
            "vnic" => Self::Vnic,
            _ => Self::Other,
        }
    }

    /// As the API and `dladm` spell it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Phys => "phys",
            Self::Aggr => "aggr",
            Self::Vlan => "vlan",
            Self::Etherstub => "etherstub",
            Self::Vnic => "vnic",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for LinkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Link state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkState {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Not reported.
    Unknown,
}

impl LinkState {
    /// From the `dladm` STATE column.
    pub fn from_dladm(s: &str) -> Self {
        match s {
            "up" => Self::Up,
            "down" => Self::Down,
            _ => Self::Unknown,
        }
    }
}

/// How a VNIC got its MAC address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacMode {
    /// Chosen by the system.
    Auto,
    /// Given by the administrator.
    Fixed,
    /// Randomly generated.
    Random,
    /// A factory address of the underlying port.
    Factory,
}

impl MacMode {
    /// From the `dladm show-vnic` MACADDRTYPE column (`factory, slot 1`
    /// included).
    pub fn from_dladm(s: &str) -> Self {
        match s.split(',').next().unwrap_or(s).trim() {
            "fixed" => Self::Fixed,
            "random" => Self::Random,
            "factory" => Self::Factory,
            _ => Self::Auto,
        }
    }
}

/// Duplex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Duplex {
    /// Full.
    Full,
    /// Half.
    Half,
    /// Not reported.
    Unknown,
}

impl Duplex {
    /// From the `dladm` DUPLEX column.
    pub fn from_dladm(s: &str) -> Self {
        match s {
            "full" => Self::Full,
            "half" => Self::Half,
            _ => Self::Unknown,
        }
    }
}

/// LACP activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LacpMode {
    /// No LACP.
    Off,
    /// Active.
    Active,
    /// Passive.
    Passive,
}

impl LacpMode {
    /// As `dladm` spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Active => "active",
            Self::Passive => "passive",
        }
    }

    /// From the `dladm show-aggr` LACPACTIVITY column.
    pub fn from_dladm(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "passive" => Self::Passive,
            _ => Self::Off,
        }
    }
}

/// LACP timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LacpTimer {
    /// Short.
    Short,
    /// Long.
    Long,
}

impl LacpTimer {
    /// As `dladm` spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Long => "long",
        }
    }

    /// From the `dladm show-aggr` LACPTIMER column.
    pub fn from_dladm(s: &str) -> Self {
        if s == "long" { Self::Long } else { Self::Short }
    }
}

/// One port of an aggregation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggrPort {
    /// Link name.
    pub name: String,
    /// `attached`, `standby`, or as `dladm show-aggr -x` prints it.
    pub state: String,
    /// Speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_mbps: Option<u32>,
}

/// Aggregation details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggrInfo {
    /// `L2`, `L3`, `L4`, or a combination.
    pub policy: String,
    /// LACP mode.
    pub lacp_mode: LacpMode,
    /// LACP timer.
    pub lacp_timer: LacpTimer,
    /// Ports.
    pub ports: Vec<AggrPort>,
}

/// One datalink. Fields not meaningful for a kind are absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// Id (derived from the name, ADR-0011).
    pub id: Id,
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: LinkKind,
    /// State.
    pub state: LinkState,
    /// Links this one sits on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub over: Vec<String>,
    /// MTU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    /// MAC address, colon-separated lowercase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    /// VNICs: how the MAC was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_mode: Option<MacMode>,
    /// VLAN id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<u16>,
    /// Speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_mbps: Option<u32>,
    /// Duplex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplex: Option<Duplex>,
    /// Driver instance behind a physical link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Media.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    /// Aggregation details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggr: Option<AggrInfo>,
    /// Zone the link is assigned to, if not the global zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    /// Part of the path to the management address.
    pub protected: bool,
    /// Mandrake metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `PATCH /network/links/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkUpdate {
    /// New MTU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    /// Metadata patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /network/aggrs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggrCreate {
    /// Name.
    pub name: String,
    /// Ports.
    pub ports: Vec<String>,
    /// Policy, default `L4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// LACP mode, default `active`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lacp_mode: Option<LacpMode>,
    /// LACP timer, default `short`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lacp_timer: Option<LacpTimer>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /network/vlans`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VlanCreate {
    /// Name.
    pub name: String,
    /// VLAN id.
    pub vid: u16,
    /// Underlying link.
    pub over: String,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /network/etherstubs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtherstubCreate {
    /// Name.
    pub name: String,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /network/vnics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VnicCreate {
    /// Name.
    pub name: String,
    /// Underlying link.
    pub over: String,
    /// Pinned MAC; omitted means auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    /// VLAN tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<u16>,
    /// MTU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// How an address is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressKind {
    /// Configured.
    Static,
    /// DHCP.
    Dhcp,
    /// IPv6 autoconfiguration.
    Addrconf,
}

impl AddressKind {
    /// As `ipadm` spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Dhcp => "dhcp",
            Self::Addrconf => "addrconf",
        }
    }

    /// From the `ipadm show-addr` TYPE column.
    pub fn from_ipadm(s: &str) -> Option<Self> {
        match s {
            "static" => Some(Self::Static),
            "dhcp" => Some(Self::Dhcp),
            "addrconf" => Some(Self::Addrconf),
            _ => None,
        }
    }
}

impl fmt::Display for AddressKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Address family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressFamily {
    /// IPv4.
    Inet,
    /// IPv6.
    Inet6,
}

impl AddressFamily {
    /// Guess from an address literal.
    pub fn of(address: &str) -> Self {
        if address.contains(':') {
            Self::Inet6
        } else {
            Self::Inet
        }
    }

    /// As `route` and `ipadm` spell it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inet => "inet",
            Self::Inet6 => "inet6",
        }
    }
}

impl fmt::Display for AddressFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One address object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    /// Id (derived from the name, ADR-0011).
    pub id: Id,
    /// The address object, for example `vnic0/v4`.
    pub name: String,
    /// The IP interface it belongs to.
    pub interface: String,
    /// Kind.
    pub kind: AddressKind,
    /// Family.
    pub family: AddressFamily,
    /// `a.b.c.d/prefix` or `xx::/prefix`; absent until assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// `ok`, `tentative`, `duplicate`, `inaccessible`, `disabled`, ...
    pub state: String,
    /// Survives reboot.
    #[serde(default)]
    pub persistent: bool,
    /// The address the daemon listens on.
    pub protected: bool,
    /// Mandrake metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /network/addresses`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressCreate {
    /// Link name; the IP interface is created if missing.
    pub interface: String,
    /// Kind.
    pub kind: AddressKind,
    /// Required for `static`, with prefix length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The part after `/` in the address object name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Do not persist across reboot.
    #[serde(default)]
    pub temporary: bool,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Where a route came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteKind {
    /// Added by an administrator; the only kind Mandrake manages.
    Static,
    /// Implied by an interface address.
    Interface,
    /// Learned (redirect or routing daemon).
    Dynamic,
}

/// One routing table entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    /// Id (derived from family, destination, and gateway, ADR-0011).
    pub id: Id,
    /// `default` or a network with prefix.
    pub destination: String,
    /// Gateway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    /// Family.
    pub family: AddressFamily,
    /// Interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// As `netstat -rn` prints them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    /// Kind.
    pub kind: RouteKind,
    /// Survives reboot.
    #[serde(default)]
    pub persistent: bool,
}

/// `POST /network/routes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCreate {
    /// `default` or a network with prefix.
    pub destination: String,
    /// Gateway.
    pub gateway: String,
}
