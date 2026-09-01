//! Driver for illumos zones via `zonecfg` and `zoneadm`.
//!
//! Shells out to native tooling and parses parsable (`-p`) output; see
//! ADR-0003. Exposes typed operations, not a reconcile loop.
//!
//! Populated in Phase 4.
