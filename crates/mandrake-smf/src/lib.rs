//! SMF manifests and service control.
//!
//! Holds the manifest bundle for `svc:/system/mandrake/mandraked:default`
//! and its `setup` companion, plus the method script, embedded so the
//! packaging step and tests read one copy. Service control helpers
//! (`svcs`, `svcadm`) arrive with the first phase that needs them.

#![allow(clippy::must_use_candidate)]

/// The SMF service bundle for `system/mandrake/setup` and
/// `system/mandrake/mandraked`.
pub const MANIFEST: &str = include_str!("../manifests/mandraked.xml");

/// The method script both services run.
pub const METHOD: &str = include_str!("../manifests/svc-mandraked");

/// FMRI of the daemon's default instance.
pub const DAEMON_FMRI: &str = "svc:/system/mandrake/mandraked:default";

/// FMRI of the one-shot setup service.
pub const SETUP_FMRI: &str = "svc:/system/mandrake/setup:default";

/// The `config/*` property names the method script reads.
pub const CONFIG_PROPERTIES: [&str; 5] = ["listen", "socket", "db", "tls_dir", "log"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_declares_both_services_and_every_config_property() {
        assert!(MANIFEST.contains("name=\"system/mandrake/setup\""));
        assert!(MANIFEST.contains("name=\"system/mandrake/mandraked\""));
        assert!(MANIFEST.contains("privileges=\"basic,net_privaddr\""));
        for prop in CONFIG_PROPERTIES {
            assert!(
                MANIFEST.contains(&format!("propval name=\"{prop}\"")),
                "{prop}"
            );
            assert!(
                METHOD.contains(&format!("prop {prop}")),
                "method reads {prop}"
            );
        }
    }

    #[test]
    fn method_handles_both_verbs() {
        assert!(METHOD.contains("setup)"));
        assert!(METHOD.contains("start)"));
        assert!(METHOD.contains("exec \"$DAEMON\""));
    }
}
