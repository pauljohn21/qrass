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

use crate::exec::State;
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
///
/// Flattening, not recursion: the structural pass moves the program `State` forward with
/// `scan` — the framework's verb whose emitted item **is** the accumulation, so the open
/// blocks, the environment and any settled rule travel as one owned value instead of living in
/// a hidden accumulator. Rules are then worked on **independently**: `flat_map` defaults to
/// `usize::MAX` concurrency and `Shared::from_stream` hands its work to the scheduler, which on
/// native is `tokio::spawn`. Document order is carried by the owned `order` key and restored by
/// one stable sort after the concurrent merge (no `merge_all(1)` serialising the fan-out away,
/// and no recursive descent anywhere). CSS text appears only at the single terminal.
pub async fn compile(content: impl Into<String>) -> Result<String, SassError> {
    let _span = tracing::info_span!("compile").entered();
    Shared::from_iter(content.into().chars().collect::<Vec<char>>())
        .scan_map(LexAcc::default(), |acc, ch| acc.feed(ch))
        .flat_map(Shared::from_iter)
        .scan_map(MsgAcc::default(), |acc, tok| acc.feed(tok))
        .flat_map(Shared::from_iter)
        .scan(State::default(), State::step)
        .filter_map(State::into_rule)
        .flat_map(|rule| {
            let order = rule.order;
            // Each settled rule is worked on independently inside the scheduled task, not on
            // the thread that produced it; `order` is what puts the concurrent merge back into
            // document order.
            Shared::from_stream(futures::stream::once(async move { (order, rule.to_css()) }))
        })
        .collect::<Vec<(u32, String)>>()
        .map(|mut expanded| {
            expanded.sort_by_key(|(order, _)| *order);
            expanded
        })
        .flat_map(Shared::from_iter)
        .map(|(_, css)| css)
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
