//! RFC 3339 timestamps in UTC.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

/// A point in time, always UTC, serialised as RFC 3339.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(#[serde(with = "time::serde::rfc3339")] OffsetDateTime);

impl Timestamp {
    /// The current time.
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    /// Build from seconds since the Unix epoch.
    pub fn from_unix(secs: i64) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp(secs).ok().map(Self)
    }

    /// This time plus `secs` seconds (negative moves back).
    #[must_use]
    pub fn plus_seconds(self, secs: i64) -> Self {
        Self(self.0 + Duration::seconds(secs))
    }

    /// Seconds from `earlier` to this time; negative if this is earlier.
    pub fn seconds_since(self, earlier: Self) -> i64 {
        (self.0 - earlier.0).whole_seconds()
    }

    /// The RFC 3339 text form, for example `2026-09-01T12:00:00Z`.
    pub fn to_rfc3339(self) -> String {
        self.0.format(&Rfc3339).unwrap_or_default()
    }

    /// The underlying `time` value.
    pub const fn inner(self) -> OffsetDateTime {
        self.0
    }
}

impl From<OffsetDateTime> for Timestamp {
    fn from(t: OffsetDateTime) -> Self {
        Self(t.to_offset(time::UtcOffset::UTC))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl FromStr for Timestamp {
    type Err = time::error::Parse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        OffsetDateTime::parse(s, &Rfc3339).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_as_rfc3339_utc() {
        let t = Timestamp::from_unix(1_756_728_000).unwrap_or_else(Timestamp::now);
        assert_eq!(t.to_rfc3339(), "2025-09-01T12:00:00Z");
        assert_eq!(t.to_rfc3339().parse::<Timestamp>().ok(), Some(t));
    }

    #[test]
    fn arithmetic() {
        let t = Timestamp::from_unix(1_000).unwrap_or_else(Timestamp::now);
        let later = t.plus_seconds(90);
        assert_eq!(later.seconds_since(t), 90);
        assert!(later > t);
    }
}
