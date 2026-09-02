//! Make sure the console asset directory exists so `rust-embed` can compile
//! even when the console has not been built; the daemon then serves a
//! "console not built" page instead of failing to compile.

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let dist = PathBuf::from(manifest).join("../../console/dist");
    // Best effort: a failure here surfaces as the rust-embed error instead.
    let _ = fs::create_dir_all(&dist);
    println!("cargo:rerun-if-changed={}", dist.display());
}
