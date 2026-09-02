//! Roles (ADR-0007).

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A user's role. Declared in ascending order of privilege so that
/// derived ordering gives `Viewer < Operator < Admin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Reads everything except other users' tokens.
    Viewer,
    /// Also performs every infrastructure mutation.
    Operator,
    /// Also manages users, tokens, and the system.
    Admin,
}

impl Role {
    /// Whether a holder of this role may do what `required` allows.
    pub fn allows(self, required: Role) -> bool {
        self >= required
    }

    /// Lowercase name as used on the wire and in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Admin => "admin",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error for an unknown role name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown role `{0}`; expected admin, operator, or viewer")]
pub struct UnknownRole(pub String);

impl FromStr for Role {
    type Err = UnknownRole;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "viewer" => Ok(Self::Viewer),
            "operator" => Ok(Self::Operator),
            "admin" => Ok(Self::Admin),
            other => Err(UnknownRole(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_is_ordered() {
        assert!(Role::Admin.allows(Role::Viewer));
        assert!(Role::Admin.allows(Role::Admin));
        assert!(Role::Operator.allows(Role::Viewer));
        assert!(!Role::Operator.allows(Role::Admin));
        assert!(!Role::Viewer.allows(Role::Operator));
    }

    #[test]
    fn names_round_trip() {
        for role in [Role::Viewer, Role::Operator, Role::Admin] {
            assert_eq!(role.as_str().parse::<Role>().ok(), Some(role));
        }
        assert!("root".parse::<Role>().is_err());
    }
}
