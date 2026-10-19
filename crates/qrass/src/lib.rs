//! qrass - a reactive SCSS→CSS compiler built on rxrust ownership pipelines.
//!
//! Each pipeline stage builds an Observable and drains it in place
//! (the shared `drain` terminal, `collect` / `reduce_initial` folds), passing
//! *owned* plain data between stages. No `Arc<Mutex<_>>` business state
//! anywhere. Orchestration is builder-style: see [`pipeline::PipelineBuilder`].
//!
//! Context (design D1, wasm contract): every chain in the crate is built on the
//! `Local` context — the per-item stage kernels and the module-singleton bus
//! (`module_bus`, one `BehaviorSubject` per canonical path) alike — so nothing
//! requires `Send`/`Sync` or the framework's `Arc`/`Mutex` plumbing, and the whole
//! compiler runs on a single-threaded `wasm32-unknown-unknown` target. The
//! singleton payload itself stays a read-only `Arc<Module>` (shared data, not
//! shared mutability).

use std::path::Path;

mod clock;
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
pub mod selector_ast;
pub mod selector_attr;
pub mod selector_extend;
pub mod selector_extend_pseudo;
pub mod selector_extend_weave;
pub mod selector_parse;
pub mod selector_super;
mod selector_super_simple;
pub mod selector_unify;
pub mod selector_unify_complex;
pub mod value;
mod value_number;

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

/// Compile an in-memory SCSS source — the file-system-free entry the
/// `wasm32-unknown-unknown` (browser) integration uses, and a plain convenience
/// API on every target. `name` is the display name **and** the base a relative
/// `@use` resolves against, exactly like an on-disk entry path.
pub fn compile_string(
    name: impl Into<String>,
    content: impl Into<String>,
    opts: &Options,
) -> Result<String, SassError> {
    pipeline::build()
        .source(name, content)
        .style(opts.style)
        .load_paths(opts.load_paths.clone())
        .drain()
}
