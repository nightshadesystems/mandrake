//! RFC 7807 problem details.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Prefix for application-specific problem types.
pub const PROBLEM_BASE: &str = "https://mandrake.nightshade.systems/problems/";

/// An error as the API reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// `about:blank` for plain HTTP errors, or a [`PROBLEM_BASE`] URI.
    #[serde(rename = "type")]
    pub type_: String,
    /// Short human-readable summary.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Explanation specific to this occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// URI of the specific occurrence, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Request id, matching the daemon log and the audit row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl Problem {
    /// A plain HTTP problem with type `about:blank`.
    pub fn new(status: u16, title: impl Into<String>) -> Self {
        Self {
            type_: "about:blank".to_owned(),
            title: title.into(),
            status,
            detail: None,
            instance: None,
            request_id: None,
        }
    }

    /// An application problem typed by `slug` under [`PROBLEM_BASE`].
    pub fn typed(status: u16, slug: &str, title: impl Into<String>) -> Self {
        Self {
            type_: format!("{PROBLEM_BASE}{slug}"),
            ..Self::new(status, title)
        }
    }

    /// Attach a detail message.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Attach the request id.
    #[must_use]
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }

    /// The slug of a typed problem, if it is one.
    pub fn slug(&self) -> Option<&str> {
        self.type_.strip_prefix(PROBLEM_BASE)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.status, self.title)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Problem {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_problems_carry_a_slug() {
        let p = Problem::typed(423, "locked", "Locked").with_detail("try later");
        assert_eq!(p.slug(), Some("locked"));
        assert_eq!(p.to_string(), "423 Locked: try later");
        let json = serde_json::to_value(&p).unwrap_or_default();
        assert_eq!(json["type"], format!("{PROBLEM_BASE}locked"));
        assert!(json.get("instance").is_none());
    }

    #[test]
    fn plain_problems_are_about_blank() {
        assert_eq!(Problem::new(404, "Not Found").slug(), None);
    }
}
