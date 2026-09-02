//! Who is acting on the API.

use serde::{Deserialize, Serialize};

use crate::{Id, Role};

/// How an actor authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    /// Console session cookie.
    Session,
    /// Bearer token.
    Token,
    /// Root over the Unix socket.
    Socket,
}

/// The authenticated principal behind a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    /// User id; absent for root over the socket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
    /// Username, or `root`.
    pub username: String,
    /// Effective role.
    pub role: Role,
    /// Authentication path.
    pub via: Via,
    /// Token used, when `via` is `token`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<Id>,
}

impl Actor {
    /// The synthetic root actor for uid 0 on the Unix socket (ADR-0007).
    pub fn root() -> Self {
        Self {
            id: None,
            username: "root".to_owned(),
            role: Role::Admin,
            via: Via::Socket,
            token_id: None,
        }
    }

    /// Whether this actor is the given user.
    pub fn is_user(&self, id: Id) -> bool {
        self.id == Some(id)
    }
}
