//! Binary entry point: parse configuration, set up logging, run.

use std::process::ExitCode;

use clap::Parser;
use mandraked::{config::Config, serve};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    let cfg = Config::parse();
    let filter = EnvFilter::try_new(&cfg.log).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    match serve::run(cfg).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "mandraked failed");
            eprintln!("mandraked: {e}");
            ExitCode::FAILURE
        }
    }
}
