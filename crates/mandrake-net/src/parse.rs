//! Parsers for `dladm`, `ipadm`, `netstat`, and `route` output. Pure
//! functions; tested against `testdata/`.

use mandrake_core::network::{
    AddressFamily, AddressKind, AggrInfo, AggrPort, Duplex, LacpMode, LacpTimer, LinkKind,
    LinkState, MacMode, RouteKind,
};

use crate::types::{AddressInfo, InterfaceInfo, LinkInfo, NetError, RouteInfo, interface_of};

fn parse_err(command: &str, detail: impl Into<String>) -> NetError {
    NetError::Parse {
        command: command.to_owned(),
        detail: detail.into(),
    }
}

/// Split one line of parsable (`-p`) output on unescaped `:`, undoing the
/// `\:` and `\\` escapes `dladm` and `ipadm` apply.
pub fn fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ':' => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// `--`, `-`, `?`, and empty mean not applicable.
fn opt(field: &str) -> Option<&str> {
    match field {
        "" | "--" | "-" | "?" | "none" => None,
        s => Some(s),
    }
}

fn rows(out: &str, command: &str, want: usize) -> Result<Vec<Vec<String>>, NetError> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f = fields(line);
            if f.len() < want {
                Err(parse_err(
                    command,
                    format!("expected {want} fields: {line}"),
                ))
            } else {
                Ok(f)
            }
        })
        .collect()
}

/// `0:c:29:ab:cd:ef` to `00:0c:29:ab:cd:ef`.
pub fn normalize_mac(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = Vec::with_capacity(6);
    for p in parts {
        let b = u8::from_str_radix(p, 16).ok()?;
        out.push(format!("{b:02x}"));
    }
    Some(out.join(":"))
}

/// `1000`, `1000Mb`, or `10Gb` to Mbps; zero or unknown to `None`.
pub fn speed_mbps(s: &str) -> Option<u32> {
    let s = opt(s)?.trim();
    let (digits, mult) = s
        .strip_suffix("Gb")
        .map_or_else(|| (s.strip_suffix("Mb").unwrap_or(s), 1), |d| (d, 1000));
    let n: u32 = digits.trim().parse().ok()?;
    (n > 0).then(|| n.checked_mul(mult))?
}

fn duplex(s: &str) -> Option<Duplex> {
    opt(s).map(Duplex::from_dladm)
}

// ------------------------------------------------------------ dladm

/// Columns for `dladm show-link -p -o`.
pub const SHOW_LINK_COLUMNS: &str = "link,class,mtu,state,bridge,over";

/// One `dladm show-link` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRow {
    /// Name.
    pub name: String,
    /// Kind from CLASS.
    pub kind: LinkKind,
    /// MTU.
    pub mtu: Option<u32>,
    /// State.
    pub state: LinkState,
    /// OVER, split on spaces and commas.
    pub over: Vec<String>,
}

/// Parse `dladm show-link -p -o` with [`SHOW_LINK_COLUMNS`].
pub fn show_link(out: &str) -> Result<Vec<LinkRow>, NetError> {
    Ok(rows(out, "dladm show-link", 6)?
        .into_iter()
        .map(|f| LinkRow {
            name: f[0].clone(),
            kind: LinkKind::from_class(&f[1]),
            mtu: opt(&f[2]).and_then(|m| m.parse().ok()),
            state: LinkState::from_dladm(&f[3]),
            over: opt(&f[5])
                .map(|o| {
                    o.split([' ', ','])
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

/// Columns for `dladm show-phys -p -o`.
pub const SHOW_PHYS_COLUMNS: &str = "link,media,state,speed,duplex,device";

/// One `dladm show-phys` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysRow {
    /// Name.
    pub name: String,
    /// Media.
    pub media: Option<String>,
    /// State.
    pub state: LinkState,
    /// Speed.
    pub speed_mbps: Option<u32>,
    /// Duplex.
    pub duplex: Option<Duplex>,
    /// Device.
    pub device: Option<String>,
}

/// Parse `dladm show-phys -p -o` with [`SHOW_PHYS_COLUMNS`].
pub fn show_phys(out: &str) -> Result<Vec<PhysRow>, NetError> {
    Ok(rows(out, "dladm show-phys", 6)?
        .into_iter()
        .map(|f| PhysRow {
            name: f[0].clone(),
            media: opt(&f[1]).map(str::to_owned),
            state: LinkState::from_dladm(&f[2]),
            speed_mbps: speed_mbps(&f[3]),
            duplex: duplex(&f[4]),
            device: opt(&f[5]).map(str::to_owned),
        })
        .collect())
}

/// Columns for `dladm show-phys -m -p -o`.
pub const SHOW_PHYS_MAC_COLUMNS: &str = "link,slot,address,inuse,client";

/// Parse `dladm show-phys -m -p -o` with [`SHOW_PHYS_MAC_COLUMNS`] to
/// `(link, mac)` using each link's primary slot.
pub fn show_phys_macs(out: &str) -> Result<Vec<(String, String)>, NetError> {
    let mut macs: Vec<(String, String)> = Vec::new();
    for f in rows(out, "dladm show-phys -m", 5)? {
        let Some(raw) = opt(&f[2]) else { continue };
        let mac = normalize_mac(raw).unwrap_or_else(|| raw.to_ascii_lowercase());
        let seen = macs.iter().position(|(n, _)| *n == f[0]);
        match seen {
            Some(i) if f[1] == "primary" => macs[i].1 = mac,
            Some(_) => {}
            None => macs.push((f[0].clone(), mac)),
        }
    }
    Ok(macs)
}

/// Columns for `dladm show-aggr -p -o`.
pub const SHOW_AGGR_COLUMNS: &str = "link,policy,addrpolicy,lacpactivity,lacptimer,flags";

/// One `dladm show-aggr` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggrRow {
    /// Name.
    pub name: String,
    /// Policy.
    pub policy: String,
    /// LACP mode.
    pub lacp_mode: LacpMode,
    /// LACP timer.
    pub lacp_timer: LacpTimer,
}

/// Parse `dladm show-aggr -p -o` with [`SHOW_AGGR_COLUMNS`].
pub fn show_aggr(out: &str) -> Result<Vec<AggrRow>, NetError> {
    Ok(rows(out, "dladm show-aggr", 6)?
        .into_iter()
        .map(|f| AggrRow {
            name: f[0].clone(),
            policy: f[1].clone(),
            lacp_mode: LacpMode::from_dladm(&f[3]),
            lacp_timer: LacpTimer::from_dladm(&f[4]),
        })
        .collect())
}

/// Columns for `dladm show-aggr -x -p -o`.
pub const SHOW_AGGR_PORT_COLUMNS: &str = "link,port,speed,duplex,state,address,portstate";

/// One `dladm show-aggr -x` row: the aggregation itself when `port` is
/// `None`, one of its ports otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggrPortRow {
    /// Aggregation.
    pub aggr: String,
    /// Port.
    pub port: Option<String>,
    /// Speed.
    pub speed_mbps: Option<u32>,
    /// Duplex.
    pub duplex: Option<Duplex>,
    /// State.
    pub state: String,
    /// MAC.
    pub mac: Option<String>,
    /// Port state.
    pub port_state: Option<String>,
}

/// Parse `dladm show-aggr -x -p -o` with [`SHOW_AGGR_PORT_COLUMNS`]. Port
/// rows may leave LINK empty; they belong to the last aggregation seen.
pub fn show_aggr_ports(out: &str) -> Result<Vec<AggrPortRow>, NetError> {
    let mut current = String::new();
    let mut ports = Vec::new();
    for f in rows(out, "dladm show-aggr -x", 7)? {
        if !f[0].is_empty() {
            current.clone_from(&f[0]);
        }
        ports.push(AggrPortRow {
            aggr: current.clone(),
            port: opt(&f[1]).map(str::to_owned),
            speed_mbps: speed_mbps(&f[2]),
            duplex: duplex(&f[3]),
            state: f[4].clone(),
            mac: opt(&f[5]).map(|m| normalize_mac(m).unwrap_or_else(|| m.to_ascii_lowercase())),
            port_state: opt(&f[6]).map(str::to_owned),
        });
    }
    Ok(ports)
}

/// Columns for `dladm show-vlan -p -o`.
pub const SHOW_VLAN_COLUMNS: &str = "link,vid,over,flags";

/// One `dladm show-vlan` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlanRow {
    /// Name.
    pub name: String,
    /// VLAN id.
    pub vid: u16,
    /// Underlying link.
    pub over: String,
}

/// Parse `dladm show-vlan -p -o` with [`SHOW_VLAN_COLUMNS`].
pub fn show_vlan(out: &str) -> Result<Vec<VlanRow>, NetError> {
    rows(out, "dladm show-vlan", 4)?
        .into_iter()
        .map(|f| {
            Ok(VlanRow {
                name: f[0].clone(),
                vid: f[1]
                    .parse()
                    .map_err(|_| parse_err("dladm show-vlan", format!("bad vid: {}", f[1])))?,
                over: f[2].clone(),
            })
        })
        .collect()
}

/// Columns for `dladm show-vnic -p -o`.
pub const SHOW_VNIC_COLUMNS: &str = "link,over,speed,macaddress,macaddrtype,vid,zone";

/// One `dladm show-vnic` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VnicRow {
    /// Name.
    pub name: String,
    /// Underlying link.
    pub over: String,
    /// Speed.
    pub speed_mbps: Option<u32>,
    /// MAC.
    pub mac: Option<String>,
    /// How the MAC was chosen.
    pub mac_mode: MacMode,
    /// VLAN tag, `None` for untagged.
    pub vid: Option<u16>,
    /// Zone.
    pub zone: Option<String>,
}

/// Parse `dladm show-vnic -p -o` with [`SHOW_VNIC_COLUMNS`].
pub fn show_vnic(out: &str) -> Result<Vec<VnicRow>, NetError> {
    Ok(rows(out, "dladm show-vnic", 7)?
        .into_iter()
        .map(|f| VnicRow {
            name: f[0].clone(),
            over: f[1].clone(),
            speed_mbps: speed_mbps(&f[2]),
            mac: opt(&f[3]).map(|m| normalize_mac(m).unwrap_or_else(|| m.to_ascii_lowercase())),
            mac_mode: MacMode::from_dladm(&f[4]),
            vid: opt(&f[5])
                .and_then(|v| v.parse::<u16>().ok())
                .filter(|v| *v != 0),
            zone: opt(&f[6]).map(str::to_owned),
        })
        .collect())
}

/// Everything `list_links` reads, joined by [`LinkSources::assemble`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkSources {
    /// `show-link`.
    pub links: Vec<LinkRow>,
    /// `show-phys`.
    pub phys: Vec<PhysRow>,
    /// `show-phys -m`.
    pub macs: Vec<(String, String)>,
    /// `show-aggr`.
    pub aggrs: Vec<AggrRow>,
    /// `show-aggr -x`.
    pub ports: Vec<AggrPortRow>,
    /// `show-vlan`.
    pub vlans: Vec<VlanRow>,
    /// `show-vnic`.
    pub vnics: Vec<VnicRow>,
}

impl LinkSources {
    /// One [`LinkInfo`] per `show-link` row, in `show-link` order.
    pub fn assemble(&self) -> Vec<LinkInfo> {
        let mut out: Vec<LinkInfo> = self
            .links
            .iter()
            .map(|r| {
                let mut l = LinkInfo::new(&r.name, r.kind);
                l.state = r.state;
                l.mtu = r.mtu;
                l.over.clone_from(&r.over);
                l
            })
            .collect();
        for l in &mut out {
            match l.kind {
                LinkKind::Phys => self.fill_phys(l),
                LinkKind::Aggr => self.fill_aggr(l),
                LinkKind::Vlan => self.fill_vlan(l),
                LinkKind::Vnic => self.fill_vnic(l),
                LinkKind::Etherstub | LinkKind::Other => {}
            }
        }
        // A VLAN shares its parent's MAC.
        let macs: Vec<(String, Option<String>)> = out
            .iter()
            .map(|l| (l.name.clone(), l.mac.clone()))
            .collect();
        for l in out.iter_mut().filter(|l| l.kind == LinkKind::Vlan) {
            if let Some(parent) = l.over.first() {
                l.mac = macs
                    .iter()
                    .find(|(n, _)| n == parent)
                    .and_then(|(_, m)| m.clone());
            }
        }
        out
    }

    fn fill_phys(&self, l: &mut LinkInfo) {
        if let Some(p) = self.phys.iter().find(|p| p.name == l.name) {
            l.media.clone_from(&p.media);
            l.speed_mbps = p.speed_mbps;
            l.duplex = p.duplex;
            l.device.clone_from(&p.device);
            if p.state != LinkState::Unknown {
                l.state = p.state;
            }
        }
        l.mac = self
            .macs
            .iter()
            .find(|(n, _)| *n == l.name)
            .map(|(_, m)| m.clone());
    }

    fn fill_aggr(&self, l: &mut LinkInfo) {
        if let Some(a) = self.aggrs.iter().find(|a| a.name == l.name) {
            let ports: Vec<AggrPort> = self
                .ports
                .iter()
                .filter(|p| p.aggr == l.name)
                .filter_map(|p| {
                    p.port.as_ref().map(|name| AggrPort {
                        name: name.clone(),
                        state: p.port_state.clone().unwrap_or_else(|| p.state.clone()),
                        speed_mbps: p.speed_mbps,
                    })
                })
                .collect();
            if !ports.is_empty() {
                l.over = ports.iter().map(|p| p.name.clone()).collect();
            }
            l.aggr = Some(AggrInfo {
                policy: a.policy.clone(),
                lacp_mode: a.lacp_mode,
                lacp_timer: a.lacp_timer,
                ports,
            });
        }
        if let Some(own) = self
            .ports
            .iter()
            .find(|p| p.aggr == l.name && p.port.is_none())
        {
            l.speed_mbps = own.speed_mbps;
            l.duplex = own.duplex;
            l.mac.clone_from(&own.mac);
        }
    }

    fn fill_vlan(&self, l: &mut LinkInfo) {
        if let Some(v) = self.vlans.iter().find(|v| v.name == l.name) {
            l.vid = Some(v.vid);
            l.over = vec![v.over.clone()];
        }
    }

    fn fill_vnic(&self, l: &mut LinkInfo) {
        if let Some(v) = self.vnics.iter().find(|v| v.name == l.name) {
            l.over = vec![v.over.clone()];
            l.speed_mbps = v.speed_mbps;
            l.mac.clone_from(&v.mac);
            l.mac_mode = Some(v.mac_mode);
            l.vid = v.vid;
            l.zone.clone_from(&v.zone);
        }
    }
}

// ------------------------------------------------------------ ipadm

/// Columns for `ipadm show-if -p -o`.
pub const SHOW_IF_COLUMNS: &str = "ifname,class,state,current,persistent,over";

/// Parse `ipadm show-if -p -o` with [`SHOW_IF_COLUMNS`].
pub fn show_if(out: &str) -> Result<Vec<InterfaceInfo>, NetError> {
    Ok(rows(out, "ipadm show-if", 6)?
        .into_iter()
        .map(|f| InterfaceInfo {
            name: f[0].clone(),
            class: f[1].clone(),
            state: f[2].clone(),
            over: opt(&f[5]).map(str::to_owned),
        })
        .collect())
}

/// Columns for `ipadm show-addr -p -o`.
pub const SHOW_ADDR_COLUMNS: &str = "addrobj,type,state,current,persistent,addr";

/// Parse `ipadm show-addr -p -o` with [`SHOW_ADDR_COLUMNS`].
pub fn show_addr(out: &str) -> Result<Vec<AddressInfo>, NetError> {
    rows(out, "ipadm show-addr", 6)?
        .into_iter()
        .map(|f| {
            let kind = AddressKind::from_ipadm(&f[1]).ok_or_else(|| {
                parse_err("ipadm show-addr", format!("unknown address type: {}", f[1]))
            })?;
            let address = opt(&f[5]).map(str::to_owned);
            let family = address.as_deref().map_or(
                match kind {
                    AddressKind::Addrconf => AddressFamily::Inet6,
                    AddressKind::Static | AddressKind::Dhcp => AddressFamily::Inet,
                },
                AddressFamily::of,
            );
            Ok(AddressInfo {
                interface: interface_of(&f[0]).to_owned(),
                name: f[0].clone(),
                kind,
                family,
                address,
                state: f[2].clone(),
                persistent: f[4].contains('U'),
            })
        })
        .collect()
}

// ------------------------------------------------------------ routes

/// A dotted mask to a prefix length; `None` when not contiguous.
pub fn mask_to_prefix(mask: &str) -> Option<u8> {
    let ip: std::net::Ipv4Addr = mask.parse().ok()?;
    let bits = u32::from(ip);
    (bits.leading_ones() == bits.count_ones())
        .then(|| u8::try_from(bits.count_ones()).ok())
        .flatten()
}

fn is_flags(tok: &str) -> bool {
    !tok.is_empty() && tok.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_number(tok: &str) -> bool {
    !tok.is_empty() && tok.chars().all(|c| c.is_ascii_digit())
}

fn route_kind(flags: &str) -> RouteKind {
    if flags.contains('D') {
        RouteKind::Dynamic
    } else if flags.contains('G') {
        RouteKind::Static
    } else {
        RouteKind::Interface
    }
}

fn v4_destination(dest: &str, mask: &str) -> String {
    if dest == "default" || (dest == "0.0.0.0" && mask == "0.0.0.0") {
        "default".to_owned()
    } else {
        format!("{dest}/{}", mask_to_prefix(mask).unwrap_or(32))
    }
}

fn v6_destination(dest: &str) -> String {
    if dest == "default" || dest == "::/0" {
        "default".to_owned()
    } else if dest.contains('/') {
        dest.to_owned()
    } else {
        format!("{dest}/128")
    }
}

/// Parse `netstat -rnv`. Persistence is not known here; see
/// [`mark_persistent`].
pub fn netstat_routes(out: &str) -> Vec<RouteInfo> {
    let mut family = None;
    let mut routes = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('-') || t.starts_with("Destination") {
            continue;
        }
        if t.contains("IPv4") {
            family = Some(AddressFamily::Inet);
            continue;
        }
        if t.contains("IPv6") {
            family = Some(AddressFamily::Inet6);
            continue;
        }
        let Some(fam) = family else { continue };
        let tok: Vec<&str> = t.split_whitespace().collect();
        let (destination, gateway, rest) = match fam {
            AddressFamily::Inet if tok.len() >= 3 => {
                (v4_destination(tok[0], tok[1]), tok[2], &tok[3..])
            }
            AddressFamily::Inet6 if tok.len() >= 2 => (v6_destination(tok[0]), tok[1], &tok[2..]),
            AddressFamily::Inet | AddressFamily::Inet6 => continue,
        };
        let flags = rest.iter().find(|x| is_flags(x)).map(|s| (*s).to_owned());
        let interface = rest
            .iter()
            .find(|x| !is_flags(x) && !is_number(x))
            .map(|s| (*s).to_owned());
        routes.push(RouteInfo {
            destination,
            gateway: Some(gateway.to_owned()),
            family: fam,
            interface,
            kind: flags.as_deref().map_or(RouteKind::Interface, route_kind),
            flags,
            persistent: false,
        });
    }
    routes
}

/// One line of `route -p show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentRoute {
    /// Family.
    pub family: AddressFamily,
    /// Destination, normalised like [`netstat_routes`].
    pub destination: String,
    /// Gateway.
    pub gateway: String,
}

/// Parse `route -p show`.
pub fn persistent_routes(out: &str) -> Vec<PersistentRoute> {
    out.lines().filter_map(persistent_route).collect()
}

fn persistent_route(line: &str) -> Option<PersistentRoute> {
    let t = line.trim();
    let t = t.strip_prefix("persistent:").unwrap_or(t).trim();
    let mut tok = t.split_whitespace();
    let mut family = None;
    let mut netmask = None;
    let mut host = false;
    let mut is_add = false;
    let mut positional = Vec::new();
    while let Some(x) = tok.next() {
        match x {
            "-host" => host = true,
            "-inet" => family = Some(AddressFamily::Inet),
            "-inet6" => family = Some(AddressFamily::Inet6),
            "-netmask" => netmask = tok.next(),
            "-ifp" | "-ifa" => {
                tok.next();
            }
            "add" => is_add = true,
            x if x.starts_with('-') || x == "route" => {}
            x => positional.push(x),
        }
    }
    if !is_add || positional.len() < 2 {
        return None;
    }
    let gateway = positional[1].to_owned();
    let family = family.unwrap_or_else(|| AddressFamily::of(&gateway));
    let dest = positional[0];
    let destination = if dest == "default" || dest.contains('/') {
        dest.to_owned()
    } else if let Some(prefix) = netmask.and_then(mask_to_prefix) {
        format!("{dest}/{prefix}")
    } else if host || family == AddressFamily::Inet6 {
        let bits = if family == AddressFamily::Inet {
            32
        } else {
            128
        };
        format!("{dest}/{bits}")
    } else {
        dest.to_owned()
    };
    Some(PersistentRoute {
        family,
        destination,
        gateway,
    })
}

/// Flag the static routes that `route -p show` also lists.
pub fn mark_persistent(routes: &mut [RouteInfo], persistent: &[PersistentRoute]) {
    for r in routes.iter_mut().filter(|r| r.kind == RouteKind::Static) {
        r.persistent = persistent.iter().any(|p| {
            p.family == r.family
                && p.destination == r.destination
                && r.gateway.as_deref() == Some(p.gateway.as_str())
        });
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

    use super::*;

    const LINKS: &str = include_str!("../testdata/dladm-show-link-p.synthetic.txt");
    const PHYS: &str = include_str!("../testdata/dladm-show-phys-p.synthetic.txt");
    const MACS: &str = include_str!("../testdata/dladm-show-phys-m-p.synthetic.txt");
    const AGGRS: &str = include_str!("../testdata/dladm-show-aggr-p.synthetic.txt");
    const PORTS: &str = include_str!("../testdata/dladm-show-aggr-x-p.synthetic.txt");
    const VLANS: &str = include_str!("../testdata/dladm-show-vlan-p.synthetic.txt");
    const VNICS: &str = include_str!("../testdata/dladm-show-vnic-p.synthetic.txt");
    const IFS: &str = include_str!("../testdata/ipadm-show-if-p.synthetic.txt");
    const ADDRS: &str = include_str!("../testdata/ipadm-show-addr-p.synthetic.txt");
    const NETSTAT: &str = include_str!("../testdata/netstat-rnv.synthetic.txt");
    const ROUTE_P: &str = include_str!("../testdata/route-p-show.synthetic.txt");

    fn sources() -> LinkSources {
        LinkSources {
            links: show_link(LINKS).unwrap(),
            phys: show_phys(PHYS).unwrap(),
            macs: show_phys_macs(MACS).unwrap(),
            aggrs: show_aggr(AGGRS).unwrap(),
            ports: show_aggr_ports(PORTS).unwrap(),
            vlans: show_vlan(VLANS).unwrap(),
            vnics: show_vnic(VNICS).unwrap(),
        }
    }

    #[test]
    fn parsable_fields_unescape_colons() {
        assert_eq!(
            fields(r"e1000g0:primary:0\:c\:29\:ab\:cd\:ef:yes:e1000g0"),
            vec!["e1000g0", "primary", "0:c:29:ab:cd:ef", "yes", "e1000g0"]
        );
        assert_eq!(fields("a::b"), vec!["a", "", "b"]);
        assert_eq!(
            normalize_mac("0:c:29:ab:cd:ef").as_deref(),
            Some("00:0c:29:ab:cd:ef")
        );
        assert_eq!(normalize_mac("nonsense"), None);
        assert_eq!(speed_mbps("1000Mb"), Some(1000));
        assert_eq!(speed_mbps("10Gb"), Some(10_000));
        assert_eq!(speed_mbps("0"), None);
    }

    #[test]
    fn links_join_every_source() {
        let links = sources().assemble();
        let names: Vec<&str> = links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "e1000g0",
                "e1000g1",
                "e1000g2",
                "e1000g3",
                "aggr0",
                "vlan10",
                "etherstub0",
                "vnic0",
                "vnic1"
            ]
        );
        let by = |n: &str| links.iter().find(|l| l.name == n).unwrap();

        let p = by("e1000g0");
        assert_eq!(p.kind, LinkKind::Phys);
        assert_eq!(p.state, LinkState::Up);
        assert_eq!(p.mac.as_deref(), Some("00:0c:29:ab:cd:ef"));
        assert_eq!(p.speed_mbps, Some(1000));
        assert_eq!(p.duplex, Some(Duplex::Full));
        assert_eq!(p.device.as_deref(), Some("e1000g0"));
        assert_eq!(p.mtu, Some(1500));
        assert!(p.over.is_empty());
        assert_eq!(by("e1000g3").state, LinkState::Down);
        assert_eq!(by("e1000g3").speed_mbps, None);

        let a = by("aggr0");
        assert_eq!(a.kind, LinkKind::Aggr);
        assert_eq!(a.over, ["e1000g1", "e1000g2"]);
        assert_eq!(a.speed_mbps, Some(1000));
        assert_eq!(a.mac.as_deref(), Some("00:0c:29:ab:cd:f0"));
        let info = a.aggr.as_ref().unwrap();
        assert_eq!(info.policy, "L4");
        assert_eq!(info.lacp_mode, LacpMode::Active);
        assert_eq!(info.lacp_timer, LacpTimer::Short);
        assert_eq!(info.ports.len(), 2);
        assert_eq!(info.ports[1].name, "e1000g2");
        assert_eq!(info.ports[1].state, "attached");

        let v = by("vlan10");
        assert_eq!(v.vid, Some(10));
        assert_eq!(v.over, ["e1000g0"]);
        assert_eq!(v.mac.as_deref(), Some("00:0c:29:ab:cd:ef"));

        assert_eq!(by("etherstub0").kind, LinkKind::Etherstub);
        assert_eq!(by("etherstub0").mtu, Some(9000));

        let n0 = by("vnic0");
        assert_eq!(n0.over, ["etherstub0"]);
        assert_eq!(n0.mac.as_deref(), Some("02:08:20:a1:b2:c3"));
        assert_eq!(n0.mac_mode, Some(MacMode::Random));
        assert_eq!(n0.vid, None);
        assert_eq!(n0.speed_mbps, None);
        let n1 = by("vnic1");
        assert_eq!(n1.vid, Some(20));
        assert_eq!(n1.mac_mode, Some(MacMode::Fixed));
        assert_eq!(n1.speed_mbps, Some(2000));
        assert_eq!(n1.zone, None);
    }

    #[test]
    fn short_rows_are_errors() {
        let err = show_link("e1000g0:phys").err().expect("error");
        assert!(matches!(err, NetError::Parse { .. }));
        assert!(show_vlan("vlan1:abc:e1000g0:--").is_err());
    }

    #[test]
    fn interfaces_and_addresses() {
        let ifs = show_if(IFS).unwrap();
        assert_eq!(ifs.len(), 3);
        assert_eq!(ifs[0].class, "loopback");
        assert_eq!(ifs[1].name, "e1000g0");

        let addrs = show_addr(ADDRS).unwrap();
        assert_eq!(addrs.len(), 5);
        let by = |n: &str| addrs.iter().find(|a| a.name == n).unwrap();
        let m = by("e1000g0/v4");
        assert_eq!(m.interface, "e1000g0");
        assert_eq!(m.kind, AddressKind::Static);
        assert_eq!(m.family, AddressFamily::Inet);
        assert_eq!(m.address.as_deref(), Some("192.168.1.10/24"));
        assert_eq!(m.state, "ok");
        assert!(m.persistent);
        let d = by("vlan10/v4");
        assert_eq!(d.kind, AddressKind::Dhcp);
        assert_eq!(d.address.as_deref(), Some("10.10.0.5/24"));
        assert!(!d.persistent);
        assert_eq!(by("lo0/v6").address.as_deref(), Some("::1/128"));
        let ac = by("e1000g0/v6");
        assert_eq!(ac.kind, AddressKind::Addrconf);
        assert_eq!(ac.family, AddressFamily::Inet6);
        assert_eq!(ac.address.as_deref(), Some("fe80::20c:29ff:feab:cdef/10"));
    }

    #[test]
    fn routes_from_netstat_and_route_p() {
        let mut routes = netstat_routes(NETSTAT);
        assert_eq!(routes.len(), 7);
        let by =
            |rs: &[RouteInfo], d: &str| rs.iter().find(|r| r.destination == d).cloned().unwrap();
        let def = by(&routes, "default");
        assert_eq!(def.gateway.as_deref(), Some("192.168.1.1"));
        assert_eq!(def.family, AddressFamily::Inet);
        assert_eq!(def.interface.as_deref(), Some("e1000g0"));
        assert_eq!(def.flags.as_deref(), Some("UG"));
        assert_eq!(def.kind, RouteKind::Static);
        let lo = by(&routes, "127.0.0.1/32");
        assert_eq!(lo.kind, RouteKind::Interface);
        assert_eq!(lo.interface.as_deref(), Some("lo0"));
        assert_eq!(by(&routes, "192.168.1.0/24").kind, RouteKind::Interface);
        assert_eq!(by(&routes, "10.20.0.0/16").kind, RouteKind::Static);
        let lo6 = by(&routes, "::1/128");
        assert_eq!(lo6.family, AddressFamily::Inet6);
        assert_eq!(lo6.interface.as_deref(), Some("lo0"));
        let ll = by(&routes, "fe80::/10");
        assert_eq!(ll.interface.as_deref(), Some("e1000g0"));
        assert_eq!(ll.kind, RouteKind::Interface);

        let persistent = persistent_routes(ROUTE_P);
        assert_eq!(persistent.len(), 2);
        assert_eq!(persistent[1].destination, "10.20.0.0/16");
        mark_persistent(&mut routes, &persistent);
        assert!(by(&routes, "default").persistent);
        assert!(by(&routes, "10.20.0.0/16").persistent);
        assert!(!by(&routes, "192.168.1.0/24").persistent);
    }

    #[test]
    fn persistent_route_forms() {
        let r =
            persistent_route("persistent: route add -inet6 default fe80::1 -ifp e1000g0").unwrap();
        assert_eq!(r.family, AddressFamily::Inet6);
        assert_eq!(r.destination, "default");
        assert_eq!(r.gateway, "fe80::1");
        let r =
            persistent_route("persistent: route add -net 10.1.0.0 -netmask 255.255.0.0 10.0.0.1")
                .unwrap();
        assert_eq!(r.destination, "10.1.0.0/16");
        let r = persistent_route("persistent: route add -host 10.9.9.9 10.0.0.1").unwrap();
        assert_eq!(r.destination, "10.9.9.9/32");
        assert!(persistent_route("persistent: no routes").is_none());
        assert_eq!(mask_to_prefix("255.255.255.0"), Some(24));
        assert_eq!(mask_to_prefix("255.0.255.0"), None);
        assert_eq!(mask_to_prefix("0.0.0.0"), Some(0));
    }
}
