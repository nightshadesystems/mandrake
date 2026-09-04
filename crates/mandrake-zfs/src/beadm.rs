//! Boot environments through `beadm` (ADR-0015): list, create, activate,
//! destroy. The daemon decides what may be destroyed; this module runs
//! what it is asked and parses `beadm list -H`.

use std::sync::{Arc, Mutex};

use mandrake_core::{
    Id, Timestamp,
    shell::{Command, Runner},
    system::BootEnvironment,
};

use crate::{BoxFuture, Result, ZfsError, fake::tool_error};

/// Typed boot-environment operations.
pub trait BootEnvs: Send + Sync {
    /// `beadm list -H`, in beadm's order.
    fn list(&self) -> BoxFuture<'_, Result<Vec<BootEnvironment>>>;
    /// `beadm create <name>`: a snapshot of the active BE.
    fn create<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `beadm activate <name>`.
    fn activate<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `beadm destroy -F <name>`.
    fn destroy<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// Parse `beadm list -H`: `name;uuid;active;mountpoint;space;policy;created`.
/// The active column holds flags: `N` booted now, `R` boots next, `-` none.
pub fn parse_list(out: &str) -> Result<Vec<BootEnvironment>> {
    let bad = |detail: String| ZfsError::Parse {
        command: "beadm list -H".to_owned(),
        detail,
    };
    let mut items = Vec::new();
    for line in out.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 7 {
            return Err(bad(format!("expected 7 fields, got {}: {line}", f.len())));
        }
        let id: Id = f[1]
            .parse()
            .map_err(|_| bad(format!("bad uuid for {}: {}", f[0], f[1])))?;
        let flags = f[2];
        let space_bytes: u64 = f[4]
            .parse()
            .map_err(|_| bad(format!("bad space for {}: {}", f[0], f[4])))?;
        let created_at = f[6]
            .parse::<i64>()
            .ok()
            .and_then(Timestamp::from_unix)
            .ok_or_else(|| bad(format!("bad creation time for {}: {}", f[0], f[6])))?;
        items.push(BootEnvironment {
            id,
            name: f[0].to_owned(),
            active: flags.contains('R'),
            booted: flags.contains('N'),
            mountpoint: (f[3] != "-" && !f[3].is_empty()).then(|| f[3].to_owned()),
            space_bytes,
            policy: (f[5] != "-" && !f[5].is_empty()).then(|| f[5].to_owned()),
            created_at,
        });
    }
    Ok(items)
}

/// Shells out to `beadm`.
#[derive(Clone)]
pub struct BeadmCli {
    runner: Arc<dyn Runner>,
}

impl BeadmCli {
    /// A driver over `runner`.
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }

    async fn run(&self, cmd: Command) -> Result<()> {
        self.runner.run(&cmd).await?;
        Ok(())
    }
}

impl BootEnvs for BeadmCli {
    fn list(&self) -> BoxFuture<'_, Result<Vec<BootEnvironment>>> {
        Box::pin(async move {
            let out = self
                .runner
                .run(&Command::new("beadm").args(["list", "-H"]))
                .await?;
            parse_list(&out.stdout)
        })
    }

    fn create<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(self.run(Command::new("beadm").args(["create", name]).privileged()))
    }

    fn activate<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(self.run(Command::new("beadm").args(["activate", name]).privileged()))
    }

    fn destroy<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(
            self.run(
                Command::new("beadm")
                    .args(["destroy", "-F", name])
                    .privileged(),
            ),
        )
    }
}

/// In-memory boot environments with beadm's observable rules.
#[derive(Debug, Clone)]
pub struct FakeBeadm {
    state: Arc<Mutex<Vec<BootEnvironment>>>,
}

impl Default for FakeBeadm {
    fn default() -> Self {
        Self::typical()
    }
}

impl FakeBeadm {
    /// One BE, booted and active: `mandrake-0.1.0`.
    pub fn typical() -> Self {
        let be = BootEnvironment {
            id: Id::new(),
            name: "mandrake-0.1.0".to_owned(),
            active: true,
            booted: true,
            mountpoint: Some("/".to_owned()),
            space_bytes: 8_123_456_789,
            policy: Some("static".to_owned()),
            created_at: Timestamp::from_unix(1_756_684_800).unwrap_or_else(Timestamp::now),
        };
        Self {
            state: Arc::new(Mutex::new(vec![be])),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Vec<BootEnvironment>) -> Result<T>) -> Result<T> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| tool_error("fake beadm state poisoned"))?;
        f(&mut guard)
    }
}

impl BootEnvs for FakeBeadm {
    fn list(&self) -> BoxFuture<'_, Result<Vec<BootEnvironment>>> {
        Box::pin(async move { self.with(|s| Ok(s.clone())) })
    }

    fn create<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.with(|s| {
                if s.iter().any(|b| b.name == name) {
                    return Err(tool_error(&format!("BE {name} already exists")));
                }
                let Some(active) = s.iter().find(|b| b.active).cloned() else {
                    return Err(tool_error("no active BE to clone"));
                };
                s.push(BootEnvironment {
                    id: Id::new(),
                    name: name.to_owned(),
                    active: false,
                    booted: false,
                    mountpoint: None,
                    space_bytes: 0,
                    policy: active.policy,
                    created_at: Timestamp::now(),
                });
                Ok(())
            })
        })
    }

    fn activate<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.with(|s| {
                if !s.iter().any(|b| b.name == name) {
                    return Err(ZfsError::NotFound(format!("BE {name}")));
                }
                for b in s.iter_mut() {
                    b.active = b.name == name;
                }
                Ok(())
            })
        })
    }

    fn destroy<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.with(|s| {
                let Some(i) = s.iter().position(|b| b.name == name) else {
                    return Err(ZfsError::NotFound(format!("BE {name}")));
                };
                if s[i].booted || s[i].active {
                    return Err(tool_error(&format!(
                        "Unable to destroy {name}: it is the active or booted BE"
                    )));
                }
                s.remove(i);
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use mandrake_core::shell::ScriptedRunner;

    use super::*;

    const LIST: &str = include_str!("../testdata/beadm-list-H.synthetic.txt");

    #[test]
    fn parses_the_list() {
        let bes = parse_list(LIST).unwrap();
        assert_eq!(bes.len(), 3);
        assert_eq!(bes[0].name, "mandrake-0.1.0");
        assert!(bes[0].active && bes[0].booted);
        assert_eq!(bes[0].mountpoint.as_deref(), Some("/"));
        assert_eq!(bes[0].space_bytes, 8_123_456_789);
        assert_eq!(bes[1].name, "mandrake-0.2.0");
        assert!(bes[1].active && !bes[1].booted, "R only: boots next");
        assert!(bes[1].mountpoint.is_none());
        assert!(!bes[2].active && !bes[2].booted);
        assert!(parse_list("one;two").is_err());
    }

    #[tokio::test]
    async fn cli_runs_beadm_through_pfexec_for_mutations() {
        let runner = Arc::new(ScriptedRunner::new());
        runner.ok("beadm list -H", LIST);
        runner.ok("beadm create", "");
        runner.ok("beadm activate", "");
        runner.ok("beadm destroy", "");
        let cli = BeadmCli::new(runner.clone());
        assert_eq!(cli.list().await.unwrap().len(), 3);
        cli.create("test").await.unwrap();
        cli.activate("test").await.unwrap();
        cli.destroy("test").await.unwrap();
        let lines = runner.lines();
        assert_eq!(lines[0], "beadm list -H");
        assert_eq!(lines[1], "pfexec beadm create test");
        assert_eq!(lines[2], "pfexec beadm activate test");
        assert_eq!(lines[3], "pfexec beadm destroy -F test");
    }

    #[tokio::test]
    async fn fake_follows_beadm_rules() {
        let f = FakeBeadm::typical();
        f.create("next").await.unwrap();
        assert!(f.create("next").await.is_err());
        assert!(f.destroy("mandrake-0.1.0").await.is_err(), "booted");
        f.activate("next").await.unwrap();
        let bes = f.list().await.unwrap();
        assert!(bes.iter().find(|b| b.name == "next").unwrap().active);
        assert!(!bes[0].active && bes[0].booted);
        assert!(f.destroy("next").await.is_err(), "active");
        f.activate("mandrake-0.1.0").await.unwrap();
        f.destroy("next").await.unwrap();
        assert!(matches!(
            f.destroy("next").await,
            Err(ZfsError::NotFound(_))
        ));
    }
}
