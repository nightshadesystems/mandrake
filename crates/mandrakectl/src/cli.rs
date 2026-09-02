//! Command-line definitions. Every subcommand maps to one API call
//! (spec §9); `--all` on list commands follows cursors on the same call.

// Doc comments here double as --help text, so bare URLs and env names
// stay readable rather than Markdown-correct.
#![allow(clippy::doc_markdown)]

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use mandrake_core::{Id, Role, Timestamp};

/// Thin CLI over the Mandrake API.
#[derive(Debug, Parser)]
#[command(name = "mandrakectl", version, about, long_about = None)]
pub struct Cli {
    /// Daemon URL, for example https://host:443. Default: the root Unix
    /// socket when it is connectable, else https://localhost.
    #[arg(long, env = "MANDRAKE_SERVER", global = true)]
    pub server: Option<String>,

    /// Use this Unix socket instead of HTTPS (root, no auth needed).
    #[arg(long, env = "MANDRAKE_SOCKET", global = true)]
    pub socket: Option<PathBuf>,

    /// Bearer token. Also MANDRAKE_TOKEN or ~/.config/mandrake/token.
    #[arg(long, env = "MANDRAKE_TOKEN", global = true, hide_env_values = true)]
    pub token: Option<String>,

    /// Read the bearer token from this file.
    #[arg(long, global = true)]
    pub token_file: Option<PathBuf>,

    /// Print JSON. Default when stdout is not a terminal.
    #[arg(long, global = true)]
    pub json: bool,

    /// PEM certificate (or bundle) to trust for the daemon.
    #[arg(long, global = true)]
    pub ca: Option<PathBuf>,

    /// Trust only a certificate with this SHA-256 fingerprint (as printed
    /// by mandraked at startup). Colons optional.
    #[arg(long, global = true, conflicts_with_all = ["ca", "insecure"])]
    pub fingerprint: Option<String>,

    /// Skip certificate verification. For recovery only.
    #[arg(long, global = true)]
    pub insecure: bool,

    /// Request timeout in seconds.
    #[arg(long, global = true, default_value_t = 30)]
    pub timeout: u64,

    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Liveness check (GET /health).
    Health,
    /// Who am I (GET /auth/session).
    Session,
    /// Host identity and resources.
    #[command(subcommand)]
    System(SystemCmd),
    /// Local users.
    #[command(subcommand)]
    Users(UsersCmd),
    /// API tokens.
    #[command(subcommand)]
    Tokens(TokensCmd),
    /// Audit log.
    #[command(subcommand)]
    Audit(AuditCmd),
    /// Long-running jobs.
    #[command(subcommand)]
    Jobs(JobsCmd),
}

/// Paging flags shared by list commands.
#[derive(Debug, Args)]
pub struct Paging {
    /// Page size (1-500).
    #[arg(long)]
    pub limit: Option<u32>,
    /// Follow cursors until the end.
    #[arg(long)]
    pub all: bool,
    /// Start from this cursor.
    #[arg(long)]
    pub cursor: Option<String>,
}

/// `system` subcommands.
#[derive(Debug, Subcommand)]
pub enum SystemCmd {
    /// Host identity (GET /system).
    Info,
    /// CPU, load, memory (GET /system/resources).
    Resources,
}

/// How a password is supplied. Never on the command line of a shared host
/// if you can avoid it; prefer --password-stdin or MANDRAKE_PASSWORD.
#[derive(Debug, Args)]
pub struct PasswordArgs {
    /// The password.
    #[arg(
        long,
        env = "MANDRAKE_PASSWORD",
        hide_env_values = true,
        conflicts_with = "password_stdin"
    )]
    pub password: Option<String>,
    /// Read the password from the first line of stdin.
    #[arg(long)]
    pub password_stdin: bool,
}

/// `users` subcommands.
#[derive(Debug, Subcommand)]
pub enum UsersCmd {
    /// List users (GET /users).
    List {
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one user (GET /users/{id}).
    Get {
        /// User id.
        id: Id,
    },
    /// Create a user (POST /users). Admin.
    Create {
        /// Login name: lowercase, 1-32 characters.
        username: String,
        /// admin, operator, or viewer.
        #[arg(long)]
        role: Role,
        /// Display name.
        #[arg(long)]
        display_name: Option<String>,
        #[command(flatten)]
        password: PasswordArgs,
    },
    /// Change role, display name, or enabled state (PATCH /users/{id}). Admin.
    Update {
        /// User id.
        id: Id,
        /// New role.
        #[arg(long)]
        role: Option<Role>,
        /// New display name.
        #[arg(long)]
        display_name: Option<String>,
        /// Disable the account.
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
        /// Enable the account.
        #[arg(long)]
        enable: bool,
    },
    /// Delete a user (DELETE /users/{id}). Admin.
    Delete {
        /// User id.
        id: Id,
    },
    /// Set a password (PUT /users/{id}/password).
    Passwd {
        /// User id.
        id: Id,
        #[command(flatten)]
        password: PasswordArgs,
        /// Current password; required when changing your own.
        #[arg(long, conflicts_with = "current_stdin")]
        current: Option<String>,
        /// Read the current password from the second line of stdin.
        #[arg(long)]
        current_stdin: bool,
    },
}

/// `tokens` subcommands.
#[derive(Debug, Subcommand)]
pub enum TokensCmd {
    /// List tokens (GET /tokens).
    List {
        /// Another user's tokens. Admin.
        #[arg(long)]
        user: Option<Id>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one token's metadata (GET /tokens/{id}).
    Get {
        /// Token id.
        id: Id,
    },
    /// Create a token; the secret is printed once (POST /tokens).
    Create {
        /// A name for the token.
        name: String,
        /// Owner other than yourself. Admin; required over the socket.
        #[arg(long)]
        user: Option<Id>,
        /// Lifetime in seconds (at least 60). Omit for no expiry.
        #[arg(long)]
        expires_in: Option<i64>,
    },
    /// Revoke a token (DELETE /tokens/{id}).
    Revoke {
        /// Token id.
        id: Id,
    },
}

/// `audit` subcommands.
#[derive(Debug, Subcommand)]
pub enum AuditCmd {
    /// List audit entries, newest first (GET /audit).
    List {
        /// Exact action, for example user.create.
        #[arg(long)]
        action: Option<String>,
        /// Only this actor id.
        #[arg(long)]
        actor: Option<Id>,
        /// Only this object id.
        #[arg(long)]
        object: Option<Id>,
        /// Not before (RFC 3339).
        #[arg(long)]
        since: Option<Timestamp>,
        /// Not after (RFC 3339).
        #[arg(long)]
        until: Option<Timestamp>,
        #[command(flatten)]
        paging: Paging,
    },
}

/// `jobs` subcommands.
#[derive(Debug, Subcommand)]
pub enum JobsCmd {
    /// List jobs, newest first (GET /jobs).
    List {
        /// queued, running, succeeded, failed, or cancelled.
        #[arg(long, value_parser = ["queued", "running", "succeeded", "failed", "cancelled"])]
        state: Option<String>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one job (GET /jobs/{id}).
    Get {
        /// Job id.
        id: Id,
    },
}
