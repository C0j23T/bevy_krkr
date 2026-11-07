//! Workspace-level build script for `bevy_krkr`.
//!
//! Binding generation itself runs in `crates/krkr_sys/build.rs`, because the
//! generated files have to land in that crate's `OUT_DIR`. This script only
//! reports where the Kirikiroid2 checkout is expected, once, at the top level.
//!
//! Real bindings (feature `real_ffi`, default) use `$KIRIKIROID2_SRC` or
//! `Kirikiroid2/` next to this file. See README.md.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=KIRIKIROID2_SRC");
    if std::env::var_os("CARGO_FEATURE_REAL_FFI").is_none() {
        return;
    }
    let src = std::env::var_os("KIRIKIROID2_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Kirikiroid2"));
    if !src.is_dir() {
        println!(
            "cargo:warning=bevy_krkr: no Kirikiroid2 checkout at {} (see README.md, \"Kirikiroid2 source\")",
            src.display()
        );
    }
}
