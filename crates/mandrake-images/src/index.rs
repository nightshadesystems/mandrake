//! The signed source index (ADR-0012): `index.json` and a detached
//! `index.json.sig` holding a base64 Ed25519 signature over the exact
//! bytes of the index.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use mandrake_core::{Timestamp, image::ImageType};
use serde::{Deserialize, Serialize};

use crate::types::{ImageError, Result, valid_sha256};

/// One image a source offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Type.
    #[serde(rename = "type")]
    pub type_: ImageType,
    /// Payload URL, absolute or relative to the index.
    pub url: String,
    /// Hex sha256 of the payload.
    pub sha256: String,
    /// Payload size.
    pub size: u64,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// OS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
}

/// The index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// Source name as the publisher spells it.
    pub name: String,
    /// When it was built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<Timestamp>,
    /// Entries.
    pub images: Vec<IndexEntry>,
}

/// Parse an index and check every entry is well formed.
pub fn parse(bytes: &[u8]) -> Result<Index> {
    let index: Index = serde_json::from_slice(bytes)
        .map_err(|e| ImageError::Index(format!("not an index: {e}")))?;
    for e in &index.images {
        if e.name.is_empty() || e.version.is_empty() || e.url.is_empty() {
            return Err(ImageError::Index(format!(
                "entry {}@{} is missing a name, version, or url",
                e.name, e.version
            )));
        }
        if !valid_sha256(&e.sha256) {
            return Err(ImageError::Index(format!(
                "entry {}@{} has a bad sha256",
                e.name, e.version
            )));
        }
    }
    Ok(index)
}

fn key_bytes<const N: usize>(b64: &str, what: &str) -> Result<[u8; N]> {
    let raw = BASE64
        .decode(b64.trim())
        .map_err(|e| ImageError::Index(format!("{what} is not base64: {e}")))?;
    raw.try_into()
        .map_err(|_| ImageError::Index(format!("{what} must be {N} bytes")))
}

/// Check `sig_b64` over `bytes` with `public_key_b64`.
pub fn verify(bytes: &[u8], sig_b64: &str, public_key_b64: &str) -> Result<()> {
    let key = VerifyingKey::from_bytes(&key_bytes::<32>(public_key_b64, "public key")?)
        .map_err(|e| ImageError::Index(format!("bad public key: {e}")))?;
    let sig = Signature::from_bytes(&key_bytes::<64>(sig_b64, "signature")?);
    key.verify(bytes, &sig)
        .map_err(|_| ImageError::Index("signature does not match".to_owned()))
}

/// Sign `bytes` with a base64 32-byte secret key; the signature, base64.
pub fn sign(bytes: &[u8], secret_key_b64: &str) -> Result<String> {
    let key = SigningKey::from_bytes(&key_bytes::<32>(secret_key_b64, "secret key")?);
    Ok(BASE64.encode(key.sign(bytes).to_bytes()))
}

/// A fresh keypair as `(secret, public)`, both base64.
pub fn keypair() -> (String, String) {
    let secret: [u8; 32] = rand::random();
    let key = SigningKey::from_bytes(&secret);
    (
        BASE64.encode(secret),
        BASE64.encode(key.verifying_key().to_bytes()),
    )
}

/// The public half of a base64 secret key.
pub fn public_key_of(secret_key_b64: &str) -> Result<String> {
    let key = SigningKey::from_bytes(&key_bytes::<32>(secret_key_b64, "secret key")?);
    Ok(BASE64.encode(key.verifying_key().to_bytes()))
}

/// Whether `b64` is a plausible public key.
pub fn valid_public_key(b64: &str) -> bool {
    key_bytes::<32>(b64, "public key")
        .ok()
        .and_then(|b| VerifyingKey::from_bytes(&b).ok())
        .is_some()
}

/// Resolve an entry URL against the index URL: absolute URLs stay,
/// relative ones join the index's directory.
pub fn resolve_url(index_url: &str, entry_url: &str) -> String {
    if entry_url.contains("://") {
        return entry_url.to_owned();
    }
    let base = index_url.rfind('/').map_or(index_url, |i| &index_url[..=i]);
    if let Some(rest) = entry_url.strip_prefix('/') {
        // Host-relative: keep scheme and host.
        let scheme_end = index_url.find("://").map_or(0, |i| i + 3);
        let host_end = index_url[scheme_end..]
            .find('/')
            .map_or(index_url.len(), |i| scheme_end + i);
        return format!("{}/{rest}", &index_url[..host_end]);
    }
    format!("{base}{entry_url}")
}

/// The signature URL beside an index.
pub fn signature_url(index_url: &str) -> String {
    format!("{index_url}.sig")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const SAMPLE: &str = include_str!("../testdata/index.sample.json");

    #[test]
    fn parses_and_resolves() {
        let index = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(index.name, "nightshade.systems");
        assert_eq!(index.images.len(), 3);
        assert_eq!(index.images[0].type_, ImageType::ZoneLx);
        assert_eq!(
            resolve_url(
                "https://images.example/mandrake/index.json",
                &index.images[0].url
            ),
            "https://images.example/mandrake/debian-12-20260901.zfs.gz"
        );
        assert_eq!(
            resolve_url("https://images.example/mandrake/index.json", "/pub/x.iso"),
            "https://images.example/pub/x.iso"
        );
        assert_eq!(
            resolve_url("https://a/index.json", "https://b/y.raw.xz"),
            "https://b/y.raw.xz"
        );
        assert_eq!(
            signature_url("https://a/b/index.json"),
            "https://a/b/index.json.sig"
        );
    }

    #[test]
    fn rejects_bad_entries() {
        assert!(parse(b"{}").is_err());
        let bad = r#"{"name":"x","images":[{"name":"a","version":"1","type":"vm-iso","url":"a.iso","sha256":"zz","size":1}]}"#;
        assert!(matches!(parse(bad.as_bytes()), Err(ImageError::Index(_))));
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let (secret, public) = keypair();
        assert!(valid_public_key(&public));
        assert_eq!(public_key_of(&secret).unwrap(), public);
        let sig = sign(SAMPLE.as_bytes(), &secret).unwrap();
        verify(SAMPLE.as_bytes(), &sig, &public).unwrap();
        let mut tampered = SAMPLE.as_bytes().to_vec();
        tampered.push(b' ');
        assert!(verify(&tampered, &sig, &public).is_err());
        let (_, other) = keypair();
        assert!(verify(SAMPLE.as_bytes(), &sig, &other).is_err());
        assert!(verify(SAMPLE.as_bytes(), "nope", &public).is_err());
        assert!(!valid_public_key("AAAA"));
    }
}
