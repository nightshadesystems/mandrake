//! `mandraked`: the single daemon that owns Mandrake host management.
//!
//! Runs as `svc:/system/mandrake/mandraked:default`, serves the HTTP+JSON
//! API defined in `api/openapi.yaml` over HTTPS and over a root-only Unix
//! socket, and embeds the web console. The library exposes the router so
//! integration tests drive it in process; `main.rs` is the thin binary.

#![allow(
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]

pub mod app;
pub mod audit;
pub mod auth;
pub mod cache;
pub mod config;
pub mod console;
pub mod cursor;
pub mod db;
pub mod drivers;
pub mod error;
pub mod events;
pub mod host;
pub mod idempotency;
pub mod images;
pub mod jobs;
pub mod metadata;
pub mod routes;
pub mod serve;
#[cfg(unix)]
pub mod socket;
pub mod tls;
pub mod vms;
pub mod zone_console;
pub mod zones;
