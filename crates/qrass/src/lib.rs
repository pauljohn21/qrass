//! qrass - a reactive SCSS→CSS compiler built on rxrust ownership pipelines.
//!
//! Each pipeline stage builds an Observable and drains it in place
//! (the shared `drain` terminal, `collect` / `reduce_initial` folds), passing
//! *owned* plain data between stages. No `Arc<Mutex<_>>` business state
//! anywhere. Orchestration is builder-style: see [`pipeline::PipelineBuilder`].
//!
//! Context layering (design D1): the per-item stage kernels are `Local`, while the
//! module singletons are broadcast by a `Shared` multicast carrier
//! (`module_bus`, one `BehaviorSubject` per canonical path) — the only place the
//! framework's own `Arc`/`Mutex` plumbing is allowed, and it keeps [`eval::EvalState`]
//! `Send + Sync` for the concurrent-branch step of D1.

use std::path::Path;

mod diag;
mod drain;
pub mod error;
pub mod lexer;
pub mod loader;
pub mod pipeline;
pub mod span;
pub mod token;
pub mod tracing_setup;

pub mod ast;
pub mod ast_expr;
mod builtins;
pub mod color4;
mod color4_convert;
mod color_css;
mod css;
pub mod env;
pub mod eval;
pub mod module;
mod module_bus;
pub mod parser;
mod plain_css;
pub mod value;

use error::SassError;

/// Supported output styles. Only [`OutputStyle::Expanded`] exists for now;
/// requesting anything else is rejected by the CLI before compiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputStyle {
    Expanded,
}

/// Compilation options.
#[derive(Clone, Debug)]
pub struct Options {
    pub style: OutputStyle,
    /// Sass `--load-path` roots: consulted (in order, after the importing
    /// file's own directory) when resolving `@use`/`@forward` URLs. Empty by
    /// default so relative resolution alone matches the element-plus load.
    pub load_paths: Vec<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            style: OutputStyle::Expanded,
            load_paths: Vec::new(),
        }
    }
}

/// Compile `entry` (a SCSS/CSS file) into expanded CSS text.
pub fn compile(entry: &Path, opts: &Options) -> Result<String, SassError> {
    pipeline::build()
        .entry(entry)
        .style(opts.style)
        .load_paths(opts.load_paths.clone())
        .drain()
}
