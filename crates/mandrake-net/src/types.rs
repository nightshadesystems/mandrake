//! What the driver reads and accepts. Wire shapes live in
//! `mandrake_core::network`; these carry only what illumos knows.

use mandrake_core::{
    network::{
        AddressFamily, AddressKind, AggrInfo, Duplex, LacpMode, LacpTimer, LinkKind, LinkState,
        MacMode, RouteKind,
    },
    shell::ShellError,
};

pub use mandrake_core::shell::FailureKind;

/// Driver result.
pub type Result<T> = std::result::Result<T, NetError>;

/// Why an operation failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum NetError {
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
    /// The request cannot be expressed to the tools.
    #[error("{0}")]
    Unsupported(String),
}

impl NetError {
    /// Classify from the tool's message.
    pub fn kind(&self) -> FailureKind {
        let text = match self {
            Self::NotFound(_) => return FailureKind::NotFound,
            Self::Parse { .. } => return FailureKind::Other,
            Self::Unsupported(_) => return FailureKind::Invalid,
            Self::Command(e) => e.stderr().to_ascii_lowercase(),
        };
        if text.contains("not found")
            || text.contains("does not exist")
            || text.contains("not in table")
            || text.contains("no such")
        {
            FailureKind::NotFound
        } else if text.contains("exists") {
            FailureKind::Exists
        } else if text.contains("busy") || text.contains("in use") || text.contains("dependent") {
            FailureKind::Conflict
        } else if text.contains("permission denied")
            || text.contains("insufficient privileges")
            || text.contains("not authorized")
            || text.contains("must be root")
        {
            FailureKind::Forbidden
        } else if text.contains("invalid")
            || text.contains("usage:")
            || text.contains("unreachable")
            || text.contains("bad ")
            || text.contains("must be")
            || text.contains("unknown")
        {
            FailureKind::Invalid
        } else {
            FailureKind::Other
        }
    }
}

/// A datalink as `dladm` describes it, joined across `show-link`,
/// `show-phys`, `show-aggr`, `show-vlan`, and `show-vnic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkInfo {
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: LinkKind,
    /// State.
    pub state: LinkState,
    /// What it sits on: ports for an aggr, one link otherwise, none for
    /// physical links and etherstubs.
    pub over: Vec<String>,
    /// MTU.
    pub mtu: Option<u32>,
    /// MAC, colon-separated lowercase.
    pub mac: Option<String>,
    /// VNICs: how the MAC was chosen.
    pub mac_mode: Option<MacMode>,
    /// VLAN id.
    pub vid: Option<u16>,
    /// Speed.
    pub speed_mbps: Option<u32>,
    /// Duplex.
    pub duplex: Option<Duplex>,
    /// Driver instance behind a physical link.
    pub device: Option<String>,
    /// Media.
    pub media: Option<String>,
    /// Aggregation details.
    pub aggr: Option<AggrInfo>,
    /// Zone the link is assigned to.
    pub zone: Option<String>,
}

impl LinkInfo {
    /// A link with nothing but a name and a kind known.
    pub fn new(name: &str, kind: LinkKind) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            state: LinkState::Unknown,
            over: Vec::new(),
            mtu: None,
            mac: None,
            mac_mode: None,
            vid: None,
            speed_mbps: None,
            duplex: None,
            device: None,
            media: None,
            aggr: None,
            zone: None,
        }
    }
}

/// An IP interface as `ipadm show-if` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceInfo {
    /// Name (the link name, or `lo0`).
    pub name: String,
    /// `ip`, `loopback`, `ipmp`, `vni`.
    pub class: String,
    /// `ok`, `down`, `failed`, ...
    pub state: String,
    /// IPMP group members.
    pub over: Option<String>,
}

/// An address object as `ipadm show-addr` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressInfo {
    /// `link/alias`.
    pub name: String,
    /// The IP interface.
    pub interface: String,
    /// Kind.
    pub kind: AddressKind,
    /// Family.
    pub family: AddressFamily,
    /// The address with prefix, once assigned.
    pub address: Option<String>,
    /// State.
    pub state: String,
    /// Survives reboot.
    pub persistent: bool,
}

/// The interface part of an address object name.
pub fn interface_of(addrobj: &str) -> &str {
    addrobj.split('/').next().unwrap_or(addrobj)
}

/// A routing table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteInfo {
    /// `default` or a network with prefix.
    pub destination: String,
    /// Gateway.
    pub gateway: Option<String>,
    /// Family.
    pub family: AddressFamily,
    /// Interface.
    pub interface: Option<String>,
    /// As `netstat -rn` prints them.
    pub flags: Option<String>,
    /// Kind.
    pub kind: RouteKind,
    /// Survives reboot.
    pub persistent: bool,
}

/// What to create an aggregation from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggrSpec {
    /// Name.
    pub name: String,
    /// Ports.
    pub ports: Vec<String>,
    /// Policy.
    pub policy: String,
    /// LACP mode.
    pub lacp_mode: LacpMode,
    /// LACP timer.
    pub lacp_timer: LacpTimer,
}

/// What to create a VLAN from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlanSpec {
    /// Name.
    pub name: String,
    /// VLAN id.
    pub vid: u16,
    /// Underlying link.
    pub over: String,
}

/// What to create a VNIC from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VnicSpec {
    /// Name.
    pub name: String,
    /// Underlying link.
    pub over: String,
    /// Pinned MAC.
    pub mac: Option<String>,
    /// VLAN tag.
    pub vid: Option<u16>,
    /// MTU.
    pub mtu: Option<u32>,
}

/// What to create an address from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressSpec {
    /// `link/alias`.
    pub addrobj: String,
    /// Kind.
    pub kind: AddressKind,
    /// The address with prefix; required for `static`.
    pub address: Option<String>,
    /// Do not persist.
    pub temporary: bool,
}

impl AddressSpec {
    /// The IP interface.
    pub fn interface(&self) -> &str {
        interface_of(&self.addrobj)
    }
}

/// A static route to add or delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSpec {
    /// `default` or a network with prefix.
    pub destination: String,
    /// Gateway.
    pub gateway: String,
    /// Family.
    pub family: AddressFamily,
}

impl RouteSpec {
    /// A route whose family follows the gateway.
    pub fn new(destination: &str, gateway: &str) -> Self {
        Self {
            destination: destination.to_owned(),
            gateway: gateway.to_owned(),
            family: AddressFamily::of(gateway),
        }
    }
}
