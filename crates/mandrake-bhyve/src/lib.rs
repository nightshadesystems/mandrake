//! Driver for bhyve-brand zones: VM lifecycle and serial/VNC console proxying.
//!
//! Renders the VM resource model (spec §7) to OmniOS bhyve brand attributes.
//! Where SmartOS behaviour differs it is noted in comments; OmniOS conventions
//! win.
//!
//! Populated in Phase 5.
