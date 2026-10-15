//! Shared helpers for the conformance harness (element-plus golden + sass-spec
//! hrx runner). These are not `#[test]` themselves (no `#[test]` fns here); they
//! are `mod`-included by `tests/element_plus_e2e.rs`, `tests/hrx_parse.rs` and
//! `tests/hrx_spec.rs`. Each item is `pub`, but a given test binary uses only a
//! subset, so `dead_code` is allowed on the modules to keep the harness warning
//! clean (task 12.1 runs `cargo clippy -- -D warnings`).

#![allow(dead_code)]

pub mod css_diff;
pub mod css_validate;
pub mod hrx;

use std::path::PathBuf;

/// Absolute path to the element-plus `index.scss` entry (repo root is the parent
/// of the crate manifest dir).
pub fn ep_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../element-plus/packages/theme-chalk/src/index.scss")
}

/// Absolute path to a golden CSS file under `tests/golden/`.
pub fn golden(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(rel)
}

/// Absolute path to the checked-out `sass-spec` submodule root.
pub fn sass_spec() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sass-spec")
}

/// Absolute path to the `sass-spec/spec` behavior-case tree.
pub fn sass_spec_dir() -> PathBuf {
    sass_spec().join("spec")
}
