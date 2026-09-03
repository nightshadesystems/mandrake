//! Errors and small value types of the images crate.

use std::fmt::Write as _;

use mandrake_core::shell::ShellError;

pub use mandrake_core::shell::FailureKind;

/// Crate result.
pub type Result<T> = std::result::Result<T, ImageError>;

/// Why an image operation failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ImageError {
    /// The network did not deliver.
    #[error("fetch failed: {0}")]
    Transport(String),
    /// The index or its signature is not right.
    #[error("index rejected: {0}")]
    Index(String),
    /// The payload hash did not match.
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    Verify {
        /// Published.
        expected: String,
        /// Computed.
        actual: String,
    },
    /// A tool failed.
    #[error(transparent)]
    Command(#[from] ShellError),
    /// Local file trouble.
    #[error("{0}")]
    Io(String),
    /// The request cannot be honoured as given.
    #[error("{0}")]
    Invalid(String),
}

impl ImageError {
    /// Classify for HTTP mapping.
    pub fn kind(&self) -> FailureKind {
        match self {
            Self::Transport(_) | Self::Io(_) => FailureKind::Other,
            Self::Index(_) | Self::Verify { .. } | Self::Invalid(_) => FailureKind::Invalid,
            Self::Command(e) => {
                let text = e.stderr().to_ascii_lowercase();
                if text.contains("does not exist") {
                    FailureKind::NotFound
                } else if text.contains("exists") {
                    FailureKind::Exists
                } else if text.contains("busy") || text.contains("dependent clones") {
                    FailureKind::Conflict
                } else if text.contains("permission denied") {
                    FailureKind::Forbidden
                } else {
                    FailureKind::Other
                }
            }
        }
    }
}

/// How a payload is compressed, from its URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Plain.
    None,
    /// gzip.
    Gzip,
    /// xz.
    Xz,
}

impl Compression {
    /// From the file name at the end of a URL.
    pub fn from_url(url: &str) -> Self {
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let ext = std::path::Path::new(path)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        match ext.as_deref() {
            Some("gz" | "tgz") => Self::Gzip,
            Some("xz") => Self::Xz,
            _ => Self::None,
        }
    }

    /// The decompressor command, if any.
    pub fn decompressor(self) -> Option<mandrake_core::shell::Command> {
        match self {
            Self::None => None,
            Self::Gzip => Some(mandrake_core::shell::Command::new("gzip").arg("-dc")),
            Self::Xz => Some(mandrake_core::shell::Command::new("xz").arg("-dc")),
        }
    }
}

/// Lowercase hex, 64 characters.
pub fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Lowercase hex of a digest.
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
