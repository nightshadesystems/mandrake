//! The real driver: `dladm`, `ipadm`, `route`, and `netstat` through a
//! [`Runner`].

use std::sync::Arc;

use mandrake_core::{
    network::{AddressFamily, AddressKind, LinkKind},
    shell::{Command, Runner},
};

use crate::{
    AddressInfo, AddressSpec, AggrSpec, BoxFuture, InterfaceInfo, LinkInfo, Net, NetError, Result,
    RouteInfo, RouteSpec, VlanSpec, VnicSpec, parse,
};

/// Shells out to the illumos tools.
#[derive(Clone)]
pub struct NetCli {
    runner: Arc<dyn Runner>,
}

impl NetCli {
    /// A driver over `runner`.
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }

    async fn stdout(&self, cmd: Command) -> Result<String> {
        Ok(self.runner.run(&cmd).await?.stdout)
    }

    async fn run(&self, cmd: Command) -> Result<()> {
        self.runner.run(&cmd).await?;
        Ok(())
    }

    /// `dladm <args> -p -o <columns>`.
    async fn dladm(&self, args: &[&str], columns: &str) -> Result<String> {
        self.stdout(
            Command::new("dladm")
                .args(args.iter().copied())
                .args(["-p", "-o", columns]),
        )
        .await
    }
}

/// The `dladm delete-*` subcommand for a kind, if it has one.
fn delete_subcommand(kind: LinkKind) -> Option<&'static str> {
    match kind {
        LinkKind::Aggr => Some("delete-aggr"),
        LinkKind::Vlan => Some("delete-vlan"),
        LinkKind::Etherstub => Some("delete-etherstub"),
        LinkKind::Vnic => Some("delete-vnic"),
        LinkKind::Phys | LinkKind::Other => None,
    }
}

/// `route -p <verb> [-inet6] <destination> <gateway>`.
fn route_command(verb: &str, spec: &RouteSpec) -> Command {
    let mut cmd = Command::new("route").args(["-p", verb]).privileged();
    if spec.family == AddressFamily::Inet6 {
        cmd = cmd.arg("-inet6");
    }
    cmd = if spec.destination == "default" {
        cmd.arg("default")
    } else if spec.destination.contains('/') {
        cmd.args(["-net", &spec.destination])
    } else {
        cmd.args(["-host", &spec.destination])
    };
    cmd.arg(&spec.gateway)
}

impl Net for NetCli {
    fn list_links(&self) -> BoxFuture<'_, Result<Vec<LinkInfo>>> {
        Box::pin(async move {
            let sources = parse::LinkSources {
                links: parse::show_link(
                    &self.dladm(&["show-link"], parse::SHOW_LINK_COLUMNS).await?,
                )?,
                phys: parse::show_phys(
                    &self.dladm(&["show-phys"], parse::SHOW_PHYS_COLUMNS).await?,
                )?,
                macs: parse::show_phys_macs(
                    &self
                        .dladm(&["show-phys", "-m"], parse::SHOW_PHYS_MAC_COLUMNS)
                        .await?,
                )?,
                aggrs: parse::show_aggr(
                    &self.dladm(&["show-aggr"], parse::SHOW_AGGR_COLUMNS).await?,
                )?,
                ports: parse::show_aggr_ports(
                    &self
                        .dladm(&["show-aggr", "-x"], parse::SHOW_AGGR_PORT_COLUMNS)
                        .await?,
                )?,
                vlans: parse::show_vlan(
                    &self.dladm(&["show-vlan"], parse::SHOW_VLAN_COLUMNS).await?,
                )?,
                vnics: parse::show_vnic(
                    &self.dladm(&["show-vnic"], parse::SHOW_VNIC_COLUMNS).await?,
                )?,
            };
            Ok(sources.assemble())
        })
    }

    fn create_aggr<'a>(&'a self, spec: &'a AggrSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("dladm").arg("create-aggr").privileged();
            for port in &spec.ports {
                cmd = cmd.args(["-l", port]);
            }
            cmd = cmd
                .args(["-P", &spec.policy])
                .args(["-L", spec.lacp_mode.as_str()])
                .args(["-T", spec.lacp_timer.as_str()])
                .arg(&spec.name);
            self.run(cmd).await
        })
    }

    fn create_vlan<'a>(&'a self, spec: &'a VlanSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("dladm")
                    .arg("create-vlan")
                    .args(["-l", &spec.over])
                    .args(["-v", &spec.vid.to_string()])
                    .arg(&spec.name)
                    .privileged(),
            )
            .await
        })
    }

    fn create_etherstub<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("dladm")
                    .args(["create-etherstub", name])
                    .privileged(),
            )
            .await
        })
    }

    fn create_vnic<'a>(&'a self, spec: &'a VnicSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut cmd = Command::new("dladm")
                .arg("create-vnic")
                .args(["-l", &spec.over])
                .privileged();
            if let Some(mac) = &spec.mac {
                cmd = cmd.args(["-m", mac]);
            }
            if let Some(vid) = spec.vid {
                cmd = cmd.args(["-v", &vid.to_string()]);
            }
            if let Some(mtu) = spec.mtu {
                cmd = cmd.args(["-p", &format!("mtu={mtu}")]);
            }
            self.run(cmd.arg(&spec.name)).await
        })
    }

    fn delete_link<'a>(&'a self, name: &'a str, kind: LinkKind) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let Some(sub) = delete_subcommand(kind) else {
                return Err(NetError::Unsupported(format!(
                    "{kind} links cannot be deleted"
                )));
            };
            self.run(Command::new("dladm").args([sub, name]).privileged())
                .await
        })
    }

    fn set_mtu<'a>(&'a self, name: &'a str, mtu: u32) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("dladm")
                    .arg("set-linkprop")
                    .args(["-p", &format!("mtu={mtu}")])
                    .arg(name)
                    .privileged(),
            )
            .await
        })
    }

    fn list_interfaces(&self) -> BoxFuture<'_, Result<Vec<InterfaceInfo>>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("ipadm").args(["show-if", "-p", "-o", parse::SHOW_IF_COLUMNS]))
                .await?;
            parse::show_if(&out)
        })
    }

    fn list_addresses(&self) -> BoxFuture<'_, Result<Vec<AddressInfo>>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("ipadm").args([
                    "show-addr",
                    "-p",
                    "-o",
                    parse::SHOW_ADDR_COLUMNS,
                ]))
                .await?;
            parse::show_addr(&out)
        })
    }

    fn create_address<'a>(&'a self, spec: &'a AddressSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let interface = spec.interface();
            let exists = self
                .list_interfaces()
                .await?
                .iter()
                .any(|i| i.name == interface);
            if !exists {
                let mut cmd = Command::new("ipadm").arg("create-if").privileged();
                if spec.temporary {
                    cmd = cmd.arg("-t");
                }
                self.run(cmd.arg(interface)).await?;
            }
            let mut cmd = Command::new("ipadm").arg("create-addr").privileged();
            if spec.temporary {
                cmd = cmd.arg("-t");
            }
            cmd = cmd.args(["-T", spec.kind.as_str()]);
            if spec.kind == AddressKind::Static {
                let Some(address) = &spec.address else {
                    return Err(NetError::Unsupported(
                        "a static address needs an address".to_owned(),
                    ));
                };
                cmd = cmd.args(["-a", address]);
            }
            self.run(cmd.arg(&spec.addrobj)).await
        })
    }

    fn delete_address<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("ipadm")
                    .args(["delete-addr", name])
                    .privileged(),
            )
            .await
        })
    }

    fn list_routes(&self) -> BoxFuture<'_, Result<Vec<RouteInfo>>> {
        Box::pin(async move {
            let out = self.stdout(Command::new("netstat").args(["-rnv"])).await?;
            let mut routes = parse::netstat_routes(&out);
            let persistent = match self
                .stdout(Command::new("route").args(["-p", "show"]))
                .await
            {
                Ok(out) => parse::persistent_routes(&out),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot read persistent routes");
                    Vec::new()
                }
            };
            parse::mark_persistent(&mut routes, &persistent);
            Ok(routes)
        })
    }

    fn add_route<'a>(&'a self, spec: &'a RouteSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.run(route_command("add", spec)).await })
    }

    fn delete_route<'a>(&'a self, spec: &'a RouteSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.run(route_command("delete", spec)).await })
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

    use mandrake_core::{
        network::{LacpMode, LacpTimer},
        shell::ScriptedRunner,
    };

    use super::*;

    fn driver(r: &Arc<ScriptedRunner>) -> NetCli {
        NetCli::new(Arc::clone(r) as Arc<dyn Runner>)
    }

    #[tokio::test]
    async fn link_commands_are_exact() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok("dladm", "");
        let d = driver(&r);
        d.create_aggr(&AggrSpec {
            name: "aggr0".to_owned(),
            ports: vec!["e1000g1".to_owned(), "e1000g2".to_owned()],
            policy: "L4".to_owned(),
            lacp_mode: LacpMode::Active,
            lacp_timer: LacpTimer::Short,
        })
        .await
        .unwrap();
        d.create_vlan(&VlanSpec {
            name: "vlan10".to_owned(),
            vid: 10,
            over: "e1000g0".to_owned(),
        })
        .await
        .unwrap();
        d.create_etherstub("etherstub0").await.unwrap();
        d.create_vnic(&VnicSpec {
            name: "vnic0".to_owned(),
            over: "etherstub0".to_owned(),
            mac: Some("02:08:20:a1:b2:c3".to_owned()),
            vid: Some(20),
            mtu: Some(9000),
        })
        .await
        .unwrap();
        d.create_vnic(&VnicSpec {
            name: "vnic1".to_owned(),
            over: "aggr0".to_owned(),
            mac: None,
            vid: None,
            mtu: None,
        })
        .await
        .unwrap();
        d.set_mtu("vnic1", 1400).await.unwrap();
        d.delete_link("vnic1", LinkKind::Vnic).await.unwrap();
        d.delete_link("aggr0", LinkKind::Aggr).await.unwrap();
        let err = d
            .delete_link("e1000g0", LinkKind::Phys)
            .await
            .err()
            .unwrap();
        assert_eq!(err.kind(), crate::FailureKind::Invalid);
        assert_eq!(
            r.lines(),
            vec![
                "pfexec dladm create-aggr -l e1000g1 -l e1000g2 -P L4 -L active -T short aggr0",
                "pfexec dladm create-vlan -l e1000g0 -v 10 vlan10",
                "pfexec dladm create-etherstub etherstub0",
                "pfexec dladm create-vnic -l etherstub0 -m 02:08:20:a1:b2:c3 -v 20 -p mtu=9000 vnic0",
                "pfexec dladm create-vnic -l aggr0 vnic1",
                "pfexec dladm set-linkprop -p mtu=1400 vnic1",
                "pfexec dladm delete-vnic vnic1",
                "pfexec dladm delete-aggr aggr0",
            ]
        );
    }

    #[tokio::test]
    async fn address_and_route_commands_are_exact() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok(
            "ipadm show-if",
            include_str!("../testdata/ipadm-show-if-p.synthetic.txt"),
        )
        .ok("ipadm", "")
        .ok("route", "");
        let d = driver(&r);
        // Interface exists: no create-if.
        d.create_address(&AddressSpec {
            addrobj: "e1000g0/backup".to_owned(),
            kind: AddressKind::Static,
            address: Some("192.168.1.11/24".to_owned()),
            temporary: false,
        })
        .await
        .unwrap();
        // Interface missing: create-if first, temporary throughout.
        d.create_address(&AddressSpec {
            addrobj: "vnic0/v4".to_owned(),
            kind: AddressKind::Dhcp,
            address: None,
            temporary: true,
        })
        .await
        .unwrap();
        d.delete_address("vnic0/v4").await.unwrap();
        d.add_route(&RouteSpec::new("default", "192.168.1.1"))
            .await
            .unwrap();
        d.add_route(&RouteSpec::new("10.20.0.0/16", "192.168.1.1"))
            .await
            .unwrap();
        d.delete_route(&RouteSpec::new("default", "fe80::1"))
            .await
            .unwrap();
        let lines = r.lines();
        let mutations: Vec<&str> = lines
            .iter()
            .filter(|l| !l.starts_with("ipadm show-if"))
            .map(String::as_str)
            .collect();
        assert_eq!(
            mutations,
            vec![
                "pfexec ipadm create-addr -T static -a 192.168.1.11/24 e1000g0/backup",
                "pfexec ipadm create-if -t vnic0",
                "pfexec ipadm create-addr -t -T dhcp vnic0/v4",
                "pfexec ipadm delete-addr vnic0/v4",
                "pfexec route -p add default 192.168.1.1",
                "pfexec route -p add -net 10.20.0.0/16 192.168.1.1",
                "pfexec route -p delete -inet6 default fe80::1",
            ]
        );
    }

    #[tokio::test]
    async fn links_and_routes_read_the_testdata() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok(
            "dladm show-link",
            include_str!("../testdata/dladm-show-link-p.synthetic.txt"),
        )
        .ok(
            "dladm show-phys -m",
            include_str!("../testdata/dladm-show-phys-m-p.synthetic.txt"),
        )
        .ok(
            "dladm show-phys",
            include_str!("../testdata/dladm-show-phys-p.synthetic.txt"),
        )
        .ok(
            "dladm show-aggr -x",
            include_str!("../testdata/dladm-show-aggr-x-p.synthetic.txt"),
        )
        .ok(
            "dladm show-aggr",
            include_str!("../testdata/dladm-show-aggr-p.synthetic.txt"),
        )
        .ok(
            "dladm show-vlan",
            include_str!("../testdata/dladm-show-vlan-p.synthetic.txt"),
        )
        .ok(
            "dladm show-vnic",
            include_str!("../testdata/dladm-show-vnic-p.synthetic.txt"),
        )
        .ok(
            "netstat -rnv",
            include_str!("../testdata/netstat-rnv.synthetic.txt"),
        )
        .fail("route -p show", 1, "route: no persistent routes");
        let d = driver(&r);
        let links = d.list_links().await.unwrap();
        assert_eq!(links.len(), 9);
        let aggr = d.link("aggr0").await.unwrap();
        assert_eq!(aggr.over, ["e1000g1", "e1000g2"]);
        assert!(matches!(d.link("nope0").await, Err(NetError::NotFound(_))));
        let routes = d.list_routes().await.unwrap();
        assert_eq!(routes.len(), 7);
        assert!(routes.iter().all(|r| !r.persistent));
    }

    #[tokio::test]
    async fn failures_classify() {
        let r = Arc::new(ScriptedRunner::new());
        r.fail(
            "dladm delete-etherstub",
            1,
            "dladm: delete-etherstub: etherstub0: link busy",
        )
        .ok("ipadm show-if", "lo0:loopback:ok:-m-v------46:---:--\n")
        .fail(
            "ipadm create-addr",
            1,
            "ipadm: cannot create address: Object already exists",
        )
        .fail(
            "route -p delete",
            1,
            "delete net 10.0.0.0/8: gateway 10.0.0.1: not in table",
        );
        let d = driver(&r);
        let busy = d
            .delete_link("etherstub0", LinkKind::Etherstub)
            .await
            .err()
            .unwrap();
        assert_eq!(busy.kind(), crate::FailureKind::Conflict);
        let exists = d
            .create_address(&AddressSpec {
                addrobj: "lo0/v4".to_owned(),
                kind: AddressKind::Static,
                address: Some("127.0.0.1/8".to_owned()),
                temporary: true,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(exists.kind(), crate::FailureKind::Exists);
        let missing = d
            .delete_route(&RouteSpec::new("10.0.0.0/8", "10.0.0.1"))
            .await
            .err()
            .unwrap();
        assert_eq!(missing.kind(), crate::FailureKind::NotFound);
    }
}
