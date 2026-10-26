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

mod builtins;
mod calculation;
mod calls;
mod env;
mod exec;
mod expr;
mod lexer;
mod operations;
mod phases;
mod selector;
mod trace;
mod unit;
mod value;

use crate::phases::{Exec, Lexer, Render, Structurize};

/// Why a compile failed. The distinction is not cosmetic: the corpus treats the categories
/// differently — an unbound variable must abort (`sass-spec`
/// `directives/use/error/member/missing`: `Error: Undefined variable.`), while an unparseable run or a
/// call Sass prints verbatim is **defined passthrough** (`css/functions/special/**`,
/// `css/custom_properties/simple`, `css/selector/escaping::dollar_char`). So a failure can never be
/// collapsed into "emit the source text" by one blanket fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// a variable reference with no binding at this position
    UndefinedVariable,
    /// a known builtin called with fewer arguments than its signature has parameters; the message is
    /// the oracle's own (`Missing argument $number2.`, `core_functions/math/div.hrx::error/too_few_args`)
    MissingArgument,
    /// a known builtin called with more arguments than its signature accepts, again stated by the
    /// oracle (`Only 2 arguments allowed, but 3 were passed.`, `div.hrx::error/too_many_args`)
    TooManyArguments,
    /// a builtin's number parameter received another value type; again the oracle's own words
    /// (`$number: c is not a number.`, `core_functions/math/round.hrx::error/type`)
    NotANumber,
    /// two numbers of **known** different dimensions were compared; the oracle names them in the message
    /// (`1px and 2s have incompatible units.`, `core_functions/math/max.hrx::error/incompatible_units`).
    /// An unlisted unit never produces this — declining to verbatim is not the same as claiming a clash
    IncompatibleUnits,
    /// a number parameter required a specific unit (or none); again the oracle's words
    /// (`$number: Expected 1% to have no units.`, `core_functions/math/percentage.hrx::error/unit`)
    WrongUnits,
    /// everything else
    Other,
}

/// The kinds that abort the stream. An unbound variable is fatal because the corpus says so
/// (`directives/use/error/member/missing`, `core_functions/meta/call.hrx::error/if_args`: both expect
/// `Error: Undefined variable.`).
///
/// The three guards flipping already required are landed, and each one is a corpus shape, not a guess:
/// 1. a block **head** resolves only `#{ … }` — an `@for $i from …` header *binds* `$i`; looking it
///    up there invented errors for ~47 cases (`directives/for/{whitespace,comment}.hrx`);
/// 2. an unbound `$x` inside a **special** call is plain-CSS text (`expr::special_call`: arguments of
///    `url`/`calc`/`element`/`expression`/`progid:`), while a *bound* one still resolves
///    (`values/calculation/calc/no_operator::variable/unquoted_string`) and `min`/`max`/`clamp` are
///    not special at all (`css/functions/not_special.hrx` computes `-c-type(1 + 1)`);
/// 3. a declaration in a **plain-CSS at-rule body** (`@function --a()`) keeps `$x` as text
///    (`exec::plain_css_body`; `css/function.hrx::{lowercase,uppercase}/result/sass_script`,
///    `result/uppercase/sass_script`) while `#{1 + 1}` in the same body still evaluates
///    (`lowercase/result/interpolation`).
///
/// **Measured with a per-case-unique key and an error-message oracle** (see `tests/hrx.rs`): hrx
/// 599 → **600**, `regress=0`. The single gain is `core_functions/meta/call::error/if_args`.
/// The older "599 → 984 (+385)" figure was an artefact of the coarse oracle: the harness used to count
/// **any** error as satisfying an `error` case, and 384 of those 385 fixtures state a different message
/// (`Expected identifier.`, `$red: Expected 0.5px to have unit "%"…`, `DEPRECATION WARNING …`) that we
/// do not produce. Those cases stay FAIL until P1's unit algebra and P7's diagnostics emit the message
/// the oracle names.
const FATAL: &[ErrorKind] = &[
    ErrorKind::UndefinedVariable,
    ErrorKind::MissingArgument,
    ErrorKind::TooManyArguments,
    ErrorKind::NotANumber,
    ErrorKind::IncompatibleUnits,
    ErrorKind::WrongUnits,
];

/// Compiler error carried on the stream's **real** `Err` channel (`Observer::error`), never as a
/// value a fallback can quietly swallow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassError {
    pub message: String,
    pub kind: ErrorKind,
}

impl SassError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Other,
        }
    }

    pub(crate) fn typed(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    /// Must this failure terminate the stream instead of falling back to verbatim text?
    pub(crate) fn is_fatal(&self) -> bool {
        FATAL.contains(&self.kind)
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
    let resolved = Shared::from_iter(content.into().chars().collect::<Vec<char>>())
        .transform(|source| Lexer { source })
        .transform(|source| Structurize { source })
        .transform(|source| Exec { source })
        .transform(|source| Render { source })
        .reduce_initial(String::new(), |mut css, chunk| {
            css.push_str(&chunk);
            css
        })
        .into_future()
        .await;
    // A stream failure arrives as `Ok(Err(SassError))` and is returned **as-is** — the old terminal
    // relabelled every failure (`map_err(|_| SassError::new("reduce"))`), which is exactly the
    // swallow-this-error shape the real channel replaces. `reduce_initial` always emits its seed, so
    // the only outer error left is an empty program, which legitimately compiles to `""`.
    match resolved {
        Ok(compiled) => compiled,
        Err(rxrust::IntoFutureError::Empty) => Ok(String::new()),
        Err(_) => Err(SassError::new("stream produced no result")),
    }
}
