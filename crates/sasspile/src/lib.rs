//! sasspile — a clean-room SCSS→CSS compiler written **from rxrust outward**: the
//! program *is* one owned-value stream. The core (`engine`) folds a flat `Vec<Msg>`
//! through an owned work-stack (`std::iter::from_fn`) — no AST tree, no recursion,
//! values built from structured tokens to CSS via `Expr`/`Value`. Behaviour is pinned
//! only by the `sass-spec/*.hrx` goldens and the element-plus corpus.

mod error;
pub mod engine;
pub mod expr;
mod lexer;
pub mod value;

pub use error::{ErrorKind, SassError};
pub use expr::{Env, Expr, Op, parse_expr, value_to_css};
pub use value::{Sep, Value};

/// Compile SCSS/CSS text to expanded CSS via the flat-stream engine.
pub fn compile_string(content: impl Into<String>) -> Result<String, SassError> {
    let _span = tracing::info_span!("compile").entered();
    engine::compile_engine(content)
}

/// Initialise a `tracing` → OpenTelemetry → stdout pipeline so a whole compile can
/// be inspected as a span tree (lex/parse/eval/render + per-decl values). Available
/// only with the `trace` feature; never compiled for the wasm target.
#[cfg(feature = "trace")]
pub fn init_trace() {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::TracerProvider;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let exporter = opentelemetry_stdout::SpanExporter::default();
    let provider = TracerProvider::builder()
        .with_simple_exporter(exporter)
        .build();
    let tracer = provider.tracer("sasspile");
    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::EnvFilter::try_from_default_env().unwrap_or_else(|_| "debug".into()))
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(tracing_subscriber::fmt::layer())
        .init();
}
