//! Shared types, identifiers, and error definitions used across Mandrake.
//!
//! Everything in this crate is illumos-agnostic and compiles on any host so
//! that the daemon, CLI, and driver crates can be unit-tested anywhere. The
//! wire types in [`api`] mirror `api/openapi.yaml`, which stays the contract.

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod actor;
pub mod api;
pub mod id;
pub mod problem;
pub mod role;
pub mod shell;
pub mod storage;
pub mod timestamp;

pub use actor::{Actor, Via};
pub use id::Id;
pub use problem::Problem;
pub use role::Role;
pub use timestamp::Timestamp;
