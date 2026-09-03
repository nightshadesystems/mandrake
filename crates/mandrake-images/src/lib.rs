//! Image catalogue: signed source indexes, fetch, sha256 verification, and
//! ZFS import (ADR-0012).
//!
//! Sources are Ed25519-signed JSON indexes ([`index`]). Payloads are
//! streamed to a staging file and hashed on the way ([`transport`]), then
//! received into a dataset, written to a zvol, or kept as a file
//! ([`store`]). [`Importer`] runs those steps for one image and reports
//! progress; the daemon wraps it in a job and owns the catalogue rows.
//!
//! Everything that touches the network or ZFS sits behind a trait with an
//! in-memory fake, so the daemon's routes are testable anywhere.

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod import;
pub mod index;
pub mod store;
pub mod transport;
pub mod types;

pub use import::{ImportPlan, Importer, Outcome, Progress};
pub use index::{Index, IndexEntry};
pub use mandrake_core::shell::BoxFuture;
pub use store::{FakeStore, Store, ZfsStore};
pub use transport::{Downloaded, FakeTransport, HttpTransport, Transport};
pub use types::*;

/// The dataset or zvol an image lives in (ADR-0012).
pub fn dataset_for(pool: &str, id: mandrake_core::Id) -> String {
    format!("{pool}/images/{id}")
}

/// The file an ISO image is kept as.
pub fn iso_path_for(pool: &str, id: mandrake_core::Id) -> String {
    format!("/{pool}/images/iso/{id}.iso")
}

/// The snapshot clones are taken from.
pub const IMAGE_SNAPSHOT: &str = "image";

/// Cap on documents fetched whole (indexes and signatures).
pub const MAX_DOCUMENT_BYTES: usize = 8 << 20;
