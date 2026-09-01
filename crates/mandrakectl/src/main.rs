//! `mandrakectl`: thin CLI over the Mandrake API.
//!
//! Every command maps to exactly one API call. JSON output by default when
//! stdout is not a TTY. Works over the daemon's Unix socket without auth when
//! run as root on the host so recovery needs neither network nor console.
//! Phase 0 ships only this entry point; commands land in Phase 2.

use std::process::ExitCode;

/// Program name as reported on `--version`.
const NAME: &str = env!("CARGO_PKG_NAME");
/// Crate version as reported on `--version`.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("{NAME} {VERSION}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("{NAME}: unknown command `{other}`");
            eprintln!("usage: {NAME} [--version]");
            ExitCode::from(2)
        }
        None => {
            eprintln!("{NAME} {VERSION}: scaffold build, no commands yet (Phase 0)");
            ExitCode::from(2)
        }
    }
}
