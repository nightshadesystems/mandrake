//! `pkg` for updates (ADR-0015): refresh, the dry-run plan, and the
//! update into a named boot environment. Root through `pfexec`; the
//! output of `pkg update -nv` is parsed here and tested against
//! `testdata/`.

use std::sync::{Arc, Mutex};

use mandrake_core::{
    shell::{BoxFuture, Command, Runner, ShellError},
    system::{UpdateAction, UpdatePackage},
};

/// Why `pkg` failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PkgError {
    /// The tool failed.
    #[error(transparent)]
    Command(#[from] ShellError),
    /// The dry run was not understood.
    #[error("cannot parse `pkg update -nv` output: {0}")]
    Parse(String),
}

/// `pkg`'s exit status for "nothing to do".
pub const NOTHING_TO_DO: i32 = 4;

/// What the dry run said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRun {
    /// Packages, in `pkg`'s order.
    pub packages: Vec<UpdatePackage>,
    /// `Create boot environment: Yes`.
    pub creates_be: bool,
    /// `Rebuild boot archive: Yes`.
    pub rebuilds_boot_archive: bool,
    /// The output, kept verbatim.
    pub raw: String,
}

impl DryRun {
    /// Nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// The Mandrake version the plan moves to, from the incorporation's
    /// new version (`0.2.0-151054.0:...` gives `0.2.0`).
    pub fn mandrake_version(&self) -> Option<String> {
        self.packages
            .iter()
            .find(|p| p.name == crate::updates::INCORPORATION)
            .and_then(|p| p.new_version.as_deref())
            .map(|v| v.split('-').next().unwrap_or(v).to_owned())
    }
}

fn yes(line: &str) -> bool {
    line.rsplit(':')
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("yes"))
}

fn version(s: &str) -> Option<String> {
    let s = s.trim();
    (s != "None" && !s.is_empty()).then(|| s.to_owned())
}

/// Parse `pkg update -nv` output. Exit status 4 (nothing to do) is
/// handled by the caller; this sees stdout only.
pub fn parse_dry_run(out: &str) -> Result<DryRun, PkgError> {
    let mut run = DryRun {
        packages: Vec::new(),
        creates_be: false,
        rebuilds_boot_archive: false,
        raw: out.to_owned(),
    };
    if out.contains("No updates available") {
        return Ok(run);
    }
    let mut in_changed = false;
    let mut current: Option<String> = None;
    let mut saw_header = false;
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !in_changed {
            if trimmed.starts_with("Create boot environment:") {
                run.creates_be = yes(trimmed);
                saw_header = true;
            } else if trimmed.starts_with("Rebuild boot archive:") {
                run.rebuilds_boot_archive = yes(trimmed);
                saw_header = true;
            } else if trimmed.starts_with("Packages to ") {
                saw_header = true;
            } else if trimmed == "Changed packages:" {
                in_changed = true;
            }
            continue;
        }
        // Inside "Changed packages": a section ends at the next unindented
        // heading that ends with a colon (Services:, Changed fmris:, ...).
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            if trimmed.ends_with(':') {
                in_changed = false;
                continue;
            }
            // A publisher line; nothing to record.
            continue;
        }
        if let Some((old, new)) = trimmed.split_once("->") {
            let Some(name) = current.take() else {
                return Err(PkgError::Parse(format!(
                    "version line without a package: {trimmed}"
                )));
            };
            let old_version = version(old);
            let new_version = version(new);
            let action = match (&old_version, &new_version) {
                (None, Some(_)) => UpdateAction::Install,
                (Some(_), None) => UpdateAction::Remove,
                _ => UpdateAction::Update,
            };
            run.packages.push(UpdatePackage {
                name,
                action,
                old_version,
                new_version,
            });
        } else if indent == 2 {
            current = Some(trimmed.to_owned());
        } else {
            return Err(PkgError::Parse(format!("unexpected line: {trimmed}")));
        }
    }
    if !saw_header {
        return Err(PkgError::Parse(
            "no plan header (Packages to ..., Create boot environment)".to_owned(),
        ));
    }
    Ok(run)
}

/// Typed `pkg` operations.
pub trait Pkg: Send + Sync {
    /// `pkg refresh --full`.
    fn refresh(&self) -> BoxFuture<'_, Result<(), PkgError>>;
    /// `pkg update -nv`, parsed; empty when there is nothing to do.
    fn dry_run(&self) -> BoxFuture<'_, Result<DryRun, PkgError>>;
    /// `pkg update -v --be-name <name>`; returns `pkg`'s output.
    fn update<'a>(&'a self, be_name: &'a str) -> BoxFuture<'a, Result<String, PkgError>>;
}

/// Shells out to `pkg`.
#[derive(Clone)]
pub struct PkgCli {
    runner: Arc<dyn Runner>,
}

impl PkgCli {
    /// A driver over `runner`.
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }
}

impl Pkg for PkgCli {
    fn refresh(&self) -> BoxFuture<'_, Result<(), PkgError>> {
        Box::pin(async move {
            self.runner
                .run(&Command::new("pkg").args(["refresh", "--full"]).privileged())
                .await?;
            Ok(())
        })
    }

    fn dry_run(&self) -> BoxFuture<'_, Result<DryRun, PkgError>> {
        Box::pin(async move {
            let cmd = Command::new("pkg").args(["update", "-nv"]).privileged();
            match self.runner.run(&cmd).await {
                Ok(out) => parse_dry_run(&out.stdout),
                Err(ShellError::Failed { status, .. }) if status == NOTHING_TO_DO => Ok(DryRun {
                    packages: Vec::new(),
                    creates_be: false,
                    rebuilds_boot_archive: false,
                    raw: "No updates available for this image.".to_owned(),
                }),
                Err(e) => Err(e.into()),
            }
        })
    }

    fn update<'a>(&'a self, be_name: &'a str) -> BoxFuture<'a, Result<String, PkgError>> {
        Box::pin(async move {
            let out = self
                .runner
                .run(
                    &Command::new("pkg")
                        .args(["update", "-v", "--be-name", be_name])
                        .privileged(),
                )
                .await?;
            Ok(out.stdout)
        })
    }
}

/// A `pkg` that answers from a canned dry run and records updates.
#[derive(Debug, Default)]
pub struct FakePkg {
    dry_run: Mutex<String>,
    updated: Mutex<Vec<String>>,
    refreshes: Mutex<u32>,
}

impl FakePkg {
    /// Nothing to do.
    pub fn new() -> Self {
        Self {
            dry_run: Mutex::new("No updates available for this image.".to_owned()),
            ..Self::default()
        }
    }

    /// Answer `pkg update -nv` with `text`.
    #[must_use]
    pub fn with_dry_run(self, text: &str) -> Self {
        if let Ok(mut d) = self.dry_run.lock() {
            text.clone_into(&mut d);
        }
        self
    }

    /// BE names updates were run into.
    pub fn updated(&self) -> Vec<String> {
        self.updated.lock().map(|u| u.clone()).unwrap_or_default()
    }

    /// How many refreshes ran.
    pub fn refreshes(&self) -> u32 {
        self.refreshes.lock().map_or(0, |r| *r)
    }
}

impl Pkg for FakePkg {
    fn refresh(&self) -> BoxFuture<'_, Result<(), PkgError>> {
        Box::pin(async move {
            if let Ok(mut r) = self.refreshes.lock() {
                *r += 1;
            }
            Ok(())
        })
    }

    fn dry_run(&self) -> BoxFuture<'_, Result<DryRun, PkgError>> {
        Box::pin(async move {
            let text = self.dry_run.lock().map(|d| d.clone()).unwrap_or_default();
            parse_dry_run(&text)
        })
    }

    fn update<'a>(&'a self, be_name: &'a str) -> BoxFuture<'a, Result<String, PkgError>> {
        Box::pin(async move {
            if let Ok(mut u) = self.updated.lock() {
                u.push(be_name.to_owned());
            }
            if let Ok(mut d) = self.dry_run.lock() {
                "No updates available for this image.".clone_into(&mut d);
            }
            Ok(format!("Updated into {be_name}\n"))
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use mandrake_core::shell::ScriptedRunner;

    use super::*;

    const PLAN: &str = include_str!("../testdata/pkg-update-nv.synthetic.txt");

    #[test]
    fn parses_the_dry_run() {
        let run = parse_dry_run(PLAN).unwrap();
        assert!(run.creates_be && run.rebuilds_boot_archive);
        assert_eq!(run.packages.len(), 5);
        assert_eq!(run.packages[0].name, crate::updates::INCORPORATION);
        assert_eq!(run.packages[0].action, UpdateAction::Update);
        assert_eq!(
            run.packages[0].old_version.as_deref(),
            Some("0.1.0-151054.0:20260901T120000Z")
        );
        assert_eq!(run.packages[3].name, "library/security/openssl-3");
        assert_eq!(run.packages[3].action, UpdateAction::Install);
        assert!(run.packages[3].old_version.is_none());
        assert_eq!(run.packages[4].action, UpdateAction::Remove);
        assert!(run.packages[4].new_version.is_none());
        assert_eq!(run.mandrake_version().as_deref(), Some("0.2.0"));
        assert!(!run.is_empty());
    }

    #[test]
    fn nothing_to_do_and_garbage() {
        let run = parse_dry_run("No updates available for this image.").unwrap();
        assert!(run.is_empty() && !run.creates_be);
        assert!(parse_dry_run("something else entirely").is_err());
        assert!(parse_dry_run("Changed packages:\n    1.0 -> 2.0\n").is_err());
    }

    #[tokio::test]
    async fn cli_handles_status_four_and_privileges() {
        let runner = Arc::new(ScriptedRunner::new());
        runner.ok("pkg refresh", "");
        runner.fail(
            "pkg update -nv",
            NOTHING_TO_DO,
            "No updates available for this image.",
        );
        runner.ok("pkg update -v --be-name", "done");
        let cli = PkgCli::new(runner.clone());
        cli.refresh().await.unwrap();
        assert!(cli.dry_run().await.unwrap().is_empty());
        assert_eq!(cli.update("mandrake-0.2.0").await.unwrap(), "done");
        let lines = runner.lines();
        assert_eq!(lines[0], "pfexec pkg refresh --full");
        assert_eq!(lines[1], "pfexec pkg update -nv");
        assert_eq!(lines[2], "pfexec pkg update -v --be-name mandrake-0.2.0");
    }

    #[tokio::test]
    async fn fake_records_updates() {
        let f = FakePkg::new().with_dry_run(PLAN);
        assert_eq!(f.dry_run().await.unwrap().packages.len(), 5);
        f.update("mandrake-0.2.0").await.unwrap();
        assert_eq!(f.updated(), vec!["mandrake-0.2.0".to_owned()]);
        assert!(f.dry_run().await.unwrap().is_empty());
    }
}
