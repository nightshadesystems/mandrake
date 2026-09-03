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
    /// Image catalogue and sources.
    #[command(subcommand)]
    Images(ImagesCmd),
    /// Native and lx zones.
    #[command(subcommand)]
    Zones(ZonesCmd),
    /// bhyve VMs.
    #[command(subcommand)]
    Vms(VmsCmd),
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

/// `images` subcommands.
#[derive(Debug, Subcommand)]
pub enum ImagesCmd {
    /// List imported images (GET /images).
    List {
        /// zone-native, zone-lx, vm-raw, or vm-iso.
        #[arg(long, value_parser = ["zone-native", "zone-lx", "vm-raw", "vm-iso"])]
        r#type: Option<String>,
        /// pending, downloading, verifying, importing, ready, or failed.
        #[arg(long, value_parser = ["pending", "downloading", "verifying", "importing", "ready", "failed"])]
        state: Option<String>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one image (GET /images/{id}).
    Get {
        /// Image id.
        id: Id,
    },
    /// What the sources offer (GET /images/available).
    Available {
        /// Only this source.
        #[arg(long)]
        source: Option<Id>,
        /// Only this type.
        #[arg(long, value_parser = ["zone-native", "zone-lx", "vm-raw", "vm-iso"])]
        r#type: Option<String>,
    },
    /// Import an image; prints the job (POST /images/import). Operator.
    Import {
        /// Name as the source lists it, or your own for a URL import.
        name: String,
        /// Version.
        version: String,
        /// Pick from this verified source's catalogue.
        #[arg(long, conflicts_with_all = ["url", "sha256"])]
        source: Option<Id>,
        /// Import directly from this URL (needs --sha256 and --type).
        #[arg(long, requires = "sha256", requires = "type")]
        url: Option<String>,
        /// Hex sha256 of the payload, which you vouch for.
        #[arg(long)]
        sha256: Option<String>,
        /// zone-native, zone-lx, vm-raw, or vm-iso (URL imports).
        #[arg(long, value_parser = ["zone-native", "zone-lx", "vm-raw", "vm-iso"])]
        r#type: Option<String>,
        /// Pool; default the data pool with the most free space.
        #[arg(long)]
        pool: Option<String>,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Change metadata (PATCH /images/{id}). Operator.
    Update {
        /// Image id.
        id: Id,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Delete an image (DELETE /images/{id}). Operator.
    Delete {
        /// Image id.
        id: Id,
    },
    /// Image sources.
    #[command(subcommand)]
    Sources(SourcesCmd),
}

/// `images sources` subcommands.
#[derive(Debug, Subcommand)]
pub enum SourcesCmd {
    /// List sources (GET /images/sources).
    List,
    /// Show one source (GET /images/sources/{id}).
    Get {
        /// Source id.
        id: Id,
    },
    /// Add a source and fetch its index (POST /images/sources). Operator.
    Add {
        /// A name.
        name: String,
        /// URL of index.json.
        url: String,
        /// Base64 Ed25519 public key; without it the source is unverified.
        #[arg(long)]
        public_key: Option<String>,
        /// Add it disabled.
        #[arg(long)]
        disabled: bool,
    },
    /// Change a source (PATCH /images/sources/{id}). Operator.
    Update {
        /// Source id.
        id: Id,
        /// New name (not for built-in sources).
        #[arg(long)]
        name: Option<String>,
        /// New URL (not for built-in sources).
        #[arg(long)]
        url: Option<String>,
        /// Set the public key.
        #[arg(long, conflicts_with = "no_key")]
        public_key: Option<String>,
        /// Remove the public key, making the source unverified.
        #[arg(long)]
        no_key: bool,
        /// Enable.
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        /// Disable.
        #[arg(long)]
        disable: bool,
    },
    /// Remove a source (DELETE /images/sources/{id}). Operator.
    Remove {
        /// Source id.
        id: Id,
    },
    /// Fetch the index now (POST /images/sources/{id}/refresh). Operator.
    Refresh {
        /// Source id.
        id: Id,
    },
}

/// `zones` subcommands.
#[derive(Debug, Subcommand)]
pub enum ZonesCmd {
    /// List zones (GET /zones).
    List {
        /// ipkg, lipkg, sparse, or lx.
        #[arg(long, value_parser = ["ipkg", "lipkg", "sparse", "lx"])]
        brand: Option<String>,
        /// A zoneadm state.
        #[arg(long)]
        state: Option<String>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one zone (GET /zones/{id}).
    Get {
        /// Zone id.
        id: Id,
    },
    /// Create and install a zone; prints the job (POST /zones). Operator.
    Create(ZoneCreateArgs),
    /// Change configuration or metadata (PATCH /zones/{id}). Operator.
    Update(ZoneUpdateArgs),
    /// Delete a zone; prints the job (DELETE /zones/{id}). Operator.
    Delete {
        /// Zone id.
        id: Id,
        /// Also destroy its datasets.
        #[arg(long)]
        purge: bool,
    },
    /// Boot a zone; prints the job (POST /zones/{id}/start). Operator.
    Start {
        /// Zone id.
        id: Id,
    },
    /// Shut a zone down; prints the job (POST /zones/{id}/stop). Operator.
    Stop {
        /// Zone id.
        id: Id,
        /// Halt instead of a clean shutdown.
        #[arg(long)]
        force: bool,
    },
    /// Reboot a zone; prints the job (POST /zones/{id}/restart). Operator.
    Restart {
        /// Zone id.
        id: Id,
    },
}

/// Flags of `zones create`.
#[derive(Debug, Args)]
pub struct ZoneCreateArgs {
    /// Zone name.
    pub name: String,
    /// ipkg, lipkg, sparse, or lx.
    #[arg(long, value_parser = ["ipkg", "lipkg", "sparse", "lx"])]
    pub brand: String,
    /// Image to clone; required for lx.
    #[arg(long)]
    pub image: Option<Id>,
    /// Pool for the zone dataset.
    #[arg(long)]
    pub pool: Option<String>,
    /// A NIC as NAME,OVER[,vid=N][,address=A/P][,gateway=G][,mac=M]. Repeatable.
    #[arg(long = "nic")]
    pub nics: Vec<String>,
    /// CPU cap, in CPUs (for example 1.5).
    #[arg(long)]
    pub cpu_cap: Option<f64>,
    /// Memory cap (2G, 512M, or bytes).
    #[arg(long)]
    pub memory: Option<String>,
    /// Do not boot with the host.
    #[arg(long)]
    pub no_autoboot: bool,
    /// Do not boot after install.
    #[arg(long)]
    pub no_start: bool,
    /// Hostname; default the zone name.
    #[arg(long)]
    pub hostname: Option<String>,
    /// A resolver; repeatable.
    #[arg(long = "resolver")]
    pub resolvers: Vec<String>,
    #[command(flatten)]
    pub metadata: MetadataArgs,
}

/// Flags of `zones update`.
#[derive(Debug, Args)]
pub struct ZoneUpdateArgs {
    /// Zone id.
    pub id: Id,
    /// Replace the NICs: NAME,OVER[,vid=N][,address=A/P][,gateway=G][,mac=M]. Repeatable.
    #[arg(long = "nic", conflicts_with = "clear_nics")]
    pub nics: Vec<String>,
    /// Remove every NIC.
    #[arg(long)]
    pub clear_nics: bool,
    /// CPU cap; `none` removes it.
    #[arg(long)]
    pub cpu_cap: Option<String>,
    /// Memory cap; `none` removes it.
    #[arg(long)]
    pub memory: Option<String>,
    /// Boot with the host: true or false.
    #[arg(long)]
    pub autoboot: Option<bool>,
    /// Hostname.
    #[arg(long)]
    pub hostname: Option<String>,
    /// Replace the resolvers; repeatable, or `none`.
    #[arg(long = "resolver")]
    pub resolvers: Vec<String>,
    #[command(flatten)]
    pub metadata: MetadataArgs,
}

// ------------------------------------------------------------ vms

/// `mandrakectl vms ...`
#[derive(Debug, Subcommand)]
pub enum VmsCmd {
    /// List VMs (GET /vms).
    List {
        /// A zoneadm state.
        #[arg(long)]
        state: Option<String>,
        #[command(flatten)]
        paging: Paging,
    },
    /// Show one VM (GET /vms/{id}).
    Get {
        /// VM id.
        id: Id,
    },
    /// Create a VM; prints the job (POST /vms). Operator.
    Create(VmCreateArgs),
    /// Change configuration or metadata (PATCH /vms/{id}). Operator.
    Update(VmUpdateArgs),
    /// Delete a VM; prints the job (DELETE /vms/{id}). Operator.
    Delete {
        /// VM id.
        id: Id,
        /// Also destroy its dataset and every disk.
        #[arg(long)]
        purge: bool,
    },
    /// Boot a VM; prints the job (POST /vms/{id}/start). Operator.
    Start {
        /// VM id.
        id: Id,
    },
    /// Shut a VM down; prints the job (POST /vms/{id}/stop). Operator.
    Stop {
        /// VM id.
        id: Id,
        /// Power off instead of an ACPI shutdown.
        #[arg(long)]
        force: bool,
    },
    /// Reboot a VM; prints the job (POST /vms/{id}/restart). Operator.
    Restart {
        /// VM id.
        id: Id,
    },
    /// Hard reset a VM; prints the job (POST /vms/{id}/reset). Operator.
    Reset {
        /// VM id.
        id: Id,
    },
    /// Disks.
    #[command(subcommand)]
    Disk(VmDiskCmd),
    /// ISOs.
    #[command(subcommand)]
    Cdrom(VmCdromCmd),
    /// Snapshots of every disk at once.
    #[command(subcommand)]
    Snapshot(VmSnapshotCmd),
}

/// `mandrakectl vms create`
// Each flag is an independent switch off a default; enums would only add
// ceremony to the command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args)]
pub struct VmCreateArgs {
    /// VM name.
    pub name: String,
    /// vCPUs (1-128).
    #[arg(long, default_value_t = 2)]
    pub vcpus: u32,
    /// Memory (2G, 512M, or bytes); at least 128M.
    #[arg(long, default_value = "2G")]
    pub memory: String,
    /// Firmware: uefi or uefi-csm.
    #[arg(long, value_parser = ["uefi", "uefi-csm"])]
    pub bootrom: Option<String>,
    /// Turn ACPI off.
    #[arg(long)]
    pub no_acpi: bool,
    /// Pool for the VM dataset; ignored when the boot disk is a clone.
    #[arg(long)]
    pub pool: Option<String>,
    /// Clone this vm-raw image as the boot disk.
    #[arg(
        long,
        conflicts_with = "boot_size",
        required_unless_present = "boot_size"
    )]
    pub image: Option<Id>,
    /// Blank boot disk of this size (20G); install from a --cdrom.
    #[arg(long)]
    pub boot_size: Option<String>,
    /// An extra blank disk of this size; repeatable.
    #[arg(long = "disk")]
    pub disks: Vec<String>,
    /// A vm-iso image to attach; repeatable.
    #[arg(long = "cdrom")]
    pub cdroms: Vec<Id>,
    /// A NIC as NAME,OVER[,vid=N][,address=A/P][,gateway=G][,mac=M]. Repeatable.
    #[arg(long = "nic")]
    pub nics: Vec<String>,
    /// No VNC display.
    #[arg(long)]
    pub no_vnc: bool,
    /// Do not boot with the host.
    #[arg(long)]
    pub no_autoboot: bool,
    /// Do not boot once created.
    #[arg(long)]
    pub no_start: bool,
    #[command(flatten)]
    pub metadata: MetadataArgs,
}

/// `mandrakectl vms update`
#[derive(Debug, Args)]
pub struct VmUpdateArgs {
    /// VM id.
    pub id: Id,
    /// vCPUs.
    #[arg(long)]
    pub vcpus: Option<u32>,
    /// Memory (4G, or bytes).
    #[arg(long)]
    pub memory: Option<String>,
    /// Firmware: uefi or uefi-csm.
    #[arg(long, value_parser = ["uefi", "uefi-csm"])]
    pub bootrom: Option<String>,
    /// ACPI: true or false.
    #[arg(long)]
    pub acpi: Option<bool>,
    /// VNC display: true or false.
    #[arg(long)]
    pub vnc: Option<bool>,
    /// Boot with the host: true or false.
    #[arg(long)]
    pub autoboot: Option<bool>,
    /// Replace the NICs: NAME,OVER[,vid=N][,address=A/P][,gateway=G][,mac=M]. Repeatable.
    #[arg(long = "nic", conflicts_with = "clear_nics")]
    pub nics: Vec<String>,
    /// Remove every NIC.
    #[arg(long)]
    pub clear_nics: bool,
    #[command(flatten)]
    pub metadata: MetadataArgs,
}

/// `mandrakectl vms disk ...`
#[derive(Debug, Subcommand)]
pub enum VmDiskCmd {
    /// Add a disk (POST /vms/{id}/disks). Operator.
    Add {
        /// VM id.
        id: Id,
        /// Blank disk of this size (50G).
        #[arg(long, conflicts_with = "image", required_unless_present = "image")]
        size: Option<String>,
        /// Clone this vm-raw image instead.
        #[arg(long)]
        image: Option<Id>,
    },
    /// Grow a disk (PATCH /vms/{id}/disks/{index}). Operator.
    Resize {
        /// VM id.
        id: Id,
        /// Disk slot.
        index: u32,
        /// New size; larger than now.
        #[arg(long)]
        size: String,
    },
    /// Detach a disk (DELETE /vms/{id}/disks/{index}). Operator.
    Remove {
        /// VM id.
        id: Id,
        /// Disk slot; not the boot disk.
        index: u32,
        /// Also destroy the volume.
        #[arg(long)]
        purge: bool,
    },
}

/// `mandrakectl vms cdrom ...`
#[derive(Debug, Subcommand)]
pub enum VmCdromCmd {
    /// Attach an ISO (POST /vms/{id}/cdroms). Operator.
    Attach {
        /// VM id.
        id: Id,
        /// A ready vm-iso image.
        #[arg(long)]
        image: Id,
    },
    /// Eject an ISO (DELETE /vms/{id}/cdroms/{index}). Operator.
    Detach {
        /// VM id.
        id: Id,
        /// Cdrom slot.
        index: u32,
    },
}

/// `mandrakectl vms snapshot ...`
#[derive(Debug, Subcommand)]
pub enum VmSnapshotCmd {
    /// List snapshots (GET /vms/{id}/snapshots).
    List {
        /// VM id.
        id: Id,
    },
    /// Take a snapshot (POST /vms/{id}/snapshots). Operator.
    Create {
        /// VM id.
        id: Id,
        /// Snapshot name.
        name: String,
        #[command(flatten)]
        metadata: MetadataArgs,
    },
    /// Delete a snapshot (DELETE /vms/{id}/snapshots/{name}). Operator.
    Delete {
        /// VM id.
        id: Id,
        /// Snapshot name.
        name: String,
    },
    /// Roll every disk back; the VM must be stopped (POST .../rollback). Operator.
    Rollback {
        /// VM id.
        id: Id,
        /// Snapshot name.
        name: String,
    },
}
