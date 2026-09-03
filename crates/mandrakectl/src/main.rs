//! `mandrakectl`: thin CLI over the Mandrake API.
//!
//! Every command maps to exactly one API call. JSON output by default when
//! stdout is not a TTY. Works over the daemon's Unix socket without auth
//! when run as root on the host so recovery needs neither network nor
//! console (spec §9, ADR-0007).
//!
//! Exit codes: 0 success, 1 the daemon refused (a problem was printed),
//! 2 usage, configuration, or transport failure.

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

mod cli;
mod client;
mod cmd;
mod images;
mod network;
mod output;
mod storage;
mod vms;
mod zones;

use std::process::ExitCode;

use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match cmd::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(client::Error::Api(problem)) => {
            eprintln!("mandrakectl: {problem}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("mandrakectl: {e}");
            ExitCode::from(2)
        }
    }
}
