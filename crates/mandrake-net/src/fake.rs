//! An in-memory network stack with the same observable behaviour as the
//! real one, for route tests and for developing the console away from
//! illumos.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use mandrake_core::{
    network::{
        AddressFamily, AddressKind, AggrInfo, AggrPort, Duplex, LinkKind, LinkState, MacMode,
        RouteKind,
    },
    shell::ShellError,
};

use crate::{
    AddressInfo, AddressSpec, AggrSpec, BoxFuture, InterfaceInfo, LinkInfo, Net, NetError, Result,
    RouteInfo, RouteSpec, VlanSpec, VnicSpec, interface_of, parse,
};

#[derive(Default)]
struct State {
    links: BTreeMap<String, LinkInfo>,
    interfaces: BTreeMap<String, InterfaceInfo>,
    addresses: BTreeMap<String, AddressInfo>,
    routes: Vec<RouteInfo>,
    next_mac: u32,
}

/// The fake driver. Clone to share.
#[derive(Clone, Default)]
pub struct FakeNet {
    state: Arc<Mutex<State>>,
}

fn tool_error(message: &str) -> NetError {
    NetError::Command(ShellError::Failed {
        command: "fake".to_owned(),
        status: 1,
        stderr: message.to_owned(),
    })
}

impl FakeNet {
    /// An empty host with no links.
    pub fn new() -> Self {
        Self::default()
    }

    /// A typical host: four `e1000g` ports (the last one unplugged), the
    /// management address on the first, loopback, and a default route.
    pub fn typical() -> Self {
        let fake = Self::new();
        fake.add_phys("e1000g0", true, "00:0c:29:ab:cd:ef");
        fake.add_phys("e1000g1", true, "00:0c:29:ab:cd:f0");
        fake.add_phys("e1000g2", true, "00:0c:29:ab:cd:f1");
        fake.add_phys("e1000g3", false, "00:0c:29:ab:cd:f2");
        fake.add_interface("lo0", "loopback");
        fake.add_address("lo0/v4", AddressKind::Static, Some("127.0.0.1/8"));
        fake.add_address("lo0/v6", AddressKind::Static, Some("::1/128"));
        fake.add_address("e1000g0/v4", AddressKind::Static, Some("192.168.1.10/24"));
        fake.seed_route(
            "127.0.0.1/32",
            "127.0.0.1",
            RouteKind::Interface,
            Some("lo0"),
        );
        fake.seed_route("::1/128", "::1", RouteKind::Interface, Some("lo0"));
        fake.seed_route(
            "192.168.1.0/24",
            "192.168.1.10",
            RouteKind::Interface,
            Some("e1000g0"),
        );
        fake.seed_route("default", "192.168.1.1", RouteKind::Static, Some("e1000g0"));
        fake
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Add a physical port.
    pub fn add_phys(&self, name: &str, up: bool, mac: &str) {
        let mut l = LinkInfo::new(name, LinkKind::Phys);
        l.state = if up { LinkState::Up } else { LinkState::Down };
        l.mtu = Some(1500);
        l.mac = Some(mac.to_owned());
        l.speed_mbps = up.then_some(1000);
        l.duplex = Some(if up { Duplex::Full } else { Duplex::Unknown });
        l.device = Some(name.to_owned());
        l.media = Some("Ethernet".to_owned());
        self.lock().links.insert(name.to_owned(), l);
    }

    /// Add an IP interface.
    pub fn add_interface(&self, name: &str, class: &str) {
        self.lock().interfaces.insert(
            name.to_owned(),
            InterfaceInfo {
                name: name.to_owned(),
                class: class.to_owned(),
                state: "ok".to_owned(),
                over: None,
            },
        );
    }

    /// Add a persistent address object, creating its interface if needed.
    pub fn add_address(&self, addrobj: &str, kind: AddressKind, address: Option<&str>) {
        let interface = interface_of(addrobj).to_owned();
        let mut s = self.lock();
        s.interfaces
            .entry(interface.clone())
            .or_insert_with(|| InterfaceInfo {
                name: interface.clone(),
                class: "ip".to_owned(),
                state: "ok".to_owned(),
                over: None,
            });
        let family = address.map_or(
            match kind {
                AddressKind::Addrconf => AddressFamily::Inet6,
                AddressKind::Static | AddressKind::Dhcp => AddressFamily::Inet,
            },
            AddressFamily::of,
        );
        s.addresses.insert(
            addrobj.to_owned(),
            AddressInfo {
                name: addrobj.to_owned(),
                interface,
                kind,
                family,
                address: address.map(str::to_owned),
                state: "ok".to_owned(),
                persistent: true,
            },
        );
    }

    /// Add a route; static routes are persistent.
    pub fn seed_route(
        &self,
        destination: &str,
        gateway: &str,
        kind: RouteKind,
        interface: Option<&str>,
    ) {
        let flags = match kind {
            RouteKind::Static => "UG",
            RouteKind::Interface
                if destination.ends_with("/32") || destination.ends_with("/128") =>
            {
                "UH"
            }
            RouteKind::Interface => "U",
            RouteKind::Dynamic => "UGD",
        };
        self.lock().routes.push(RouteInfo {
            destination: destination.to_owned(),
            gateway: Some(gateway.to_owned()),
            family: AddressFamily::of(gateway),
            interface: interface.map(str::to_owned),
            flags: Some(flags.to_owned()),
            kind,
            persistent: kind == RouteKind::Static,
        });
    }

    fn require_free(s: &State, name: &str) -> Result<()> {
        if s.links.contains_key(name) {
            return Err(tool_error(&format!("{name}: link name already exists")));
        }
        if !valid_link_name(name) {
            return Err(tool_error(&format!("{name}: invalid link name")));
        }
        Ok(())
    }

    fn require_link<'s>(s: &'s State, name: &str) -> Result<&'s LinkInfo> {
        s.links
            .get(name)
            .ok_or_else(|| tool_error(&format!("{name}: object not found")))
    }

    fn require_unused(s: &State, name: &str) -> Result<()> {
        let dependents = s.links.values().any(|l| l.over.iter().any(|o| o == name));
        if dependents || s.interfaces.contains_key(name) {
            return Err(tool_error(&format!("{name}: link busy")));
        }
        Ok(())
    }

    fn next_mac(s: &mut State) -> String {
        s.next_mac += 1;
        let n = s.next_mac;
        format!(
            "02:08:20:{:02x}:{:02x}:{:02x}",
            (n >> 16) & 0xff,
            (n >> 8) & 0xff,
            n & 0xff
        )
    }
}

/// illumos link names: letters, digits, underscores, ending in a digit,
/// at most 31 characters.
fn valid_link_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=31).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        && bytes[bytes.len() - 1].is_ascii_digit()
}

impl Net for FakeNet {
    fn list_links(&self) -> BoxFuture<'_, Result<Vec<LinkInfo>>> {
        Box::pin(async move { Ok(self.lock().links.values().cloned().collect()) })
    }

    fn create_aggr<'a>(&'a self, spec: &'a AggrSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_free(&s, &spec.name)?;
            if spec.ports.is_empty() {
                return Err(tool_error(
                    "invalid: an aggregation needs at least one port",
                ));
            }
            let mut ports = Vec::new();
            let mut up = false;
            let mut speed = 0;
            for name in &spec.ports {
                let port = Self::require_link(&s, name)?;
                if port.kind != LinkKind::Phys {
                    return Err(tool_error(&format!("{name}: invalid: not a physical link")));
                }
                Self::require_unused(&s, name)?;
                up |= port.state == LinkState::Up;
                speed += port.speed_mbps.unwrap_or(0);
                ports.push(AggrPort {
                    name: name.clone(),
                    state: "attached".to_owned(),
                    speed_mbps: port.speed_mbps,
                });
            }
            let mut l = LinkInfo::new(&spec.name, LinkKind::Aggr);
            l.state = if up { LinkState::Up } else { LinkState::Down };
            l.mtu = Some(1500);
            l.over.clone_from(&spec.ports);
            l.speed_mbps = (speed > 0).then_some(speed);
            l.duplex = Some(Duplex::Full);
            l.mac = s.links.get(&spec.ports[0]).and_then(|p| p.mac.clone());
            l.aggr = Some(AggrInfo {
                policy: spec.policy.clone(),
                lacp_mode: spec.lacp_mode,
                lacp_timer: spec.lacp_timer,
                ports,
            });
            s.links.insert(spec.name.clone(), l);
            Ok(())
        })
    }

    fn create_vlan<'a>(&'a self, spec: &'a VlanSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_free(&s, &spec.name)?;
            if !(1..=4094).contains(&spec.vid) {
                return Err(tool_error("invalid VLAN id"));
            }
            let over = Self::require_link(&s, &spec.over)?;
            if !matches!(over.kind, LinkKind::Phys | LinkKind::Aggr) {
                return Err(tool_error(&format!(
                    "{}: invalid: VLANs sit on physical links or aggregations",
                    spec.over
                )));
            }
            let mut l = LinkInfo::new(&spec.name, LinkKind::Vlan);
            l.state = over.state;
            l.mtu = over.mtu;
            l.mac.clone_from(&over.mac);
            l.over = vec![spec.over.clone()];
            l.vid = Some(spec.vid);
            s.links.insert(spec.name.clone(), l);
            Ok(())
        })
    }

    fn create_etherstub<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_free(&s, name)?;
            let mut l = LinkInfo::new(name, LinkKind::Etherstub);
            l.state = LinkState::Up;
            l.mtu = Some(9000);
            s.links.insert(name.to_owned(), l);
            Ok(())
        })
    }

    fn create_vnic<'a>(&'a self, spec: &'a VnicSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_free(&s, &spec.name)?;
            let over = Self::require_link(&s, &spec.over)?;
            if !matches!(
                over.kind,
                LinkKind::Phys | LinkKind::Aggr | LinkKind::Etherstub
            ) {
                return Err(tool_error(&format!(
                    "{}: invalid: VNICs sit on physical links, aggregations, or etherstubs",
                    spec.over
                )));
            }
            if let Some(vid) = spec.vid
                && !(1..=4094).contains(&vid)
            {
                return Err(tool_error("invalid VLAN id"));
            }
            let mut l = LinkInfo::new(&spec.name, LinkKind::Vnic);
            l.state = over.state;
            l.mtu = spec.mtu.or(over.mtu);
            l.over = vec![spec.over.clone()];
            l.speed_mbps = over.speed_mbps;
            l.vid = spec.vid;
            let (mac, mode) = match &spec.mac {
                Some(m) => (
                    parse::normalize_mac(m)
                        .ok_or_else(|| tool_error(&format!("{m}: invalid MAC address")))?,
                    MacMode::Fixed,
                ),
                None => (Self::next_mac(&mut s), MacMode::Random),
            };
            l.mac = Some(mac);
            l.mac_mode = Some(mode);
            s.links.insert(spec.name.clone(), l);
            Ok(())
        })
    }

    fn delete_link<'a>(&'a self, name: &'a str, kind: LinkKind) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let link = Self::require_link(&s, name)?;
            if matches!(kind, LinkKind::Phys | LinkKind::Other) {
                return Err(NetError::Unsupported(format!(
                    "{kind} links cannot be deleted"
                )));
            }
            if link.kind != kind {
                return Err(tool_error(&format!("{name}: invalid: not a {kind}")));
            }
            Self::require_unused(&s, name)?;
            s.links.remove(name);
            Ok(())
        })
    }

    fn set_mtu<'a>(&'a self, name: &'a str, mtu: u32) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            Self::require_link(&s, name)?;
            if !(576..=9216).contains(&mtu) {
                return Err(tool_error(&format!("{name}: invalid mtu {mtu}")));
            }
            if let Some(l) = s.links.get_mut(name) {
                l.mtu = Some(mtu);
            }
            Ok(())
        })
    }

    fn list_interfaces(&self) -> BoxFuture<'_, Result<Vec<InterfaceInfo>>> {
        Box::pin(async move { Ok(self.lock().interfaces.values().cloned().collect()) })
    }

    fn list_addresses(&self) -> BoxFuture<'_, Result<Vec<AddressInfo>>> {
        Box::pin(async move { Ok(self.lock().addresses.values().cloned().collect()) })
    }

    fn create_address<'a>(&'a self, spec: &'a AddressSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let interface = spec.interface().to_owned();
            if interface != "lo0" && !s.links.contains_key(&interface) {
                return Err(tool_error(&format!("{interface}: object not found")));
            }
            if s.addresses.contains_key(&spec.addrobj) {
                return Err(tool_error("cannot create address: Object already exists"));
            }
            let address = match spec.kind {
                AddressKind::Static => {
                    let Some(a) = &spec.address else {
                        return Err(tool_error("invalid: a static address needs an address"));
                    };
                    let (ip, prefix) = a.split_once('/').unwrap_or((a, ""));
                    if ip.parse::<std::net::IpAddr>().is_err()
                        || (!prefix.is_empty() && prefix.parse::<u8>().is_err())
                    {
                        return Err(tool_error(&format!("{a}: invalid address")));
                    }
                    Some(a.clone())
                }
                AddressKind::Dhcp => None,
                AddressKind::Addrconf => Some("fe80::20c:29ff:feab:cdef/10".to_owned()),
            };
            s.interfaces
                .entry(interface.clone())
                .or_insert_with(|| InterfaceInfo {
                    name: interface.clone(),
                    class: "ip".to_owned(),
                    state: "ok".to_owned(),
                    over: None,
                });
            let family = address
                .as_deref()
                .map_or(AddressFamily::Inet, AddressFamily::of);
            s.addresses.insert(
                spec.addrobj.clone(),
                AddressInfo {
                    name: spec.addrobj.clone(),
                    interface,
                    kind: spec.kind,
                    family,
                    address,
                    state: "ok".to_owned(),
                    persistent: !spec.temporary,
                },
            );
            Ok(())
        })
    }

    fn delete_address<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            if s.addresses.remove(name).is_none() {
                return Err(tool_error("cannot delete address: Object not found"));
            }
            Ok(())
        })
    }

    fn list_routes(&self) -> BoxFuture<'_, Result<Vec<RouteInfo>>> {
        Box::pin(async move { Ok(self.lock().routes.clone()) })
    }

    fn add_route<'a>(&'a self, spec: &'a RouteSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let (gw, _) = spec.gateway.split_once('/').unwrap_or((&spec.gateway, ""));
            if gw.parse::<std::net::IpAddr>().is_err() {
                return Err(tool_error(&format!("{}: bad address", spec.gateway)));
            }
            if spec.destination != "default" {
                let (ip, prefix) = spec
                    .destination
                    .split_once('/')
                    .unwrap_or((&spec.destination, ""));
                if ip.parse::<std::net::IpAddr>().is_err()
                    || (!prefix.is_empty() && prefix.parse::<u8>().is_err())
                {
                    return Err(tool_error(&format!("{}: bad address", spec.destination)));
                }
            }
            let dup = s.routes.iter().any(|r| {
                r.family == spec.family
                    && r.destination == spec.destination
                    && r.gateway.as_deref() == Some(spec.gateway.as_str())
            });
            if dup {
                return Err(tool_error(&format!(
                    "add net {}: gateway {}: entry exists",
                    spec.destination, spec.gateway
                )));
            }
            s.routes.push(RouteInfo {
                destination: spec.destination.clone(),
                gateway: Some(spec.gateway.clone()),
                family: spec.family,
                interface: None,
                flags: Some("UG".to_owned()),
                kind: RouteKind::Static,
                persistent: true,
            });
            Ok(())
        })
    }

    fn delete_route<'a>(&'a self, spec: &'a RouteSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let found = s.routes.iter().position(|r| {
                r.kind == RouteKind::Static
                    && r.family == spec.family
                    && r.destination == spec.destination
                    && r.gateway.as_deref() == Some(spec.gateway.as_str())
            });
            let Some(i) = found else {
                return Err(tool_error(&format!(
                    "delete net {}: gateway {}: not in table",
                    spec.destination, spec.gateway
                )));
            };
            s.routes.remove(i);
            Ok(())
        })
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

    use mandrake_core::network::{LacpMode, LacpTimer};

    use super::*;
    use crate::FailureKind;

    #[tokio::test]
    async fn topology_lifecycle() {
        let net = FakeNet::typical();
        assert_eq!(net.list_links().await.unwrap().len(), 4);
        net.create_aggr(&AggrSpec {
            name: "aggr0".to_owned(),
            ports: vec!["e1000g1".to_owned(), "e1000g2".to_owned()],
            policy: "L4".to_owned(),
            lacp_mode: LacpMode::Active,
            lacp_timer: LacpTimer::Short,
        })
        .await
        .unwrap();
        let aggr = net.link("aggr0").await.unwrap();
        assert_eq!(aggr.speed_mbps, Some(2000));
        assert_eq!(aggr.aggr.as_ref().unwrap().ports.len(), 2);

        // A port in an aggr cannot join another.
        let err = net
            .create_aggr(&AggrSpec {
                name: "aggr1".to_owned(),
                ports: vec!["e1000g2".to_owned()],
                policy: "L4".to_owned(),
                lacp_mode: LacpMode::Off,
                lacp_timer: LacpTimer::Long,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(err.kind(), FailureKind::Conflict);

        net.create_etherstub("stub0").await.unwrap();
        net.create_vnic(&VnicSpec {
            name: "vnic0".to_owned(),
            over: "stub0".to_owned(),
            mac: None,
            vid: None,
            mtu: None,
        })
        .await
        .unwrap();
        let vnic = net.link("vnic0").await.unwrap();
        assert_eq!(vnic.mac_mode, Some(MacMode::Random));
        assert_eq!(vnic.mtu, Some(9000));
        assert!(vnic.mac.as_deref().unwrap().starts_with("02:08:20:"));

        // The etherstub is busy while the VNIC exists.
        let busy = net
            .delete_link("stub0", LinkKind::Etherstub)
            .await
            .err()
            .unwrap();
        assert_eq!(busy.kind(), FailureKind::Conflict);
        let wrong = net
            .delete_link("stub0", LinkKind::Vnic)
            .await
            .err()
            .unwrap();
        assert_eq!(wrong.kind(), FailureKind::Invalid);
        net.delete_link("vnic0", LinkKind::Vnic).await.unwrap();
        net.delete_link("stub0", LinkKind::Etherstub).await.unwrap();
        assert!(matches!(
            net.link("stub0").await,
            Err(NetError::NotFound(_))
        ));

        let bad = net.set_mtu("aggr0", 100).await.err().unwrap();
        assert_eq!(bad.kind(), FailureKind::Invalid);
        net.set_mtu("aggr0", 9000).await.unwrap();
        assert_eq!(net.link("aggr0").await.unwrap().mtu, Some(9000));

        let dup = net.create_etherstub("e1000g0").await.err().unwrap();
        assert_eq!(dup.kind(), FailureKind::Exists);
        let phys = net
            .delete_link("e1000g0", LinkKind::Phys)
            .await
            .err()
            .unwrap();
        assert_eq!(phys.kind(), FailureKind::Invalid);
    }

    #[tokio::test]
    async fn addresses_and_routes() {
        let net = FakeNet::typical();
        assert_eq!(net.list_addresses().await.unwrap().len(), 3);
        assert_eq!(net.list_interfaces().await.unwrap().len(), 2);
        net.create_address(&AddressSpec {
            addrobj: "e1000g1/v4".to_owned(),
            kind: AddressKind::Static,
            address: Some("10.0.0.5/24".to_owned()),
            temporary: true,
        })
        .await
        .unwrap();
        assert_eq!(net.list_interfaces().await.unwrap().len(), 3);
        let a = net.address("e1000g1/v4").await.unwrap();
        assert!(!a.persistent);
        assert_eq!(a.family, AddressFamily::Inet);
        let dup = net
            .create_address(&AddressSpec {
                addrobj: "e1000g1/v4".to_owned(),
                kind: AddressKind::Dhcp,
                address: None,
                temporary: false,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(dup.kind(), FailureKind::Exists);
        let bad = net
            .create_address(&AddressSpec {
                addrobj: "e1000g1/x".to_owned(),
                kind: AddressKind::Static,
                address: Some("not-an-address".to_owned()),
                temporary: false,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(bad.kind(), FailureKind::Invalid);
        // The link now has an interface, so it is busy.
        assert_eq!(
            net.create_aggr(&AggrSpec {
                name: "aggr0".to_owned(),
                ports: vec!["e1000g1".to_owned()],
                policy: "L4".to_owned(),
                lacp_mode: LacpMode::Active,
                lacp_timer: LacpTimer::Short,
            })
            .await
            .err()
            .unwrap()
            .kind(),
            FailureKind::Conflict
        );
        net.delete_address("e1000g1/v4").await.unwrap();
        let gone = net.delete_address("e1000g1/v4").await.err().unwrap();
        assert_eq!(gone.kind(), FailureKind::NotFound);

        assert_eq!(net.list_routes().await.unwrap().len(), 4);
        net.add_route(&RouteSpec::new("10.20.0.0/16", "192.168.1.1"))
            .await
            .unwrap();
        let dup = net
            .add_route(&RouteSpec::new("10.20.0.0/16", "192.168.1.1"))
            .await
            .err()
            .unwrap();
        assert_eq!(dup.kind(), FailureKind::Exists);
        let bad = net
            .add_route(&RouteSpec::new("10.20.0.0/16", "nowhere"))
            .await
            .err()
            .unwrap();
        assert_eq!(bad.kind(), FailureKind::Invalid);
        net.delete_route(&RouteSpec::new("10.20.0.0/16", "192.168.1.1"))
            .await
            .unwrap();
        let missing = net
            .delete_route(&RouteSpec::new("192.168.1.0/24", "192.168.1.10"))
            .await
            .err()
            .unwrap();
        assert_eq!(missing.kind(), FailureKind::NotFound);
        assert_eq!(net.list_routes().await.unwrap().len(), 4);
    }
}
