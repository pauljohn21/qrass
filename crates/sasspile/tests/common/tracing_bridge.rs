//! Where a dev-time diagnostic view gets its subscriber — the one place that decides how a run is
//! observed, so no `src/` file ever names an OTel type (the pipeline only uses `tracing` macros).
//!
//! With `--features trace`, existing spans are bridged onto OpenTelemetry and printed by
//! `opentelemetry-stdout`: each span is written when it ends, carrying its `parent` id, so
//! `compile → phase → decl` nests without any collector or network hop. Without the feature, the same
//! code path installs a plain `fmt` subscriber. Either way `RUST_LOG` governs verbosity, and the default
//! stays at `info` so a normal harness run is no noisier than it was before.
//!
//! The OTel crates are `[dev-dependencies]`: they reach tests and examples only, never the library.
//! Cargo cannot make a dev-dependency conditional, so `trace` gates the *wiring*, not the compile — which
//! is stated plainly rather than pretending a feature strips the build.

/// The default when the environment says nothing: `info`, matching what the harness logged before any
/// tracing work, so turning on `trace` never silently buries the caller in DEBUG output.
fn filter() -> tracing_subscriber::filter::EnvFilter {
    tracing_subscriber::filter::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info"))
}

/// Install the subscriber, once. `try_init` keeps this safe when a test already installed one.
#[cfg(feature = "trace")]
pub fn init() {
    use opentelemetry::trace::TracerProvider as _;
    // All three are traits: `Layer` supplies `with_filter`, `SubscriberExt` the layering, and
    // `SubscriberInitExt` the `.try_init()` that keeps this safe when a subscriber already exists.
    use tracing_subscriber::{
        layer::{Layer as _, SubscriberExt as _},
        util::SubscriberInitExt as _,
    };
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .build();
    // The tracer holds the pipeline through an `Arc`, so the provider handle itself is not needed after
    // this; dropping it would shut the exporter down before the spans are written.
    let tracer = provider.tracer("sasspile");
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .try_init();
}

#[cfg(not(feature = "trace"))]
pub fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .try_init();
}
