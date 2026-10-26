//! Ledger CLI thin shell: tracing subscriber setup + argument plumbing; all
//! logic lives in the `ledger` library (shared with the hrx runner).
//!
//! Under `--features trace` the run also gets an OpenTelemetry view (see [`trace_setup`]). The root span
//! is not decoration: `tracing-opentelemetry` exports **spans**, and this shell used to create none, so a
//! "traced" CLI run would have exported an empty trace. One span per invocation, carrying which command
//! ran against which database, is the minimum a reading of "what did this call do" needs.

use rusqlite::Connection;
use std::path::Path;
use tracing::info;

fn main() {
    trace_setup();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = args
        .iter()
        .find(|a| a.starts_with("--db="))
        .map_or(ledger::DB, |a| a.trim_start_matches("--db="));
    let argv: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--db="))
        .cloned()
        .collect();
    let command = argv.first().map(String::as_str).unwrap_or("<none>");
    let span = tracing::info_span!("ledger", command, db = %db);
    let _guard = span.enter();
    match Connection::open(Path::new(db)).and_then(|conn| ledger::run(&conn, &argv)) {
        Ok(()) => {}
        Err(e) => info!(?e, "ledger error"),
    }
}

/// The filter the environment asks for, defaulting to `info` so a normal run prints exactly what it
/// printed before any tracing work.
fn filter() -> tracing_subscriber::filter::EnvFilter {
    tracing_subscriber::filter::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info"))
}

/// Install the subscriber. With the `trace` feature the spans of this run are additionally bridged to
/// OpenTelemetry and printed by `opentelemetry-stdout`, which needs no collector and no network.
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
    let tracer = provider.tracer("ledger");
    // The tracer owns the exporter pipeline through an `Arc`; dropping the provider handle here would
    // shut it down before the root span is ever exported.
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_filter(filter()),
        )
        .try_init();
}

#[cfg(not(feature = "trace"))]
fn trace_setup() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_target(false)
        .try_init();
}
