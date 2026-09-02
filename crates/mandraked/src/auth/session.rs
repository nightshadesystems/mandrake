//! Console sessions (ADR-0007).

use mandrake_core::{Id, Timestamp};
use rusqlite::{Connection, OptionalExtension, params};

use super::token;

/// Cookie name.
pub const COOKIE: &str = "mandrake_session";
/// Seconds of inactivity before a session ends.
pub const IDLE_SECS: i64 = 12 * 3600;
/// Seconds after login when a session ends regardless.
pub const ABSOLUTE_SECS: i64 = 7 * 24 * 3600;
/// How often `last_seen_at` is written back.
const TOUCH_SECS: i64 = 60;

/// A live session row.
#[derive(Debug, Clone)]
pub struct SessionRow {
    /// SHA-256 of the cookie value.
    pub hash: String,
    /// Owner.
    pub user_id: Id,
    /// Absolute expiry.
    pub expires_at: Timestamp,
    /// Idle expiry.
    pub idle_expires_at: Timestamp,
}

/// Create a session for `user_id`; returns the cookie value and the row.
pub fn create(
    conn: &Connection,
    user_id: Id,
    source: Option<&str>,
) -> rusqlite::Result<(String, SessionRow)> {
    let bytes: [u8; 32] = rand::random();
    let secret: String = {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    };
    let now = Timestamp::now();
    let row = SessionRow {
        hash: token::hash(&secret),
        user_id,
        expires_at: now.plus_seconds(ABSOLUTE_SECS),
        idle_expires_at: now.plus_seconds(IDLE_SECS),
    };
    conn.execute(
        "INSERT INTO sessions (hash, user_id, created_at, last_seen_at, expires_at, idle_expires_at, source) \
         VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6)",
        params![
            row.hash,
            user_id.to_string(),
            now.to_rfc3339(),
            row.expires_at.to_rfc3339(),
            row.idle_expires_at.to_rfc3339(),
            source,
        ],
    )?;
    Ok((secret, row))
}

/// Look up a session by cookie value, refresh its idle expiry, and return
/// it if still valid. Expired rows are deleted on sight.
pub fn touch(conn: &Connection, secret: &str) -> rusqlite::Result<Option<SessionRow>> {
    let hash = token::hash(secret);
    let row = conn
        .query_row(
            "SELECT hash, user_id, expires_at, idle_expires_at, last_seen_at FROM sessions WHERE hash = ?1",
            [&hash],
            |r| {
                Ok((
                    SessionRow {
                        hash: r.get("hash")?,
                        user_id: crate::db::get_id(r, "user_id")?,
                        expires_at: crate::db::get_ts(r, "expires_at")?,
                        idle_expires_at: crate::db::get_ts(r, "idle_expires_at")?,
                    },
                    crate::db::get_ts(r, "last_seen_at")?,
                ))
            },
        )
        .optional()?;
    let Some((mut row, last_seen)) = row else {
        return Ok(None);
    };
    let now = Timestamp::now();
    if now >= row.expires_at || now >= row.idle_expires_at {
        conn.execute("DELETE FROM sessions WHERE hash = ?1", [&hash])?;
        return Ok(None);
    }
    if now.seconds_since(last_seen) >= TOUCH_SECS {
        row.idle_expires_at = now.plus_seconds(IDLE_SECS).min(row.expires_at);
        conn.execute(
            "UPDATE sessions SET last_seen_at = ?1, idle_expires_at = ?2 WHERE hash = ?3",
            params![now.to_rfc3339(), row.idle_expires_at.to_rfc3339(), hash],
        )?;
    }
    Ok(Some(row))
}

/// Delete one session by hash.
pub fn delete(conn: &Connection, hash: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sessions WHERE hash = ?1", [hash])?;
    Ok(())
}

/// Delete every session of a user, optionally keeping one.
pub fn delete_for_user(
    conn: &Connection,
    user_id: Id,
    keep: Option<&str>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM sessions WHERE user_id = ?1 AND hash <> COALESCE(?2, '')",
        params![user_id.to_string(), keep],
    )
}

/// Delete expired sessions and stale idempotency records.
pub fn sweep(conn: &Connection) -> rusqlite::Result<usize> {
    let now = Timestamp::now();
    let sessions = conn.execute(
        "DELETE FROM sessions WHERE expires_at <= ?1 OR idle_expires_at <= ?1",
        [now.to_rfc3339()],
    )?;
    let idem = conn.execute(
        "DELETE FROM idempotency WHERE created_at <= ?1",
        [now.plus_seconds(-24 * 3600).to_rfc3339()],
    )?;
    Ok(sessions + idem)
}

/// `Set-Cookie` value establishing a session.
pub fn set_cookie(secret: &str) -> String {
    format!("{COOKIE}={secret}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={ABSOLUTE_SECS}")
}

/// `Set-Cookie` value clearing the session cookie.
pub fn clear_cookie() -> String {
    format!("{COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
}

/// Pull the session cookie out of a `Cookie` header value.
pub fn from_cookie_header(header: &str) -> Option<&str> {
    header.split(';').map(str::trim).find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == COOKIE).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_cookie_header() {
        assert_eq!(
            from_cookie_header("a=b; mandrake_session=xyz; c=d"),
            Some("xyz")
        );
        assert_eq!(from_cookie_header("a=b"), None);
    }
}
