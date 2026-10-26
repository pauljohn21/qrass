//! `sasslint` CLI: scan a directory (default `crates/sasspile/src`) for iron-rule
//! violations; report each via `tracing`; exit non-zero if any are found (a CI gate).
//!
//! The documented invocation is `sasslint lint <dir>` (`rust.md`), and the first version of this shell
//! read `argv[1]` as the **path** — so `sasslint lint crates/sasspile/src` searched for a directory named
//! `lint`, found nothing, and reported `count=0` forever: a gate that always passes is worse than no
//! gate, because it looks like a clean bill of health while the `gate_sasspile_is_clean` test on the same
//! code was red. The verb is therefore consumed, and a positional path is still accepted.

use std::path::PathBuf;
use std::process::exit;

use sasslint::scan_dir;

fn main() {
    trace_setup();

    let arguments = std::env::args().skip(1).collect::<Vec<String>>();
    let root = arguments
        .iter()
        .find(|argument| argument.as_str() != "lint")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/sasspile/src"));

    let span = tracing::info_span!("sasslint", path = %root.display());
    let _guard = span.enter();

    if !root.exists() {
        tracing::error!(path = %root.display(), "no such path — nothing was linted");
        exit(2);
    }

    let findings = scan_dir(&root);
    findings.iter().for_each(|f| {
        tracing::warn!(rule = f.rule, file = %f.file, line = f.line, text = %f.text, "violation")
    });
    tracing::info!(count = findings.len(), "sasslint complete");

    if !findings.is_empty() {
        exit(1);
    }
}

/// The filter the environment asks for, `info` by default so the gate's output does not change shape.
fn filter() -> tracing_subscriber::filter::EnvFilter {
    tracing_subscriber::filter::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info"))
}

/// Install the subscriber. Under `--features trace` the `sasslint` span (which carries the scanned path)
/// is also bridged to OpenTelemetry and printed by `opentelemetry-stdout`, so "how long did the gate take
/// and over what" is readable without timing the process from outside.
#[cfg(feature = "trace")]
fn trace_setup() {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{
        layer::{Layer as _, SubscriberExt as _},
        util::SubscriberInitExt as _,
    };
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .build();
    let tracer = provider.tracer("sasslint");
    // The tracer holds the pipeline through an `Arc`; dropping the provider would close the exporter
    // before the span is written.
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .try_init();
}

#[cfg(not(feature = "trace"))]
fn trace_setup() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .try_init();
}
