//! Password hashing with argon2id (ADR-0007).

use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};

/// Shortest password accepted.
pub const MIN_LEN: usize = 12;

/// Longest password accepted; also bounds hashing work per request.
pub const MAX_LEN: usize = 1024;

const MEMORY_KIB: u32 = 19 * 1024;
const ITERATIONS: u32 = 2;
const LANES: u32 = 1;

/// Errors from hashing.
#[derive(Debug, thiserror::Error)]
#[error("password hashing failed: {0}")]
pub struct HashError(String);

fn hasher() -> Result<Argon2<'static>, HashError> {
    let params =
        Params::new(MEMORY_KIB, ITERATIONS, LANES, None).map_err(|e| HashError(e.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Check the policy on a candidate password.
pub fn check_policy(password: &str) -> Result<(), &'static str> {
    if password.chars().count() < MIN_LEN {
        return Err("password must be at least 12 characters");
    }
    if password.len() > MAX_LEN {
        return Err("password is too long");
    }
    Ok(())
}

/// Hash a password into a PHC string. The salt is drawn from the OS.
pub fn hash(password: &str) -> Result<String, HashError> {
    let hash = hasher()?
        .hash_password(password.as_bytes())
        .map_err(|e| HashError(e.to_string()))?;
    Ok(hash.to_string())
}

/// Verify a password against a PHC string, whose own parameters are used.
/// Any malformed input is false.
pub fn verify(password: &str, phc: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), phc)
        .is_ok()
}

/// Spend about as long as a real verification would, so a login for a
/// missing user takes as long as one for a wrong password.
pub fn burn_time(password: &str) {
    // A fixed hash of an unrelated password; the result is discarded.
    const DUMMY: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$\
                         5uQvXk3Ff8Zk0y9Yg1wVQpXK4tqzcy7Xo2r6aM6oL5c";
    let _ = verify(password, DUMMY);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_verify_and_reject() {
        let phc = hash("correct horse battery").unwrap_or_default();
        assert!(phc.starts_with("$argon2id$"), "{phc}");
        assert!(phc.contains("m=19456,t=2,p=1"), "{phc}");
        assert!(verify("correct horse battery", &phc));
        assert!(!verify("wrong horse battery", &phc));
        assert!(!verify("anything", "not a hash"));
    }

    #[test]
    fn policy() {
        assert!(check_policy("short").is_err());
        assert!(check_policy("twelve chars").is_ok());
    }
}
