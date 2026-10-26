//! The gate's `trace` feature must produce a real trace.
//!
//! Pinned together with the plain reading on purpose: the CLI is a CI gate, and a diagnostic path that
//! swallowed `count=…` (or exited differently) would turn "we looked" into "we passed" — the failure this
//! project has already been bitten by once (`cli_directory.rs`).

#![cfg(feature = "trace")]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn a_traced_lint_exports_the_scan_span_and_keeps_its_report() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let traced = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", source.to_str().expect("utf-8 path")])
        .output()
        .expect("run sasslint");
    let stdout = String::from_utf8_lossy(&traced.stdout);

    assert!(
        stdout.contains("Span #"),
        "`--features trace` must export the run as a span: {stdout}"
    );
    assert!(
        stdout.contains("path"),
        "the span must say which directory was scanned, or the trace proves nothing: {stdout}"
    );
    assert!(
        stdout.contains("sasslint complete"),
        "the gate's own reading must still be printed: {stdout}"
    );
    assert!(
        traced.status.success(),
        "`crates/sasspile/src` is expected clean; a traced run must not change that verdict"
    );
}
