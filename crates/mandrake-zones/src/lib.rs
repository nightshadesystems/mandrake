//! Driver for illumos zones via `zonecfg` and `zoneadm` (ADR-0003,
//! ADR-0012).
//!
//! [`Zones`] is the typed operation surface. [`ZonesCli`] implements it by
//! shelling out through a [`mandrake_core::shell::Runner`]; [`FakeZones`]
//! implements it in memory with the same observable behaviour so the
//! daemon's routes are testable anywhere. Parsers and the zonecfg script
//! renderers live in [`parse`] as pure functions.
//!
//! The driver knows nothing about images, ids, or jobs: it configures,
//! installs, boots, and removes zones as asked. Which brand may do what is
//! the daemon's (ADR-0012).

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod cli;
pub mod fake;
pub mod parse;
pub mod types;

pub use cli::ZonesCli;
pub use fake::FakeZones;
pub use mandrake_core::shell::BoxFuture;
pub use types::*;

/// Typed zone operations. Names are zone names.
pub trait Zones: Send + Sync {
    /// Every zone but the global one, as `zoneadm list -pc` reports it.
    fn list(&self) -> BoxFuture<'_, Result<Vec<ZoneSummary>>>;
    /// One zone's configuration (`zonecfg export`).
    fn config<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<ZoneConfig>>;
    /// Write a new configuration (`zonecfg create`).
    fn create<'a>(&'a self, spec: &'a ZoneSpec) -> BoxFuture<'a, Result<()>>;
    /// Rewrite the managed parts of an existing configuration.
    fn update<'a>(&'a self, spec: &'a ZoneSpec) -> BoxFuture<'a, Result<()>>;
    /// Set one string attribute, adding it if missing.
    fn set_attr<'a>(
        &'a self,
        name: &'a str,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<()>>;
    /// `zoneadm install` from packages, an archive, or a prepared zonepath.
    fn install<'a>(&'a self, name: &'a str, source: &'a InstallSource)
    -> BoxFuture<'a, Result<()>>;
    /// `zoneadm boot`.
    fn boot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zoneadm shutdown`, with `-r` to reboot.
    fn shutdown<'a>(&'a self, name: &'a str, reboot: bool) -> BoxFuture<'a, Result<()>>;
    /// `zoneadm halt`.
    fn halt<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zoneadm uninstall -F`.
    fn uninstall<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zonecfg delete -F`.
    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// The zonecfg attribute carrying a Mandrake id (spec §7).
pub const ID_ATTR: &str = "mandrake-id";
/// The zonecfg attribute naming the image a zone was cloned from.
pub const IMAGE_ATTR: &str = "mandrake-image";
/// The zonecfg attribute carrying the hostname (ADR-0012).
pub const HOSTNAME_ATTR: &str = "hostname";
/// The zonecfg attribute carrying comma-separated resolvers.
pub const RESOLVERS_ATTR: &str = "resolvers";
