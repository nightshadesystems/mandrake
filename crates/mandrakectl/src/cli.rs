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
    /// Pools, datasets, volumes, snapshots, disks.
    #[command(subcommand)]
    Storage(StorageCmd),
    /// Links, addresses, routes.
    #[command(subcommand)]
    Network(NetworkCmd),
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

/// Metadata flags shared by create and update commands.
#[derive(Debug, Args, Default)]
pub struct MetadataArgs {
    /// Display name.
    #[arg(long)]
    pub display_name: Option<String>,
    /// Description.
    #[arg(long)]
    pub description: Option<String>,
    /// A tag; repeat for several.
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// Free-form notes.
    #[arg(long)]
    pub notes: Option<String>,
}

/// `storage` subcommands.
#[derive(Debug, Subcommand)]
pub enum StorageCmd {
    /// Disks and their pool membership (GET /storage/devices).
    Devices,
    /// ZFS pools.
    #[command(subcommand)]
    Pools(PoolsCmd),
    /// Filesystems and volumes.
    #[command(subcommand)]
    Datasets(DatasetsCmd),
    /// Volumes only (GET /storage/volumes).
    Volumes {
        /// Only this pool.
        #[arg(long)]
        pool: Option<String>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Snapshots.
    #[command(subcommand)]
    Snapshots(SnapshotsCmd),
}

/// `storage pools` subcommands.
#[derive(Debug, Subcommand)]
pub enum PoolsCmd {
    /// List pools (GET /storage/pools).
    List {
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one pool with its vdev tree (GET /storage/pools/{id}).
    Get {
        /// Pool id.
        id: Id,
    },
    /// Create a pool (POST /storage/pools). Operator.
    Create {
        /// Pool name.
        name: String,
        /// A vdev as TYPE:DEV[,DEV...]; TYPE is stripe, mirror, raidz1,
        /// raidz2, raidz3, log, cache, or spare. Repeat for several.
        #[arg(long = "vdev", required = true)]
        vdevs: Vec<String>,
        /// Sector shift (9, 12, 13); default auto.
        #[arg(long)]
        ashift: Option<u32>,
        /// Root dataset compression; default lz4.
        #[arg(long)]
        compression: Option<String>,
        /// Overwrite disks that carry a foreign label.
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Change metadata (PATCH /storage/pools/{id}). Operator.
    Update {
        /// Pool id.
        id: Id,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Destroy a pool and everything on it (DELETE /storage/pools/{id}). Admin.
    Destroy {
        /// Pool id.
        id: Id,
        /// The pool name, echoed as a safeguard.
        #[arg(long)]
        name: String,
    },
    /// Start a scrub and print the job (POST /storage/pools/{id}/scrub). Operator.
    Scrub {
        /// Pool id.
        id: Id,
        /// Stop the running scrub instead.
        #[arg(long)]
        stop: bool,
    },
}

/// `storage datasets` subcommands.
#[derive(Debug, Subcommand)]
pub enum DatasetsCmd {
    /// List datasets (GET /storage/datasets).
    List {
        /// Only this pool.
        #[arg(long)]
        pool: Option<String>,
        /// Only children of this dataset.
        #[arg(long)]
        parent: Option<String>,
        /// filesystem or volume.
        #[arg(long, value_parser = ["filesystem", "volume"])]
        kind: Option<String>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one dataset (GET /storage/datasets/{id}).
    Get {
        /// Dataset id.
        id: Id,
    },
    /// Create a filesystem, or a volume with --size (POST /storage/datasets). Operator.
    Create(DatasetCreateArgs),
    /// Change properties or metadata (PATCH /storage/datasets/{id}). Operator.
    Update(DatasetUpdateArgs),
    /// Destroy a dataset (DELETE /storage/datasets/{id}). Operator.
    Destroy {
        /// Dataset id.
        id: Id,
        /// Also destroy children and snapshots.
        #[arg(long)]
        recursive: bool,
    },
}

/// Flags of `storage datasets create`.
#[derive(Debug, Args)]
pub struct DatasetCreateArgs {
    /// Full name, pool/path.
    pub name: String,
    /// Volume size (10G, 512M, or bytes); makes a volume.
    #[arg(long)]
    pub size: Option<String>,
    /// Thin-provision the volume.
    #[arg(long, requires = "size")]
    pub sparse: bool,
    /// Volume block size.
    #[arg(long, requires = "size")]
    pub volblocksize: Option<String>,
    /// Compression (lz4, zstd, gzip, off).
    #[arg(long)]
    pub compression: Option<String>,
    /// Quota.
    #[arg(long)]
    pub quota: Option<String>,
    /// Reservation.
    #[arg(long)]
    pub reservation: Option<String>,
    /// Mountpoint (filesystems).
    #[arg(long, conflicts_with = "size")]
    pub mountpoint: Option<String>,
    /// Record size (filesystems).
    #[arg(long, conflicts_with = "size")]
    pub recordsize: Option<String>,
    /// Do not update access times.
    #[arg(long, conflicts_with = "size")]
    pub no_atime: bool,
    /// Create missing parents.
    #[arg(long)]
    pub parents: bool,
    #[command(flatten)]
    pub metadata: MetadataArgs,
}

/// Flags of `storage datasets update`.
#[derive(Debug, Args)]
pub struct DatasetUpdateArgs {
    /// Dataset id.
    pub id: Id,
    /// New volume size; volumes only grow.
    #[arg(long)]
    pub size: Option<String>,
    /// Compression.
    #[arg(long)]
    pub compression: Option<String>,
    /// Quota; `none` removes it.
    #[arg(long)]
    pub quota: Option<String>,
    /// Reservation; `none` removes it.
    #[arg(long)]
    pub reservation: Option<String>,
    /// Mountpoint.
    #[arg(long)]
    pub mountpoint: Option<String>,
    /// Update access times: true or false.
    #[arg(long)]
    pub atime: Option<bool>,
    #[command(flatten)]
    pub metadata: MetadataArgs,
}

/// `storage snapshots` subcommands.
#[derive(Debug, Subcommand)]
pub enum SnapshotsCmd {
    /// List snapshots (GET /storage/snapshots).
    List {
        /// Only this dataset.
        #[arg(long)]
        dataset: Option<String>,
        /// Include descendants of --dataset.
        #[arg(long, requires = "dataset")]
        recursive: bool,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one snapshot (GET /storage/snapshots/{id}).
    Get {
        /// Snapshot id.
        id: Id,
    },
    /// Take a snapshot (POST /storage/snapshots). Operator.
    Create {
        /// Dataset to snapshot.
        dataset: String,
        /// The part after @.
        name: String,
        /// Snapshot every descendant too.
        #[arg(long)]
        recursive: bool,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Destroy a snapshot (DELETE /storage/snapshots/{id}). Operator.
    Destroy {
        /// Snapshot id.
        id: Id,
    },
    /// Roll the dataset back to a snapshot (POST /storage/snapshots/{id}/rollback). Operator.
    Rollback {
        /// Snapshot id.
        id: Id,
        /// Also destroy newer snapshots.
        #[arg(long)]
        discard_newer: bool,
    },
    /// Clone a snapshot into a new dataset (POST /storage/snapshots/{id}/clone). Operator.
    Clone {
        /// Snapshot id.
        id: Id,
        /// Full name of the new dataset.
        target: String,
    },
}

/// `network` subcommands.
#[derive(Debug, Subcommand)]
pub enum NetworkCmd {
    /// Every datalink.
    #[command(subcommand)]
    Links(LinksCmd),
    /// Link aggregations.
    #[command(subcommand)]
    Aggrs(AggrsCmd),
    /// VLANs.
    #[command(subcommand)]
    Vlans(VlansCmd),
    /// Etherstubs (virtual switches).
    #[command(subcommand)]
    Etherstubs(EtherstubsCmd),
    /// VNICs.
    #[command(subcommand)]
    Vnics(VnicsCmd),
    /// IP addresses.
    #[command(subcommand)]
    Addresses(AddressesCmd),
    /// Routes.
    #[command(subcommand)]
    Routes(RoutesCmd),
}

/// `network links` subcommands.
#[derive(Debug, Subcommand)]
pub enum LinksCmd {
    /// List links with what they sit over (GET /network/links).
    List,
    /// Show one link (GET /network/links/{id}).
    Get {
        /// Link id.
        id: Id,
    },
    /// Change MTU or metadata (PATCH /network/links/{id}). Operator.
    Update {
        /// Link id.
        id: Id,
        /// New MTU (576-9216).
        #[arg(long)]
        mtu: Option<u32>,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
}

/// `network aggrs` subcommands.
#[derive(Debug, Subcommand)]
pub enum AggrsCmd {
    /// Create an aggregation (POST /network/aggrs). Operator.
    Create {
        /// Link name, ending in a digit.
        name: String,
        /// A physical port; repeat for several.
        #[arg(long = "port", required = true)]
        ports: Vec<String>,
        /// L2, L3, L4, or a combination; default L4.
        #[arg(long)]
        policy: Option<String>,
        /// off, active, or passive; default active.
        #[arg(long, value_parser = ["off", "active", "passive"])]
        lacp: Option<String>,
        /// short or long; default short.
        #[arg(long, value_parser = ["short", "long"])]
        timer: Option<String>,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Delete an aggregation (DELETE /network/aggrs/{id}). Operator.
    Delete {
        /// Link id.
        id: Id,
    },
}

/// `network vlans` subcommands.
#[derive(Debug, Subcommand)]
pub enum VlansCmd {
    /// Create a VLAN (POST /network/vlans). Operator.
    Create {
        /// Link name, ending in a digit.
        name: String,
        /// VLAN id (1-4094).
        #[arg(long)]
        vid: u16,
        /// Physical link or aggregation beneath it.
        #[arg(long)]
        over: String,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Delete a VLAN (DELETE /network/vlans/{id}). Operator.
    Delete {
        /// Link id.
        id: Id,
    },
}

/// `network etherstubs` subcommands.
#[derive(Debug, Subcommand)]
pub enum EtherstubsCmd {
    /// Create an etherstub (POST /network/etherstubs). Operator.
    Create {
        /// Link name, ending in a digit.
        name: String,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Delete an etherstub (DELETE /network/etherstubs/{id}). Operator.
    Delete {
        /// Link id.
        id: Id,
    },
}

/// `network vnics` subcommands.
#[derive(Debug, Subcommand)]
pub enum VnicsCmd {
    /// Create a VNIC (POST /network/vnics). Operator.
    Create {
        /// Link name, ending in a digit.
        name: String,
        /// Physical link, aggregation, or etherstub beneath it.
        #[arg(long)]
        over: String,
        /// Pin a MAC address; default chosen by the system.
        #[arg(long)]
        mac: Option<String>,
        /// VLAN tag (1-4094).
        #[arg(long)]
        vid: Option<u16>,
        /// MTU (576-9216).
        #[arg(long)]
        mtu: Option<u32>,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Delete a VNIC (DELETE /network/vnics/{id}). Operator.
    Delete {
        /// Link id.
        id: Id,
    },
}

/// `network addresses` subcommands.
#[derive(Debug, Subcommand)]
pub enum AddressesCmd {
    /// List address objects (GET /network/addresses).
    List,
    /// Show one address (GET /network/addresses/{id}).
    Get {
        /// Address id.
        id: Id,
    },
    /// Add an address to a link (POST /network/addresses). Operator.
    Create {
        /// Link name; the IP interface is created when missing.
        interface: String,
        /// static, dhcp, or addrconf.
        #[arg(long, value_parser = ["static", "dhcp", "addrconf"])]
        kind: String,
        /// The address with prefix length (static).
        #[arg(long)]
        address: Option<String>,
        /// The part after / in the address object name; default v4 or v6.
        #[arg(long)]
        alias: Option<String>,
        /// Do not persist across reboot.
        #[arg(long)]
        temporary: bool,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Remove an address (DELETE /network/addresses/{id}). Operator.
    Delete {
        /// Address id.
        id: Id,
    },
}

/// `network routes` subcommands.
#[derive(Debug, Subcommand)]
pub enum RoutesCmd {
    /// List the routing table (GET /network/routes).
    List,
    /// Add a persistent static route (POST /network/routes). Operator.
    Create {
        /// default, or a network with prefix length.
        destination: String,
        /// Gateway address.
        gateway: String,
    },
    /// Remove a static route (DELETE /network/routes/{id}). Operator.
    Delete {
        /// Route id.
        id: Id,
    },
}
