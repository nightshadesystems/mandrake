//! Driver for Crossbow networking via `dladm`, `ipadm`, `route`, and
//! `netstat` (ADR-0003, ADR-0011).
//!
//! [`Net`] is the typed operation surface. [`NetCli`] implements it by
//! shelling out through a [`mandrake_core::shell::Runner`]; [`FakeNet`]
//! implements it in memory with the same observable behaviour so the
//! daemon's routes are testable anywhere. Parsers live in [`parse`] as pure
//! functions over `&str`.
//!
//! Physical links, aggrs, VLANs, etherstubs, VNICs, addresses, and routes
//! are surfaced directly with no abstraction layer over them. Protecting
//! the management path is the daemon's job (ADR-0011); this crate executes
//! what it is asked.

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod cli;
pub mod fake;
pub mod parse;
pub mod types;

pub use cli::NetCli;
pub use fake::FakeNet;
pub use mandrake_core::shell::BoxFuture;
pub use types::*;

use mandrake_core::network::LinkKind;

/// Typed network operations. Names are link names, address object names
/// (`link/alias`), or route destinations.
pub trait Net: Send + Sync {
    /// Every datalink with what it sits on and its kind-specific details.
    fn list_links(&self) -> BoxFuture<'_, Result<Vec<LinkInfo>>>;

    /// One link by name.
    fn link<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<LinkInfo>> {
        Box::pin(async move {
            self.list_links()
                .await?
                .into_iter()
                .find(|l| l.name == name)
                .ok_or_else(|| NetError::NotFound(name.to_owned()))
        })
    }

    /// `dladm create-aggr`.
    fn create_aggr<'a>(&'a self, spec: &'a AggrSpec) -> BoxFuture<'a, Result<()>>;
    /// `dladm create-vlan`.
    fn create_vlan<'a>(&'a self, spec: &'a VlanSpec) -> BoxFuture<'a, Result<()>>;
    /// `dladm create-etherstub`.
    fn create_etherstub<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `dladm create-vnic`.
    fn create_vnic<'a>(&'a self, spec: &'a VnicSpec) -> BoxFuture<'a, Result<()>>;
    /// `dladm delete-{aggr,vlan,etherstub,vnic}`; physical links refuse.
    fn delete_link<'a>(&'a self, name: &'a str, kind: LinkKind) -> BoxFuture<'a, Result<()>>;
    /// `dladm set-linkprop -p mtu=N`.
    fn set_mtu<'a>(&'a self, name: &'a str, mtu: u32) -> BoxFuture<'a, Result<()>>;

    /// IP interfaces (`ipadm show-if`).
    fn list_interfaces(&self) -> BoxFuture<'_, Result<Vec<InterfaceInfo>>>;
    /// Address objects (`ipadm show-addr`).
    fn list_addresses(&self) -> BoxFuture<'_, Result<Vec<AddressInfo>>>;

    /// One address object by name.
    fn address<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<AddressInfo>> {
        Box::pin(async move {
            self.list_addresses()
                .await?
                .into_iter()
                .find(|a| a.name == name)
                .ok_or_else(|| NetError::NotFound(name.to_owned()))
        })
    }

    /// `ipadm create-addr`, creating the IP interface first when missing.
    fn create_address<'a>(&'a self, spec: &'a AddressSpec) -> BoxFuture<'a, Result<()>>;
    /// `ipadm delete-addr`.
    fn delete_address<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;

    /// The routing table (`netstat -rnv`) with persistence from `route -p show`.
    fn list_routes(&self) -> BoxFuture<'_, Result<Vec<RouteInfo>>>;
    /// `route -p add`.
    fn add_route<'a>(&'a self, spec: &'a RouteSpec) -> BoxFuture<'a, Result<()>>;
    /// `route -p delete`.
    fn delete_route<'a>(&'a self, spec: &'a RouteSpec) -> BoxFuture<'a, Result<()>>;
}
