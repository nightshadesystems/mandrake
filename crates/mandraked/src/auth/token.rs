//! Bearer token secrets (ADR-0007).
//!
//! `mdk_` + 32 random bytes as base64url. Only the SHA-256 of the whole
//! secret is stored, plus its first eight characters for display.

use std::fmt::Write as _;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Secret prefix identifying a Mandrake token.
pub const PREFIX: &str = "mdk_";

/// Characters of the secret (after the prefix) kept for display.
pub const DISPLAY_LEN: usize = 8;

/// A freshly generated secret and what to store for it.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The full secret, shown to the caller once.
    pub secret: String,
    /// Display prefix.
    pub prefix: String,
    /// Hex SHA-256 of the secret.
    pub hash: String,
}

/// Generate a new secret.
pub fn generate() -> Generated {
    let bytes: [u8; 32] = rand::random();
    let secret = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
    let prefix = secret[PREFIX.len()..PREFIX.len() + DISPLAY_LEN].to_owned();
    let hash = hash(&secret);
    Generated {
        secret,
        prefix,
        hash,
    }
}

/// Lowercase hex of some bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Hex SHA-256 of a secret (token or session id).
pub fn hash(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

/// Whether a presented bearer credential has the shape of a token.
pub fn looks_like_token(s: &str) -> bool {
    s.starts_with(PREFIX) && s.len() > PREFIX.len() + DISPLAY_LEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_distinct_and_hashable() {
        let a = generate();
        let b = generate();
        assert_ne!(a.secret, b.secret);
        assert_eq!(a.prefix.len(), DISPLAY_LEN);
        assert_eq!(a.hash.len(), 64);
        assert_eq!(hash(&a.secret), a.hash);
        assert!(looks_like_token(&a.secret));
        assert!(!looks_like_token("mdk_short"));
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }
}
