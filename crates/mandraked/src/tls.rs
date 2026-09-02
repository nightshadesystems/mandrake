//! TLS material: a self-signed certificate generated on first start.

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

/// Certificate and key as PEM, plus the certificate's SHA-256 fingerprint.
#[derive(Debug, Clone)]
pub struct Material {
    /// Certificate chain, PEM.
    pub cert_pem: Vec<u8>,
    /// Private key, PEM.
    pub key_pem: Vec<u8>,
    /// `AA:BB:...` SHA-256 over the DER certificate.
    pub fingerprint: String,
    /// Whether this call generated the material.
    pub generated: bool,
}

/// Errors loading or generating TLS material.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// Filesystem.
    #[error("{path}: {source}")]
    Io {
        /// Path involved.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },
    /// Certificate generation.
    #[error("generating certificate: {0}")]
    Generate(String),
    /// The stored certificate is not PEM we can read.
    #[error("{0} is not a PEM certificate")]
    BadPem(String),
}

fn io(path: &Path, source: std::io::Error) -> TlsError {
    TlsError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Load `cert.pem` and `key.pem` from `dir`, generating both if either is
/// missing. New files are created mode 0600 on Unix.
pub fn load_or_generate(dir: &Path, hostname: &str) -> Result<Material, TlsError> {
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    if cert_path.is_file() && key_path.is_file() {
        let cert_pem = std::fs::read(&cert_path).map_err(|e| io(&cert_path, e))?;
        let key_pem = std::fs::read(&key_path).map_err(|e| io(&key_path, e))?;
        let der = pem_to_der(&cert_pem)
            .ok_or_else(|| TlsError::BadPem(cert_path.display().to_string()))?;
        return Ok(Material {
            fingerprint: fingerprint(&der),
            cert_pem,
            key_pem,
            generated: false,
        });
    }

    let mut names = vec![hostname.to_owned(), "localhost".to_owned()];
    names.dedup();
    let generated =
        rcgen::generate_simple_self_signed(names).map_err(|e| TlsError::Generate(e.to_string()))?;
    let cert_pem = generated.cert.pem().into_bytes();
    let key_pem = generated.signing_key.serialize_pem().into_bytes();
    let fp = fingerprint(generated.cert.der());

    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    write_private(&key_path, &key_pem)?;
    write_private(&cert_path, &cert_pem)?;
    Ok(Material {
        cert_pem,
        key_pem,
        fingerprint: fp,
        generated: true,
    })
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), TlsError> {
    std::fs::write(path, bytes).map_err(|e| io(path, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| io(path, e))?;
    }
    Ok(())
}

/// `AA:BB:...` SHA-256 of DER bytes.
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The first certificate block of a PEM file as DER.
pub fn pem_to_der(pem: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(pem).ok()?;
    let mut inside = false;
    let mut b64 = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "-----BEGIN CERTIFICATE-----" {
            inside = true;
        } else if line == "-----END CERTIFICATE-----" {
            break;
        } else if inside {
            b64.push_str(line);
        }
    }
    if b64.is_empty() {
        return None;
    }
    STANDARD.decode(b64).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use super::*;

    #[test]
    fn generates_then_reloads_the_same_certificate() {
        let Ok(dir) = tempfile::tempdir() else {
            return;
        };
        let first = load_or_generate(dir.path(), "test.local").ok();
        let second = load_or_generate(dir.path(), "test.local").ok();
        let (Some(a), Some(b)) = (first, second) else {
            panic!("generation failed");
        };
        assert!(a.generated);
        assert!(!b.generated);
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.fingerprint.len(), 32 * 3 - 1);
    }
}
