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
mod phases;
mod value;

use crate::phases::{Exec, Lexer, Render, Structurize};

/// Compiler error carried as an in-stream value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassError {
    pub message: String,
}

impl SassError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
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
///
/// Each phase is a framework-shaped operator kernel (`crate::phases`): lazy `struct{source}` +
/// state inside its `Observer` + the three impls. A kernel emits every item it produces straight
/// into `observer.next(..)`, so nothing is batched into a `Vec` and flattened again.
///
/// Flattening, not recursion: `Exec` keeps the program `State` (open blocks, environment) inside
/// its own observer and moves it forward by `&mut self`, emitting only the rule an event settles —
/// so no whole-state clone rides the stream and no `Arc<Env>` snapshot is smuggled alongside a
/// declaration; a declaration is still evaluated at the position it appears.
///
/// Order is a property of the **stream**, not of a key: up to `Render` the chain is serial, so the
/// rules reach [`Render`] in document order and their CSS comes out in that order — no `collect`,
/// no `sort_by_key`, no per-rule task hand-off. Real concurrency is reserved for independent module
/// subgraphs (P5), where the constraint is that nothing may be produced before its inner observable
/// is subscribed (`merge_all` drops what a queued inner emits while it waits). CSS text appears
/// only at the single terminal.
pub async fn compile(content: impl Into<String>) -> Result<String, SassError> {
    let _span = tracing::info_span!("compile").entered();
    Shared::from_iter(content.into().chars().collect::<Vec<char>>())
        .transform(|source| Lexer { source })
        .transform(|source| Structurize { source })
        .transform(|source| Exec { source })
        .transform(|source| Render { source })
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
