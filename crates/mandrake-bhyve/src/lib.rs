//! bhyve VMs as OmniOS bhyve-brand zones (ADR-0013).
//!
//! A VM is a zone; its lifecycle runs through the zones driver. This crate
//! is the mapping in both directions: a [`VmSpec`] to the [`ZoneSpec`] the
//! bhyve brand wants (`vcpus`, `ram`, `bootrom`, `acpi`, `vnc`, `bootdisk`
//! and `disk` attributes with matching device resources, `cdrom`
//! attributes with read-only lofs mounts, `anet` NICs), and a zone's
//! configuration back to a [`VmConfig`]. Pure functions; the attribute
//! spellings are confirmed against a VM built by hand on OmniOS r151054.

#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

pub mod render;
pub mod types;

pub use mandrake_zones::ZoneSpec;
pub use render::{VNC_SOCKET, from_zone_config, to_zone_spec, vnc_socket_path, zvol_device};
pub use types::*;

/// The zone brand VMs use.
pub const BRAND: &str = "bhyve";
