//! flat_map-driven reactive pipeline.
//!
//! Each compiler stage is a *cold stream function* returning a `Stage<T>` (a
//! type-erased `Local` observable whose value channel carries `Result<T,
//! SassError>` and whose error channel is `Infallible`). Stages connect via
//! [`flat_map`](Observable::flat_map) so **every inter-stage transition is
//! driven by a stream emission**, not by sequential statements. An error
//! short-circuits through [`fail`] straight to the terminal. The builder holds
//! owned config only; a single `mpsc` terminal drains the result by value.
//!
//! This is the qrass landing template from `docs/rxrust-ai-manual.md` §8.

use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use rxrust::observer::Emitter;
use rxrust::prelude::*;

use crate::OutputStyle;
use crate::error::{ErrorKind, SassError};
use crate::span::{FileId, Files};
use crate::token::Spanned;

/// A type-erased stage stream: value channel is `Result<T, SassError>`, error
/// channel is `Infallible` (errors ride the value channel as `Result::Err`).
type Stage<T> = LocalBoxedObservable<'static, Result<T, SassError>, Infallible>;

/// Stage payloads: each owns the data for the next stage, moved by value.
#[derive(Clone)]
struct Loaded {
    /// Source registry; read starting in the module system (task 6) when @use/@forward
    /// resolves additional files. Unused by the single-entry lex→parse→eval chain.
    #[allow(dead_code)]
    files: Files,
    file: FileId,
    name: String,
    content: String,
}
#[derive(Clone)]
struct Lexed {
    loaded: Loaded,
    tokens: Vec<Spanned>,
}
#[derive(Clone)]
struct Parsed {
    lexed: Lexed,
    program: Vec<crate::ast::Stmt>,
}
#[derive(Clone)]
struct Evaled {
    chunks: Vec<String>,
}
#[derive(Clone)]
struct Rendered {
    css: String,
}

/// Short-circuit: carry an error straight to the terminal (same `Stage<T>` shape).
fn fail<T: 'static>(e: SassError) -> Stage<T> {
    Local::of(Err(e)).box_it()
}

/// Load a single entry file into a [`Stage`] emitting one owned [`Loaded`].
fn load_stage(path: PathBuf) -> Stage<Loaded> {
    let module = path.display().to_string();
    let started = Instant::now();
    let span = tracing::span!(
        tracing::Level::INFO,
        "compile",
        stage = "load",
        module = module.as_str(),
        elapsed_ms = tracing::field::Empty
    );
    let span2 = span.clone();
    Local::create::<Result<Loaded, SassError>, Infallible, _, _>(
        move |emitter: &mut dyn Emitter<Result<Loaded, SassError>, Infallible>| {
            let _g = span2.enter();
            let out = (|| -> Result<Loaded, SassError> {
                let mut files = Files::default();
                let src = crate::loader::load_entry(&path, &mut files)?;
                let content = src
                    .content(&files)
                    .ok_or_else(|| {
                        SassError::new(ErrorKind::Load, "entry source vanished from registry")
                    })?
                    .to_owned();
                let name = files.name(src.file).to_owned();
                Ok(Loaded {
                    files,
                    file: src.file,
                    name,
                    content,
                })
            })();
            emitter.next(out);
            emitter.complete();
        },
    )
    .tap(move |_| {
        span.record(
            "elapsed_ms",
            &tracing::field::debug(started.elapsed().as_millis() as u64),
        );
    })
    .box_it()
}

/// Lex stage: pipe the loaded source through the rxrust token stream.
fn lex_stage(loaded: Loaded) -> Stage<Lexed> {
    let started = Instant::now();
    let span = tracing::span!(
        tracing::Level::INFO,
        "compile",
        stage = "lex",
        module = loaded.name.as_str(),
        elapsed_ms = tracing::field::Empty
    );
    crate::lexer::token_stream(loaded.file, loaded.name.clone(), loaded.content.clone())
        .collect::<Vec<_>>()
        .map(move |items| {
            let result: Result<Vec<_>, _> = items.into_iter().collect();
            result.map(|tokens| Lexed {
                loaded: loaded.clone(),
                tokens,
            })
        })
        .tap(move |_| {
            span.record(
                "elapsed_ms",
                &tracing::field::debug(started.elapsed().as_millis() as u64),
            );
        })
        .box_it()
}

/// Parse stage: fold the token stream into an owned program via the reactive
/// per-token parser, then hand the tree to eval (tokens stay on `Parsed.lexed`).
fn parse_stage(lexed: Lexed) -> Stage<Parsed> {
    let started = Instant::now();
    let span = tracing::span!(
        tracing::Level::INFO,
        "compile",
        stage = "parse",
        module = lexed.loaded.name.clone(),
        tokens = lexed.tokens.len(),
        stmts = tracing::field::Empty,
        elapsed_ms = tracing::field::Empty
    );
    let src = lexed.loaded.clone();
    let span2 = span.clone();
    crate::parser::parse_stream(
        src.file,
        src.name.clone(),
        src.content.clone(),
        lexed.tokens.clone(),
    )
    .collect::<Vec<_>>()
    .map(move |items| {
        let _g = span2.enter();
        let result: Result<Vec<_>, _> = items.into_iter().collect();
        result.map(|program| {
            span2.record("stmts", &tracing::field::debug(program.len()));
            Parsed {
                lexed: lexed.clone(),
                program,
            }
        })
    })
    .tap(move |_| {
        let _g = span.enter();
        span.record(
            "elapsed_ms",
            &tracing::field::debug(started.elapsed().as_millis() as u64),
        );
    })
    .box_it()
}

/// Eval stage: fold the owned program into CSS chunks through the reactive
/// per-statement eval stream, short-circuiting on the first error.
fn eval_stage(parsed: Parsed, load_paths: Vec<PathBuf>) -> Stage<Evaled> {
    let started = Instant::now();
    let loaded = &parsed.lexed.loaded;
    let span = tracing::span!(
        tracing::Level::INFO,
        "compile",
        stage = "eval",
        module = loaded.name.as_str(),
        stmts = parsed.program.len(),
        chunks = tracing::field::Empty,
        elapsed_ms = tracing::field::Empty
    );
    let span2 = span.clone();
    crate::eval::eval_stream(
        loaded.file,
        loaded.name.clone(),
        loaded.content.clone(),
        parsed.program.clone(),
        load_paths,
    )
    .collect::<Vec<_>>()
    .map(move |items| {
        let _g = span2.enter();
        let result: Result<Vec<String>, SassError> = items.into_iter().collect();
        result.map(|chunks| {
            span2.record("chunks", &tracing::field::debug(chunks.len()));
            Evaled { chunks }
        })
    })
    .tap(move |_| {
        let _g = span.enter();
        span.record(
            "elapsed_ms",
            &tracing::field::debug(started.elapsed().as_millis() as u64),
        );
    })
    .box_it()
}

/// Render stage: fold CSS chunks through rxrust `reduce_initial`, emitting one
/// owned document.
fn render_stage(evaled: Evaled) -> Stage<Rendered> {
    let started = Instant::now();
    let span = tracing::span!(
        tracing::Level::INFO,
        "compile",
        stage = "render",
        elapsed_ms = tracing::field::Empty
    );
    Local::from_iter(evaled.chunks)
        .reduce_initial(String::new(), |mut acc, chunk| {
            if !chunk.trim().is_empty() {
                if !acc.is_empty() {
                    acc.push('\n');
                }
                acc.push_str(chunk.trim());
            }
            acc
        })
        .map(move |css| Ok(Rendered { css }))
        .tap(move |_| {
            span.record(
                "elapsed_ms",
                &tracing::field::debug(started.elapsed().as_millis() as u64),
            );
        })
        .box_it()
}

/// Collects pipeline configuration; finish with [`drain`](Self::drain).
pub struct PipelineBuilder {
    entry: Option<PathBuf>,
    style: OutputStyle,
    load_paths: Vec<PathBuf>,
}

impl PipelineBuilder {
    pub fn entry(mut self, path: impl AsRef<Path>) -> Self {
        self.entry = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn style(mut self, style: OutputStyle) -> Self {
        self.style = style;
        self
    }

    /// Register Sass `--load-path` roots for `@use`/`@forward` resolution.
    pub fn load_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.load_paths = paths;
        self
    }

    /// Wire load → lex → parse → eval → render via `flat_map` and drain once.
    pub fn drain(self) -> Result<String, SassError> {
        let entry = self
            .entry
            .ok_or_else(|| SassError::new(ErrorKind::Load, "pipeline has no entry file"))?;

        let (tx, rx) = mpsc::channel::<Result<String, SassError>>();
        let load_paths = self.load_paths.clone();
        load_stage(entry)
            .flat_map(|r| match r {
                Ok(loaded) => lex_stage(loaded),
                Err(e) => fail(e),
            })
            .flat_map(|r| match r {
                Ok(lexed) => parse_stage(lexed),
                Err(e) => fail(e),
            })
            .flat_map(move |r| match r {
                Ok(parsed) => eval_stage(parsed, load_paths.clone()),
                Err(e) => fail(e),
            })
            .flat_map(|r| match r {
                Ok(evaled) => render_stage(evaled),
                Err(e) => fail(e),
            })
            .map(|r| match r {
                Ok(rendered) => {
                    // the goldens prepend `@charset "UTF-8";` when the emitted CSS
                    // contains any non-ASCII byte (e.g. CJK font-family names).
                    let css = if rendered.css.is_ascii() {
                        format!("{}\n", rendered.css)
                    } else {
                        format!("@charset \"UTF-8\";\n{}\n", rendered.css)
                    };
                    Ok(css)
                }
                Err(e) => Err(e),
            })
            .subscribe(move |out| {
                tx.send(out).ok();
            });

        rx.recv().unwrap_or_else(|_| {
            Err(SassError::new(
                ErrorKind::Render,
                "pipeline produced no output",
            ))
        })
    }
}

/// Start a builder with defaults (expanded style).
pub fn build() -> PipelineBuilder {
    PipelineBuilder {
        entry: None,
        style: OutputStyle::Expanded,
        load_paths: Vec::new(),
    }
}
