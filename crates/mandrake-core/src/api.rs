//! Wire types mirroring `api/openapi.yaml`.
//!
//! The daemon serialises these and the CLI deserialises them. Field names
//! and optionality follow the contract; nullable fields serialise as
//! `null`, optional non-nullable fields are omitted when absent.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Actor, Id, Role, Timestamp};

/// Cursor-paginated envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// This page's items.
    pub items: Vec<T>,
    /// Cursor for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

/// `POST /auth/login` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    /// Username.
    pub username: String,
    /// Password.
    pub password: String,
}

/// The current actor and, for cookie sessions, when it expires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Who is acting.
    pub actor: Actor,
    /// Absolute expiry of the session, if a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// Idle expiry of the session, if a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_expires_at: Option<Timestamp>,
}

/// A local user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Id.
    pub id: Id,
    /// Login name.
    pub username: String,
    /// Role.
    pub role: Role,
    /// Display name, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Whether the account is disabled.
    pub disabled: bool,
    /// Set while locked out after failed logins.
    pub locked_until: Option<Timestamp>,
    /// Last successful login.
    pub last_login_at: Option<Timestamp>,
    /// Created.
    pub created_at: Timestamp,
    /// Last changed.
    pub updated_at: Timestamp,
}

/// `POST /users` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCreate {
    /// Login name.
    pub username: String,
    /// Initial password.
    pub password: String,
    /// Role.
    pub role: Role,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// `PATCH /users/{id}` body; omitted fields are unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserUpdate {
    /// New role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// New display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Enable or disable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// `PUT /users/{id}/password` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordChange {
    /// Required when changing one's own password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_password: Option<String>,
    /// The new password.
    pub new_password: String,
}

/// A bearer token's metadata; the secret is never included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Id.
    pub id: Id,
    /// Owner.
    pub user_id: Id,
    /// Name given at creation.
    pub name: String,
    /// First eight characters of the secret after `mdk_`.
    pub prefix: String,
    /// Created.
    pub created_at: Timestamp,
    /// Expiry, if any.
    pub expires_at: Option<Timestamp>,
    /// Last use, to the minute.
    pub last_used_at: Option<Timestamp>,
}

/// `POST /tokens` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCreate {
    /// Name.
    pub name: String,
    /// Owner other than the caller; admin only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Id>,
    /// Lifetime in seconds; omit for no expiry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<i64>,
}

/// `POST /tokens` response: the metadata plus the one-time secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCreated {
    /// Metadata.
    #[serde(flatten)]
    pub token: Token,
    /// The full bearer token, shown once.
    pub secret: String,
}

/// The object an audit entry or event is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectRef {
    /// Resource family, singular: `user`, `token`, `system`, ...
    pub kind: String,
    /// Object id, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
    /// Human name at the time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ObjectRef {
    /// A reference to an object with id and name.
    pub fn new(kind: &str, id: Id, name: impl Into<String>) -> Self {
        Self {
            kind: kind.to_owned(),
            id: Some(id),
            name: Some(name.into()),
        }
    }

    /// A reference to a singleton such as `system`.
    pub fn singleton(kind: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            id: None,
            name: None,
        }
    }
}

/// Outcome of an audited call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditResult {
    /// Succeeded.
    Ok,
    /// Refused by authorisation or policy.
    Denied,
    /// Attempted and failed.
    Failed,
}

impl AuditResult {
    /// Lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Denied => "denied",
            Self::Failed => "failed",
        }
    }
}

/// One audit log row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Monotonic id; usable as a cursor.
    pub id: String,
    /// When.
    pub at: Timestamp,
    /// Who.
    pub actor: Actor,
    /// `<kind>.<verb>`.
    pub action: String,
    /// What.
    pub object: ObjectRef,
    /// Summary before the call.
    pub before: Option<Value>,
    /// Summary after the call.
    pub after: Option<Value>,
    /// Outcome.
    pub result: AuditResult,
    /// Free text, for example the reason for a denial.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Request id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Client address, or `socket`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Job lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    /// Waiting to run.
    Queued,
    /// Running.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Finished with an error.
    Failed,
    /// Cancelled before finishing.
    Cancelled,
}

/// A long-running operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Id.
    pub id: Id,
    /// State.
    pub state: JobState,
    /// Operation family, for example `image.import`.
    pub kind: String,
    /// What it acts on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ObjectRef>,
    /// Progress from 0 to 1, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Latest human-readable status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Created.
    pub created_at: Timestamp,
    /// Started.
    pub started_at: Option<Timestamp>,
    /// Finished.
    pub finished_at: Option<Timestamp>,
    /// Error, when failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<crate::Problem>,
}

/// One frame on the event stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Monotonic id; pass as `since` to resume.
    pub id: String,
    /// When.
    pub at: Timestamp,
    /// `<kind>.<verb>`.
    pub kind: String,
    /// What.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<ObjectRef>,
    /// Who.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    /// Kind-specific payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Host identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Host id, generated once.
    pub id: Id,
    /// Hostname.
    pub hostname: String,
    /// Always `mandrake`.
    pub product: String,
    /// Mandrake version.
    pub version: String,
    /// OmniOS release, for example `r151054`.
    pub omnios_release: String,
    /// Active boot environment.
    pub boot_environment: String,
    /// Seconds since boot.
    pub uptime_seconds: u64,
    /// Current time on the host.
    pub time: Timestamp,
    /// Host timezone name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Memory figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    /// Physical memory.
    pub total_bytes: u64,
    /// Free memory.
    pub free_bytes: u64,
}

/// CPU, memory, and load at one instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemResources {
    /// Online CPUs.
    pub cpus: u32,
    /// 1, 5, and 15 minute load averages.
    pub load_avg: [f64; 3],
    /// Memory.
    pub memory: Memory,
    /// When sampled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampled_at: Option<Timestamp>,
}

/// Per-object metadata held in SQLite, not in illumos (ADR-0002).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Metadata {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.description.is_none()
            && self.tags.is_none()
            && self.notes.is_none()
    }
}
