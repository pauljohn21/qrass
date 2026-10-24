//! sasspile — a clean-room SCSS→CSS compiler written **from rxrust outward**: the
//! program *is* one owned-value stream (chars → tokens → parse-fold → render), and
//! the public entry is the single `subscribe` at its edge. No intermediate
//! "call a function, get a `Vec` back" relays, no bespoke stage/error framework —
//! the rxrust operators themselves are the architecture. Behaviour is pinned only
//! by the `sass-spec/*.hrx` goldens and the element-plus corpus.
//!
//! Current stage: rulesets with **nesting** and **variables** — `$x: …` binds and is
//! never emitted, `$x` references resolve (document-order), selectors parent-combine
//! (descendant + `&`), all to expanded CSS. `@use`/`@include`, value operators and
//! builtins grow on top of this same stream, each new stage another operator.

mod ast;
mod error;
mod eval;
pub mod expr;
mod lexer;
mod parse;
mod render;
pub mod value;

pub use error::{ErrorKind, SassError};
pub use expr::{Env, Expr, Op, parse_expr, value_to_css};
pub use value::{Sep, Value};

use std::sync::mpsc;

use rxrust::prelude::*;

use crate::lexer::token_stream;
use crate::parse::Acc;

/// Compile SCSS/CSS text to expanded CSS. The whole compiler is the observable
/// chain below; this function is the one place a stream meets ordinary Rust (an
/// `mpsc` sink fed by `subscribe`, non-timed sources running fully inline).
pub fn compile_string(content: impl Into<String>) -> Result<String, SassError> {
    let span = tracing::info_span!("compile");
    let _guard = span.enter();
    let (tx, rx) = mpsc::channel::<Result<String, SassError>>();
    token_stream(content.into())
        .reduce_initial(Acc::default(), |acc, tok| acc.step(tok))
        .map(Acc::finish)
        .flat_map(|program| match program {
            Ok(program) => {
                let mut env = eval::Env::from_globals(&program.globals);
                let rules = env.resolve_all(program.rules);
                Local::from_iter(rules)
                    .reduce_initial(String::new(), |mut css, rule| {
                        render::write_rule(&mut css, &rule);
                        css
                    })
                    .map(Ok::<_, SassError>)
                    .box_it()
            }
            Err(err) => Local::of(Err::<String, SassError>(err)).box_it(),
        })
        .subscribe(move |out| {
            tx.send(out).ok();
        });
    rx.recv()
        .unwrap_or_else(|_| Err(SassError::new(ErrorKind::Internal, "no output")))
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
