//! One SCSS string, one run, with the diagnostic subscriber installed — the small entry point for
//! reading a trace, because the harness measures 11,533 cases and nobody can read that as a diagnosis.
//!
//! ```text
//! cargo run -p sasspile --features trace --example trace_pipeline -- 'a {b: calc(calc(1px + 1%))}'
//! RUST_LOG=debug cargo run -p sasspile --features trace --example trace_pipeline
//! ```
//!
//! Without `--features trace` this still compiles and runs: the result comes out as a `tracing` event
//! only, no OpenTelemetry involved. The result is reported through `tracing` rather than `println!` for
//! the same reason the library never prints — one output channel, so `RUST_LOG` governs what a run shows.

#[path = "../tests/common/tracing_bridge.rs"]
mod tracing_bridge;

use sasspile::compile;

/// The default sample is a shape this slice decided by measurement rather than guess: an unsimplifiable
/// inner `calc` inside an outer one, so the output shows the one-dropped shell.
const DEFAULT_SCSS: &str = "a {b: calc(calc(1px + 1%))}";

#[tokio::main]
async fn main() {
    tracing_bridge::init();
    let scss = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_SCSS.into());
    match compile(scss).await {
        Ok(css) => tracing::info!(result = %css.trim(), "compiled"),
        Err(error) => {
            tracing::error!(kind = ?error.kind, message = %error.message, "compile failed")
        }
    }
}
