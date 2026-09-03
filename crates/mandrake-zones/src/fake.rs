//! An in-memory zone stack with the same observable behaviour as the real
//! one, for route tests and for developing the console away from illumos.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use mandrake_core::{shell::ShellError, zone::ZoneState};

use crate::{
    BoxFuture, InstallSource, Result, ZoneConfig, ZoneError, ZoneSpec, ZoneSummary, Zones, parse,
};

#[derive(Debug, Clone)]
struct Entry {
    config: ZoneConfig,
    state: ZoneState,
}

#[derive(Default)]
struct State {
    zones: BTreeMap<String, Entry>,
}

/// The fake driver. Clone to share.
#[derive(Clone, Default)]
pub struct FakeZones {
    state: Arc<Mutex<State>>,
}

fn tool_error(message: &str) -> ZoneError {
    ZoneError::Command(ShellError::Failed {
        command: "fake".to_owned(),
        status: 1,
        stderr: message.to_owned(),
    })
}

fn not_found(name: &str) -> ZoneError {
    tool_error(&format!("zonecfg: zone '{name}': No such zone configured"))
}

impl FakeZones {
    /// No zones.
    pub fn new() -> Self {
        Self::default()
    }

    /// A typical host: one native zone `build` created out of band, so it
    /// has no `mandrake-id`, installed but not running.
    pub fn typical() -> Self {
        let fake = Self::new();
        fake.add_zone(
            ZoneConfig {
                name: "build".to_owned(),
                brand: "ipkg".to_owned(),
                zonepath: "/rpool/zones/build".to_owned(),
                autoboot: false,
                ip_type: "exclusive".to_owned(),
                ..ZoneConfig::default()
            },
            ZoneState::Installed,
        );
        fake
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Add a zone in a given state, as if it existed before the daemon.
    pub fn add_zone(&self, config: ZoneConfig, state: ZoneState) {
        self.lock()
            .zones
            .insert(config.name.clone(), Entry { config, state });
    }

    /// A zone's state, for assertions.
    pub fn state_of(&self, name: &str) -> Option<ZoneState> {
        self.lock().zones.get(name).map(|e| e.state)
    }

    fn transition(
        &self,
        name: &str,
        allowed: &[ZoneState],
        next: ZoneState,
        refusal: &str,
    ) -> Result<()> {
        let mut s = self.lock();
        let entry = s.zones.get_mut(name).ok_or_else(|| not_found(name))?;
        if !allowed.contains(&entry.state) {
            return Err(tool_error(&format!(
                "zoneadm: zone '{name}': {refusal} (state {})",
                entry.state
            )));
        }
        entry.state = next;
        Ok(())
    }
}

fn apply_spec(config: &mut ZoneConfig, spec: &ZoneSpec) {
    config.autoboot = spec.autoboot;
    config.nics.clone_from(&spec.nics);
    config.cpu_cap = spec.cpu_cap;
    config.memory_cap = spec.memory_cap;
    for k in parse::MANAGED_ATTRS {
        if !spec.attrs.contains_key(k) {
            config.attrs.remove(k);
        }
    }
    for (k, v) in &spec.attrs {
        config.attrs.insert(k.clone(), v.clone());
    }
}

impl Zones for FakeZones {
    fn list(&self) -> BoxFuture<'_, Result<Vec<ZoneSummary>>> {
        Box::pin(async move {
            Ok(self
                .lock()
                .zones
                .values()
                .map(|e| ZoneSummary {
                    name: e.config.name.clone(),
                    state: e.state,
                    brand: e.config.brand.clone(),
                    zonepath: e.config.zonepath.clone(),
                    uuid: None,
                    exclusive_ip: e.config.ip_type != "shared",
                })
                .collect())
        })
    }

    fn config<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<ZoneConfig>> {
        Box::pin(async move {
            self.lock()
                .zones
                .get(name)
                .map(|e| e.config.clone())
                .ok_or_else(|| ZoneError::NotFound(name.to_owned()))
        })
    }

    fn create<'a>(&'a self, spec: &'a ZoneSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            if s.zones.contains_key(&spec.name) {
                return Err(tool_error(&format!(
                    "zonecfg: zone '{}': Zone already exists",
                    spec.name
                )));
            }
            if !parse::valid_zone_name(&spec.name) {
                return Err(tool_error("zonecfg: invalid zone name"));
            }
            if s.zones.values().any(|e| e.config.zonepath == spec.zonepath) {
                return Err(tool_error(&format!(
                    "zonecfg: zonepath {} is in use",
                    spec.zonepath
                )));
            }
            let mut config = ZoneConfig {
                name: spec.name.clone(),
                brand: spec.brand.clone(),
                zonepath: spec.zonepath.clone(),
                ip_type: "exclusive".to_owned(),
                ..ZoneConfig::default()
            };
            apply_spec(&mut config, spec);
            s.zones.insert(
                spec.name.clone(),
                Entry {
                    config,
                    state: ZoneState::Configured,
                },
            );
            Ok(())
        })
    }

    fn update<'a>(&'a self, spec: &'a ZoneSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let entry = s
                .zones
                .get_mut(&spec.name)
                .ok_or_else(|| not_found(&spec.name))?;
            apply_spec(&mut entry.config, spec);
            Ok(())
        })
    }

    fn set_attr<'a>(
        &'a self,
        name: &'a str,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let entry = s.zones.get_mut(name).ok_or_else(|| not_found(name))?;
            entry.config.attrs.insert(key.to_owned(), value.to_owned());
            Ok(())
        })
    }

    fn install<'a>(
        &'a self,
        name: &'a str,
        _source: &'a InstallSource,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.transition(
                name,
                &[ZoneState::Configured, ZoneState::Incomplete],
                ZoneState::Installed,
                "zone is already installed",
            )
        })
    }

    fn boot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let state = self.state_of(name).ok_or_else(|| not_found(name))?;
            if state == ZoneState::Running {
                return Err(tool_error(&format!(
                    "zoneadm: zone '{name}': zone is already running"
                )));
            }
            self.transition(
                name,
                &[ZoneState::Installed, ZoneState::Ready, ZoneState::Down],
                ZoneState::Running,
                "cannot boot: zone is not installed",
            )
        })
    }

    fn shutdown<'a>(&'a self, name: &'a str, reboot: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let next = if reboot {
                ZoneState::Running
            } else {
                ZoneState::Installed
            };
            self.transition(name, &[ZoneState::Running], next, "zone is not running")
        })
    }

    fn halt<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.transition(
                name,
                &[
                    ZoneState::Running,
                    ZoneState::Ready,
                    ZoneState::ShuttingDown,
                    ZoneState::Down,
                    ZoneState::Installed,
                ],
                ZoneState::Installed,
                "zone is not running",
            )
        })
    }

    fn uninstall<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.transition(
                name,
                &[
                    ZoneState::Installed,
                    ZoneState::Incomplete,
                    ZoneState::Down,
                    ZoneState::Configured,
                ],
                ZoneState::Configured,
                "zone is running; cannot uninstall",
            )
        })
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut s = self.lock();
            let entry = s.zones.get(name).ok_or_else(|| not_found(name))?;
            if entry.state != ZoneState::Configured {
                return Err(tool_error(&format!(
                    "zonecfg: zone '{name}': zone is installed; cannot delete"
                )));
            }
            s.zones.remove(name);
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

    use super::*;
    use crate::FailureKind;

    fn spec(name: &str) -> ZoneSpec {
        ZoneSpec {
            name: name.to_owned(),
            brand: "lx".to_owned(),
            zonepath: format!("/tank/zones/{name}"),
            autoboot: true,
            nics: Vec::new(),
            cpu_cap: None,
            memory_cap: None,
            attrs: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn lifecycle() {
        let z = FakeZones::typical();
        assert_eq!(z.list().await.unwrap().len(), 1);
        z.create(&spec("web")).await.unwrap();
        assert_eq!(z.state_of("web"), Some(ZoneState::Configured));
        let dup = z.create(&spec("web")).await.err().unwrap();
        assert_eq!(dup.kind(), FailureKind::Exists);
        let not_installed = z.boot("web").await.err().unwrap();
        assert_eq!(not_installed.kind(), FailureKind::Conflict);
        z.install("web", &InstallSource::Prepared).await.unwrap();
        z.boot("web").await.unwrap();
        assert_eq!(z.state_of("web"), Some(ZoneState::Running));
        assert_eq!(
            z.boot("web").await.err().unwrap().kind(),
            FailureKind::Exists
        );
        assert_eq!(
            z.uninstall("web").await.err().unwrap().kind(),
            FailureKind::Conflict
        );
        z.shutdown("web", true).await.unwrap();
        assert_eq!(z.state_of("web"), Some(ZoneState::Running));
        z.shutdown("web", false).await.unwrap();
        assert_eq!(z.state_of("web"), Some(ZoneState::Installed));
        z.set_attr("web", "mandrake-id", "abc").await.unwrap();
        let mut s = spec("web");
        s.autoboot = false;
        s.attrs.insert("hostname".to_owned(), "web".to_owned());
        z.update(&s).await.unwrap();
        let cfg = z.config("web").await.unwrap();
        assert!(!cfg.autoboot);
        assert_eq!(
            cfg.attrs.get("mandrake-id").map(String::as_str),
            Some("abc")
        );
        assert_eq!(cfg.attrs.get("hostname").map(String::as_str), Some("web"));
        assert_eq!(
            z.delete("web").await.err().unwrap().kind(),
            FailureKind::Conflict
        );
        z.uninstall("web").await.unwrap();
        z.delete("web").await.unwrap();
        assert!(matches!(z.config("web").await, Err(ZoneError::NotFound(_))));
        assert_eq!(
            z.boot("web").await.err().unwrap().kind(),
            FailureKind::NotFound
        );
    }
}
