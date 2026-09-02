//! Opaque pagination cursors.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;

/// Largest page a client may ask for.
pub const MAX_LIMIT: u32 = 500;
/// Page size when the client does not say.
pub const DEFAULT_LIMIT: u32 = 100;

/// `cursor` and `limit` query parameters.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pagination {
    /// Opaque cursor from a previous page.
    pub cursor: Option<String>,
    /// Requested page size.
    pub limit: Option<u32>,
}

impl Pagination {
    /// The page size to use, clamped to `1..=MAX_LIMIT`.
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// The decoded cursor, if one was given and it decodes.
    pub fn after(&self) -> Option<String> {
        self.cursor.as_deref().and_then(decode)
    }
}

/// Encode a position as an opaque cursor.
pub fn encode(position: &str) -> String {
    URL_SAFE_NO_PAD.encode(position.as_bytes())
}

/// Decode a cursor back to its position; `None` if it is not one of ours.
pub fn decode(cursor: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    String::from_utf8(bytes).ok()
}

/// Trim a fetched `limit + 1` list to `limit` items and produce the next
/// cursor from the last kept item's position.
pub fn page<T>(
    mut items: Vec<T>,
    limit: u32,
    position: impl Fn(&T) -> String,
) -> (Vec<T>, Option<String>) {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if items.len() > limit {
        items.truncate(limit);
        let next = items.last().map(|last| encode(&position(last)));
        (items, next)
    } else {
        (items, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips() {
        let c = encode("alice");
        assert_eq!(decode(&c).as_deref(), Some("alice"));
        assert_eq!(decode("%%%"), None);
    }

    #[test]
    fn limit_is_clamped() {
        assert_eq!(Pagination::default().limit(), DEFAULT_LIMIT);
        assert_eq!(
            Pagination {
                cursor: None,
                limit: Some(0)
            }
            .limit(),
            1
        );
        assert_eq!(
            Pagination {
                cursor: None,
                limit: Some(9999)
            }
            .limit(),
            MAX_LIMIT
        );
    }

    #[test]
    fn paging_trims_and_points_at_the_last_kept_item() {
        let (items, next) = page(vec![1, 2, 3], 2, ToString::to_string);
        assert_eq!(items, vec![1, 2]);
        assert_eq!(next.as_deref().and_then(decode).as_deref(), Some("2"));
        let (items, next) = page(vec![1, 2], 2, ToString::to_string);
        assert_eq!(items, vec![1, 2]);
        assert!(next.is_none());
    }
}
