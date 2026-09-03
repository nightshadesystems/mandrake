//! The real driver: `zonecfg` and `zoneadm` through a [`Runner`].

use std::sync::Arc;

use mandrake_core::shell::{Command, Runner};

use crate::{
    BoxFuture, InstallSource, Result, ZoneConfig, ZoneError, ZoneSpec, ZoneSummary, Zones, parse,
};

/// Shells out to the illumos tools.
#[derive(Clone)]
pub struct ZonesCli {
    runner: Arc<dyn Runner>,
}

impl ZonesCli {
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

    /// `pfexec zonecfg -z <name> "<commands; ...>"`: one argument, no shell.
    async fn zonecfg(&self, name: &str, commands: &[String]) -> Result<()> {
        self.run(
            Command::new("zonecfg")
                .args(["-z", name])
                .arg(commands.join("; "))
                .privileged(),
        )
        .await
    }

    async fn zoneadm(&self, name: &str, args: &[&str]) -> Result<()> {
        self.run(
            Command::new("zoneadm")
                .args(["-z", name])
                .args(args.iter().copied())
                .privileged(),
        )
        .await
    }
}

/// The `zoneadm install` arguments for a source (ADR-0012).
///
/// `Prepared` relies on the brand adopting a populated zonepath dataset;
/// the flag is confirmed against OmniOS r151054 and lives only here.
pub fn install_args(source: &InstallSource) -> Vec<String> {
    let mut args = vec!["install".to_owned()];
    match source {
        InstallSource::Packages => {}
        InstallSource::Archive(path) => {
            args.push("-s".to_owned());
            args.push(path.clone());
        }
        InstallSource::Prepared => {
            args.push("-x".to_owned());
            args.push("nodataset".to_owned());
        }
    }
    args
}

impl Zones for ZonesCli {
    fn list(&self) -> BoxFuture<'_, Result<Vec<ZoneSummary>>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zoneadm").args(["list", "-pc"]))
                .await?;
            parse::zoneadm_list(&out)
        })
    }

    fn config<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<ZoneConfig>> {
        Box::pin(async move {
            let out = self
                .stdout(Command::new("zonecfg").args(["-z", name, "export"]))
                .await
                .map_err(|e| match e.kind() {
                    crate::FailureKind::NotFound => ZoneError::NotFound(name.to_owned()),
                    _ => e,
                })?;
            parse::zonecfg_export(name, &out)
        })
    }

    fn create<'a>(&'a self, spec: &'a ZoneSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.zonecfg(&spec.name, &parse::render_create(spec)).await })
    }

    fn update<'a>(&'a self, spec: &'a ZoneSpec) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let current = self.config(&spec.name).await?;
            self.zonecfg(&spec.name, &parse::render_update(&current, spec))
                .await
        })
    }

    fn set_attr<'a>(
        &'a self,
        name: &'a str,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let current = self.config(name).await?;
            let exists = current.attrs.contains_key(key);
            self.zonecfg(name, &parse::render_set_attr(key, value, exists))
                .await
        })
    }

    fn install<'a>(
        &'a self,
        name: &'a str,
        source: &'a InstallSource,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let args = install_args(source);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            self.zoneadm(name, &args).await
        })
    }

    fn boot<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.zoneadm(name, &["boot"]).await })
    }

    fn shutdown<'a>(&'a self, name: &'a str, reboot: bool) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if reboot {
                self.zoneadm(name, &["shutdown", "-r"]).await
            } else {
                self.zoneadm(name, &["shutdown"]).await
            }
        })
    }

    fn halt<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.zoneadm(name, &["halt"]).await })
    }

    fn uninstall<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.zoneadm(name, &["uninstall", "-F"]).await })
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("zonecfg")
                    .args(["-z", name, "delete", "-F"])
                    .privileged(),
            )
            .await
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

    use mandrake_core::{shell::ScriptedRunner, zone::ZoneNic};

    use super::*;

    fn driver(r: &Arc<ScriptedRunner>) -> ZonesCli {
        ZonesCli::new(Arc::clone(r) as Arc<dyn Runner>)
    }

    #[tokio::test]
    async fn lifecycle_commands_are_exact() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok(
            "zonecfg -z web export",
            include_str!("../testdata/zonecfg-export.lx.synthetic.txt"),
        )
        .ok("zonecfg", "")
        .ok("zoneadm", "");
        let d = driver(&r);
        let spec = ZoneSpec {
            name: "web".to_owned(),
            brand: "lx".to_owned(),
            zonepath: "/tank/zones/web".to_owned(),
            autoboot: true,
            nics: vec![ZoneNic {
                name: "net0".to_owned(),
                over: "stub0".to_owned(),
                mac: None,
                vid: None,
                address: None,
                gateway: None,
            }],
            cpu_cap: None,
            memory_cap: None,
            devices: Vec::new(),
            fs: Vec::new(),
            attrs: [("mandrake-id".to_owned(), "abc".to_owned())]
                .into_iter()
                .collect(),
        };
        d.create(&spec).await.unwrap();
        d.install("web", &InstallSource::Prepared).await.unwrap();
        d.install(
            "web",
            &InstallSource::Archive("/tank/images/x.zfs.gz".to_owned()),
        )
        .await
        .unwrap();
        d.boot("web").await.unwrap();
        d.shutdown("web", true).await.unwrap();
        d.shutdown("web", false).await.unwrap();
        d.halt("web").await.unwrap();
        d.set_attr("web", "mandrake-image", "img").await.unwrap();
        d.uninstall("web").await.unwrap();
        d.delete("web").await.unwrap();
        assert_eq!(
            r.lines(),
            vec![
                "pfexec zonecfg -z web 'create -b; set brand=lx; set zonepath=/tank/zones/web; \
                 set autoboot=true; set ip-type=exclusive; add anet; set linkname=net0; \
                 set lower-link=stub0; end; add attr; set name=mandrake-id; set type=string; \
                 set value=\"abc\"; end; commit'",
                "pfexec zoneadm -z web install -x nodataset",
                "pfexec zoneadm -z web install -s /tank/images/x.zfs.gz",
                "pfexec zoneadm -z web boot",
                "pfexec zoneadm -z web shutdown -r",
                "pfexec zoneadm -z web shutdown",
                "pfexec zoneadm -z web halt",
                "zonecfg -z web export",
                "pfexec zonecfg -z web 'add attr; set name=mandrake-image; set type=string; \
                 set value=\"img\"; end; commit'",
                "pfexec zoneadm -z web uninstall -F",
                "pfexec zonecfg -z web delete -F",
            ]
        );
    }

    #[tokio::test]
    async fn list_and_config_read_the_testdata() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok(
            "zoneadm list -pc",
            include_str!("../testdata/zoneadm-list-pc.synthetic.txt"),
        )
        .ok(
            "zonecfg -z web export",
            include_str!("../testdata/zonecfg-export.lx.synthetic.txt"),
        )
        .fail(
            "zonecfg -z nope export",
            1,
            "zonecfg: No such zone configured\nUse 'create' to begin configuring a new zone.",
        );
        let d = driver(&r);
        assert_eq!(d.list().await.unwrap().len(), 3);
        let cfg = d.config("web").await.unwrap();
        assert_eq!(cfg.nics.len(), 1);
        assert!(matches!(
            d.config("nope").await,
            Err(ZoneError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn failures_classify() {
        let r = Arc::new(ScriptedRunner::new());
        r.fail(
            "zoneadm -z web uninstall",
            1,
            "zoneadm: zone 'web': zone is running; cannot uninstall",
        )
        .fail(
            "zoneadm -z web boot",
            1,
            "zoneadm: zone 'web': zone is already running",
        );
        let d = driver(&r);
        assert_eq!(
            d.uninstall("web").await.err().unwrap().kind(),
            crate::FailureKind::Conflict
        );
        assert_eq!(
            d.boot("web").await.err().unwrap().kind(),
            crate::FailureKind::Exists
        );
    }
}
