//! Command-line and environment configuration. No config file: the SMF
//! method script passes flags (ADR-0007 discussion, Phase 2 plan).

use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

/// `mandraked` options.
#[derive(Debug, Clone, Parser)]
#[command(name = "mandraked", version, about = "Mandrake host management daemon")]
pub struct Config {
    /// Address for the HTTPS listener.
    #[arg(long, env = "MANDRAKED_LISTEN", default_value = "0.0.0.0:443")]
    pub listen: SocketAddr,

    /// Unix socket path; root is trusted without authentication here.
    #[arg(
        long,
        env = "MANDRAKED_SOCKET",
        default_value = "/var/run/mandrake/mandraked.sock"
    )]
    pub socket: PathBuf,

    /// Do not open the Unix socket.
    #[arg(long, env = "MANDRAKED_NO_SOCKET")]
    pub no_socket: bool,

    /// Use in-memory fake drivers instead of the illumos tools, for
    /// developing the console away from illumos. Never on an appliance.
    #[arg(long, env = "MANDRAKED_FAKE_DRIVERS")]
    pub fake_drivers: bool,

    /// SQLite database path.
    #[arg(
        long,
        env = "MANDRAKED_DB",
        default_value = "/var/mandrake/mandrake.db"
    )]
    pub db: PathBuf,

    /// Directory holding cert.pem and key.pem; generated if missing.
    #[arg(long, env = "MANDRAKED_TLS_DIR", default_value = "/etc/mandrake/tls")]
    pub tls_dir: PathBuf,

    /// Hostname to advertise and to put in a generated certificate.
    /// Defaults to the system hostname.
    #[arg(long, env = "MANDRAKED_HOSTNAME")]
    pub hostname: Option<String>,

    /// Log filter, for example `info` or `mandraked=debug,tower_http=info`.
    #[arg(long, env = "MANDRAKED_LOG", default_value = "info")]
    pub log: String,
}
