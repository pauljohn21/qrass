//! `sasslint` CLI: scan a directory (default `crates/sasspile/src`) for iron-rule
//! violations; report each via `tracing`; exit non-zero if any are found (a CI gate).

use std::path::PathBuf;
use std::process::exit;

use sasslint::scan_dir;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/sasspile/src"));

    let span = tracing::info_span!("sasslint", path = %root.display());
    let _guard = span.enter();

    let findings = scan_dir(&root);
    findings.iter().for_each(|f| {
        tracing::warn!(rule = f.rule, file = %f.file, line = f.line, text = %f.text, "violation")
    });
    tracing::info!(count = findings.len(), "sasslint complete");

    if !findings.is_empty() {
        exit(1);
    }
}
