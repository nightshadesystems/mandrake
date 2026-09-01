//! Image catalogue: sources, fetch, sha256 verification, and ZFS import.
//!
//! Sources are Ed25519-signed JSON indexes. Imported images become datasets
//! or zvols under `<pool>/images`; VM and zone creation clones them.
//!
//! Populated in Phase 4.
