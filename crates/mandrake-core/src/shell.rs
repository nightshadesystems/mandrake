//! Running illumos tooling (ADR-0003, ADR-0011).
//!
//! Every driver builds a [`Command`] as an argument vector, never a shell
//! string, and hands it to a [`Runner`]. The [`SystemRunner`] spawns the
//! program, prefixing `pfexec` for privileged commands; the
//! [`ScriptedRunner`] answers from canned outputs so a driver's command
//! construction is testable on any host.

use std::{fmt, future::Future, pin::Pin, sync::Mutex};

use serde::{Deserialize, Serialize};

/// A boxed future, so `Runner` stays object safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where `pfexec` lives on illumos.
pub const PFEXEC: &str = "/usr/bin/pfexec";

/// A program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Program name or path.
    pub program: String,
    /// Arguments, each passed verbatim.
    pub args: Vec<String>,
    /// Run through `pfexec` (a mutation that needs the RBAC profile).
    pub privileged: bool,
}

impl Command {
    /// A command with no arguments.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            privileged: false,
        }
    }

    /// Append one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append several arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Mark as privileged: run through `pfexec`.
    #[must_use]
    pub fn privileged(mut self) -> Self {
        self.privileged = true;
        self
    }

    /// The command line as it would be typed, for logs and errors.
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.privileged {
            s.push_str("pfexec ");
        }
        s.push_str(&self.plain());
        s
    }

    /// The command line without the `pfexec` prefix; what scripts match.
    pub fn plain(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.program);
        for a in &self.args {
            s.push(' ');
            if a.is_empty() || a.contains(' ') {
                s.push('\'');
                s.push_str(a);
                s.push('\'');
            } else {
                s.push_str(a);
            }
        }
        s
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// What a program produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// Exit status; `-1` when killed by a signal.
    pub status: i32,
    /// Standard output, lossily decoded.
    pub stdout: String,
    /// Standard error, lossily decoded.
    pub stderr: String,
}

/// Why a command did not succeed.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum ShellError {
    /// The program could not be started.
    #[error("cannot run {command}: {reason}")]
    Spawn {
        /// The command line.
        command: String,
        /// The OS error text.
        reason: String,
    },
    /// The program ran and reported failure.
    #[error("{command} failed (status {status}): {stderr}")]
    Failed {
        /// The command line.
        command: String,
        /// Exit status.
        status: i32,
        /// Trimmed standard error.
        stderr: String,
    },
    /// The program was not scripted (fake runner only).
    #[error("no scripted answer for {command}")]
    Unscripted {
        /// The command line.
        command: String,
    },
}

impl ShellError {
    /// The tool's own message, when it produced one.
    pub fn stderr(&self) -> &str {
        match self {
            Self::Failed { stderr, .. } => stderr,
            Self::Spawn { reason, .. } => reason,
            Self::Unscripted { .. } => "",
        }
    }
}

/// Runs commands. Implemented by the system and by test fakes.
pub trait Runner: Send + Sync {
    /// Run `cmd` to completion. A non-zero exit is `ShellError::Failed`.
    fn run<'a>(&'a self, cmd: &'a Command) -> BoxFuture<'a, Result<Output, ShellError>>;
}

/// The real thing: spawns the program, `pfexec` first when privileged.
#[derive(Debug, Clone)]
pub struct SystemRunner {
    pfexec: String,
}

impl Default for SystemRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemRunner {
    /// A runner using the standard `pfexec` path.
    pub fn new() -> Self {
        Self {
            pfexec: PFEXEC.to_owned(),
        }
    }

    /// A runner with a different privilege-escalation program (tests, dev).
    pub fn with_pfexec(pfexec: impl Into<String>) -> Self {
        Self {
            pfexec: pfexec.into(),
        }
    }
}

impl Runner for SystemRunner {
    fn run<'a>(&'a self, cmd: &'a Command) -> BoxFuture<'a, Result<Output, ShellError>> {
        Box::pin(async move {
            let mut process = if cmd.privileged {
                let mut p = tokio::process::Command::new(&self.pfexec);
                p.arg(&cmd.program);
                p
            } else {
                tokio::process::Command::new(&cmd.program)
            };
            process.args(&cmd.args);
            process.env("LC_ALL", "C");
            process.stdin(std::process::Stdio::null());
            process.kill_on_drop(true);
            tracing::debug!(command = %cmd, "running");
            let output = process.output().await.map_err(|e| ShellError::Spawn {
                command: cmd.display(),
                reason: e.to_string(),
            })?;
            let out = Output {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            };
            if output.status.success() {
                Ok(out)
            } else {
                tracing::warn!(command = %cmd, status = out.status, stderr = %out.stderr.trim(), "command failed");
                Err(ShellError::Failed {
                    command: cmd.display(),
                    status: out.status,
                    stderr: out.stderr.trim().to_owned(),
                })
            }
        })
    }
}

/// A canned answer for the scripted runner.
#[derive(Debug, Clone)]
pub struct Script {
    /// Matches when the command line starts with this text.
    pub prefix: String,
    /// What to answer.
    pub answer: Result<Output, ShellError>,
}

/// Answers commands from scripts and records everything it was asked.
///
/// Scripts are matched in order by command-line prefix, so a driver's
/// argument construction is asserted exactly.
#[derive(Debug, Default)]
pub struct ScriptedRunner {
    scripts: Mutex<Vec<Script>>,
    calls: Mutex<Vec<Command>>,
}

impl ScriptedRunner {
    /// An empty runner; every command is unscripted until scripted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer commands starting with `prefix` with `stdout` and status 0.
    pub fn ok(&self, prefix: impl Into<String>, stdout: impl Into<String>) -> &Self {
        self.push(Script {
            prefix: prefix.into(),
            answer: Ok(Output {
                status: 0,
                stdout: stdout.into(),
                stderr: String::new(),
            }),
        });
        self
    }

    /// Answer commands starting with `prefix` with a failure.
    pub fn fail(&self, prefix: impl Into<String>, status: i32, stderr: impl Into<String>) -> &Self {
        let prefix = prefix.into();
        self.push(Script {
            answer: Err(ShellError::Failed {
                command: prefix.clone(),
                status,
                stderr: stderr.into(),
            }),
            prefix,
        });
        self
    }

    fn push(&self, script: Script) {
        if let Ok(mut s) = self.scripts.lock() {
            s.push(script);
        }
    }

    /// Every command run so far, in order.
    pub fn calls(&self) -> Vec<Command> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// The command lines run so far, in order.
    pub fn lines(&self) -> Vec<String> {
        self.calls().iter().map(Command::display).collect()
    }
}

impl Runner for ScriptedRunner {
    fn run<'a>(&'a self, cmd: &'a Command) -> BoxFuture<'a, Result<Output, ShellError>> {
        Box::pin(async move {
            if let Ok(mut c) = self.calls.lock() {
                c.push(cmd.clone());
            }
            let line = cmd.plain();
            let found = self.scripts.lock().ok().and_then(|s| {
                s.iter()
                    .find(|s| line.starts_with(&s.prefix))
                    .map(|s| s.answer.clone())
            });
            found.unwrap_or(Err(ShellError::Unscripted {
                command: cmd.display(),
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_quotes_only_when_needed() {
        let c = Command::new("zfs")
            .args(["create", "-o", "compression=lz4", "tank/a b"])
            .privileged();
        assert_eq!(
            c.display(),
            "pfexec zfs create -o compression=lz4 'tank/a b'"
        );
    }

    #[tokio::test]
    async fn scripted_runner_matches_prefixes_and_records() {
        let r = ScriptedRunner::new();
        r.ok("zfs list", "tank\t1\n")
            .fail("zfs destroy", 1, "dataset is busy");
        let list = Command::new("zfs").args(["list", "-Hp"]);
        let out = r.run(&list).await.unwrap_or_default();
        assert_eq!(out.stdout, "tank\t1\n");
        let destroy = Command::new("zfs").args(["destroy", "tank/x"]).privileged();
        let err = r.run(&destroy).await.err();
        assert!(matches!(err, Some(ShellError::Failed { status: 1, .. })));
        assert!(matches!(
            r.run(&Command::new("zpool")).await.err(),
            Some(ShellError::Unscripted { .. })
        ));
        assert_eq!(
            r.lines(),
            vec!["zfs list -Hp", "pfexec zfs destroy tank/x", "zpool"]
        );
    }

    #[tokio::test]
    async fn system_runner_reports_spawn_failures() {
        let r = SystemRunner::new();
        let err = r
            .run(&Command::new("/definitely/not/a/program"))
            .await
            .err();
        assert!(matches!(err, Some(ShellError::Spawn { .. })));
    }
}

/// A coarse classification of a driver failure, for HTTP mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The object does not exist.
    NotFound,
    /// The object already exists.
    Exists,
    /// Busy, has dependents, or otherwise refused for now.
    Conflict,
    /// Not permitted.
    Forbidden,
    /// Bad arguments.
    Invalid,
    /// Anything else.
    Other,
}
