//! sasspile — a clean-room SCSS→CSS compiler architected as a **PUSH (rxrust) stream**: a
//! source pushes owned values *forward* through operators (`from_iter → scan_map → flat_map →
//! filter → collect → into_future`), and is **never** pulled/walked as a materialized AST.
//!
//! This crate was reset to a from-scratch skeleton after adopting the push doctrine
//! (`dart-sass = pull / rxrust = push`). The old pull-shaped code (parse→AST→recursive walk)
//! has been deleted to prevent backsliding. The pipeline is rebuilt phase by phase (P0–P8)
//! per `openspec/changes/redesign-sasspile-rxrust-shared-stream/design.md`. Governed by
//! `.qoder/rules/rust.md`; behaviour oracle = `sass-spec/*.hrx` + `element-plus` only.

use std::fmt;

use rxrust::prelude::*;

mod env;
mod exec;
mod expr;
mod lexer;
mod value;

use crate::exec::{Fold, StyleRule};
use crate::lexer::{LexAcc, MsgAcc};

/// Compiler error carried as an in-stream value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassError {
    pub message: String,
}

impl SassError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for SassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SassError {}

/// The public entry — and the **only** external API. One cold **push** chain, written here and
/// nowhere else: no stage function takes or returns an observable type, no `box_it()` seam.
/// Characters flow forward through the framework's own operators, each accumulator a plain
/// owned value, and CSS text is produced only at the single `into_future().await` terminal.
pub async fn compile(content: impl Into<String>) -> Result<String, SassError> {
    let _span = tracing::info_span!("compile").entered();
    Shared::from_iter(content.into().chars().collect::<Vec<char>>())
        .scan_map(LexAcc::default(), |acc, ch| acc.feed(ch))
        .flat_map(Shared::from_iter)
        .scan_map(MsgAcc::default(), |acc, tok| acc.feed(tok))
        .flat_map(Shared::from_iter)
        .scan_map(Fold::default(), |fold, ev| fold.step(ev))
        .filter_map(std::convert::identity)
        .collect::<Vec<StyleRule>>()
        .map(|mut rules| {
            rules.sort_by_key(|rule| rule.order);
            rules
        })
        .flat_map(Shared::from_iter)
        .map(|rule| rule.to_css())
        .reduce_initial(String::new(), |mut css, chunk| {
            css.push_str(&chunk);
            css
        })
        .into_future()
        .await
        .or_else(|e| match e {
            rxrust::IntoFutureError::Empty => Ok(Ok(String::new())),
            other => Err(other),
        })
        .map_err(|_| SassError::new("no output"))?
        .map_err(|_| SassError::new("reduce"))
}
