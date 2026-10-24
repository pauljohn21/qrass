//! sasspile — a clean-room SCSS→CSS compiler written **from rxrust outward**: the
//! program *is* one owned-value stream (chars → tokens → parse-fold → render), and
//! the public entry is the single `subscribe` at its edge. No intermediate
//! "call a function, get a `Vec` back" relays, no bespoke stage/error framework —
//! the rxrust operators themselves are the architecture. Behaviour is pinned only
//! by the `sass-spec/*.hrx` goldens and the element-plus corpus.
//!
//! Current stage: a first vertical slice — flat rulesets `sel { prop: value; … }`
//! compiled to expanded CSS (e.g. the `css/plain` golden `& {a: b}` → `& {\n  a: b;\n}`).
//! Nesting, `@use`/`@include`, values/operations and builtins grow on top of this
//! same stream, each new stage another operator on the chain.

mod ast;
mod error;
mod lexer;
mod parse;
mod render;

pub use error::{ErrorKind, SassError};

use std::sync::mpsc;

use rxrust::prelude::*;

use crate::lexer::token_stream;
use crate::parse::Acc;

/// Compile SCSS/CSS text to expanded CSS. The whole compiler is the observable
/// chain below; this function is the one place a stream meets ordinary Rust (an
/// `mpsc` sink fed by `subscribe`, non-timed sources running fully inline).
pub fn compile_string(content: impl Into<String>) -> Result<String, SassError> {
    let (tx, rx) = mpsc::channel::<Result<String, SassError>>();
    token_stream(content.into())
        .reduce_initial(Acc::default(), |acc, tok| acc.step(tok))
        .map(Acc::finish)
        .flat_map(|program| match program {
            Ok(program) => Local::from_iter(program.rules)
                .reduce_initial(String::new(), |mut css, rule| {
                    css.push_str(&render::rule_css(&rule));
                    css
                })
                .map(Ok::<_, SassError>)
                .box_it(),
            Err(err) => Local::of(Err::<String, SassError>(err)).box_it(),
        })
        .subscribe(move |out| {
            tx.send(out).ok();
        });
    rx.recv()
        .unwrap_or_else(|_| Err(SassError::new(ErrorKind::Internal, "no output")))
}
