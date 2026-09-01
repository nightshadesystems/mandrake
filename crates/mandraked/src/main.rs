//! `mandraked`: the single daemon that owns Mandrake host management.
//!
//! Runs as `svc:/system/mandrake/mandraked:default`, serves the HTTP+JSON API
//! defined in `api/openapi.yaml`, and embeds the web console. Phase 0 ships
//! only this entry point; the server lands in Phase 2.

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
            eprintln!("{NAME}: unknown argument `{other}`");
            eprintln!("usage: {NAME} [--version]");
            ExitCode::from(2)
        }
        None => {
            eprintln!("{NAME} {VERSION}: scaffold build, no server yet (Phase 0)");
            ExitCode::SUCCESS
        }
    }
}
