//! Eval stage (task 5.3): fold the owned statement tree into a stream of CSS
//! rule chunks. The reactive shape mirrors the lexer/parser: `from_iter(program)`
//! drives a fold whose accumulator — the [`EvalFold`] carrying state, chunks and
//! a parked error — moves **by value** through every statement via
//! `reduce_initial`, and `flat_map` re-emits the resolved chunks that render
//! reduces. No `Clone` is needed to satisfy `FnMut` capture.
//!
//! Nested traversal lives in [`eval_stmt`] (an `impl EvalState` sibling) and
//! expression evaluation in [`eval_expr`] — structural recursion over owned
//! `&[Stmt]` / `&Expr` with the environment threaded through `&mut self.env`,
//! never a shared/global mutable cell.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;

use rxrust::prelude::*;

use self::branch_fanout::{BranchKey, Prepared};
use crate::ast::Stmt;
use crate::ast::{FunctionDecl, MixinDecl};
use crate::clock::StopWatch;
use crate::env::{Env, ScopeId};
use crate::error::SassError;
use crate::module_bus::ModuleBus;
use crate::parser::parse;
use crate::span::FileId;
use crate::token::Piece;

#[path = "branch_fanout.rs"]
mod branch_fanout;
#[path = "eval_arith.rs"]
mod eval_arith;
#[path = "eval_bind.rs"]
mod eval_bind;
#[path = "eval_color4.rs"]
mod eval_color4;
#[path = "eval_color4_check.rs"]
mod eval_color4_check;
#[path = "eval_color_space.rs"]
mod eval_color_space;
#[path = "eval_color_space_symbolic.rs"]
mod eval_color_space_symbolic;
#[path = "eval_callable.rs"]
mod eval_callable;
#[path = "eval_callable_call.rs"]
mod eval_callable_call;
#[path = "eval_config.rs"]
mod eval_config;
#[path = "eval_control.rs"]
mod eval_control;
#[path = "eval_expr.rs"]
mod eval_expr;
#[path = "eval_extend.rs"]
mod eval_extend;
#[path = "eval_function.rs"]
mod eval_function;
#[path = "eval_if.rs"]
mod eval_if;
#[path = "eval_load_css.rs"]
mod eval_load_css;
#[path = "eval_mixin.rs"]
mod eval_mixin;
#[path = "eval_module.rs"]
mod eval_module;
#[path = "eval_stmt.rs"]
mod eval_stmt;

/// One `with (…)` value on offer. A `@use … with` entry also gets recorded under
/// its site (see [`EvalState::config_sites`]) so the statement can check it was
/// claimed; a `@forward … with` value the forwarder supplied itself belongs to no
/// upstream declaration and is never checked. `consumed` flips the moment a
/// `!default` declaration anywhere down the `@forward` chain claims it — the
/// check that decides `directives/use/error/with/undefined.hrx`.
pub(crate) struct ConfigOffer {
    /// Original spelling of the configured name (`$a-b`), quoted verbatim by
    /// the load-css family's error messages.
    pub(crate) name: String,
    pub(crate) value: crate::value::SassValue,
    pub(crate) consumed: bool,
    /// Where the entry was written, for the golden's marker run.
    pub(crate) entry: crate::span::Span,
}

/// One live `@use`/`@forward` edge of the module-load stack. The goldens render
/// this chain under an error's snippet: each ancestor frame shows the position
/// of the statement that pulled the next file in, inside the file that wrote it
/// (`directives/forward/error/with.hrx` `namespace`: `_downstream.scss 1:28 @use`
/// then `input.scss 1:1 root stylesheet`). `keyword` labels the frame *below*
/// this link, since a frame is captioned by how it entered.
pub(crate) struct LoadLink {
    /// The file that wrote the `@use`/`@forward`.
    pub(crate) parent_file: String,
    /// Where in that file it was written, already resolved to 1-based line/col.
    pub(crate) parent_line: u32,
    pub(crate) parent_col: u32,
    /// `"@use"` or `"@forward"` — the golden's frame label.
    pub(crate) keyword: &'static str,
}

/// A rule whose selector is (or contains) a placeholder `%name`, collected
/// structurally so the global `@extend` pass (task 7.5) can re-emit it for every
/// concrete selector that extends the placeholder. Placeholders never render on
/// their own, so these rules bypass the normal chunk stream entirely.
pub(crate) struct PlaceholderRule {
    pub selector: String,
    pub decls: Vec<(String, String)>,
    /// Index of the in-stream marker chunk this placeholder was emitted at, so
    /// the `@extend` pass can substitute the grouped extended rule back into its
    /// original *source position* (matching the goldens) instead of appending it.
    pub slot: usize,
}

/// The `@content` block captured at an `@include` site, replayed when the mixin
/// body reaches `@content` (task 7.1). `site_scope` is the lexical scope of the
/// call, so content sees the include site's variables rather than the mixin's.
#[derive(Clone)]
pub(crate) struct ContentCtx {
    pub block: Vec<Stmt>,
    pub params: Vec<String>,
    /// The include site's **full** environment (arena + active scope): a mixin
    /// body runs in its defining module's scope, so `@content` must restore the
    /// caller's whole arena, not merely a scope id from a now-discarded env.
    pub site_env: Env,
    pub site_scope: ScopeId,
    /// Which module owns [`site_env`](Self::site_env) — restored as `env_owner`
    /// while the content block replays so nested closures resolve correctly.
    pub site_owner: String,
}

/// Accumulator moved through the eval fold: the scope arena plus rendering hooks.
pub struct EvalState {
    pub env: Env,
    pub file: FileId,
    pub name: String,
    pub content: String,
    /// The L2 module singletons (design D1): one `Local` multicast carrier per
    /// canonical path, so a diamond `@use` broadcasts the *same* `Arc<Module>` to
    /// every parent instead of copying — or re-running — the instantiation.
    pub(crate) bus: ModuleBus,
    /// The L2 branch fan-out's memoized sources (design D1, manual §2): what the
    /// `Local` `flat_map` over a body's `@use`/`@forward` branches acquired.
    /// Purely a cache — a missing entry makes `load_module` acquire the source
    /// inline through the same unit, so behaviour never depends on this map.
    pub(crate) prepared: HashMap<BranchKey, Result<Prepared, SassError>>,
    /// Canonical paths on the current `@use` chain, for cycle detection.
    pub stack: Vec<String>,
    /// The live `@use`/`@forward` chain, innermost last, behind an error's
    /// frame stack (see [`LoadLink`]).
    pub(crate) links: Vec<LoadLink>,
    /// Per-instantiation-frame buffer of `@forward`ed exports (task 6.3).
    pub forward_buf: Vec<crate::module::Forwarded>,
    /// Per-instantiation-frame view of the `with (…)` configuration visible to
    /// the module being run: `(normalized name, offer index)` pairs.
    pub(crate) config_stack: Vec<Vec<(String, usize)>>,
    /// Every `with (…)` value offered during the run, shared by index so one
    /// `!default` declaration anywhere down the `@forward` chain marks the
    /// original `@use` entry consumed (`with/distributed_vars.hrx`).
    pub(crate) config_offers: Vec<ConfigOffer>,
    /// Offer indices created by each `@use … with` site, keyed by site id, so
    /// the site can report exactly which entry no module declared.
    pub(crate) config_sites: Vec<Vec<usize>>,
    /// Mixin registry keyed by name (module mixins are rebound per namespace).
    pub mixins: HashMap<String, std::rc::Rc<MixinDecl>>,
    /// Function registry keyed by name (task 7.2).
    pub functions: HashMap<String, std::rc::Rc<FunctionDecl>>,
    /// The module bound to each `@use` namespace (round-73 W4): what
    /// `meta.module-functions`/`module-mixins` enumerate. `as *` binds no
    /// namespace and therefore records nothing (`module_functions` goldens:
    /// `@use "other" as *` then `module-functions("other")` is a
    /// no-namespace error).
    pub(crate) bound_modules: std::collections::HashMap<String, std::rc::Rc<crate::module::Module>>,
    /// Defining-module environment snapshot per closure: a `@function`/`@mixin`
    /// body resolves its free variables and `ns.$var` namespace bindings against
    /// the module it was **declared** in (the goldens' lexical scoping), not the
    /// caller's — so each module's private vars survive its `instantiate`.
    pub(crate) mixin_env: HashMap<String, Env>,
    pub(crate) function_env: HashMap<String, Env>,
    /// Canonical module path where each mixin/function was **declared**. When a
    /// closure is invoked from the same module that is currently being evaluated,
    /// its body runs against the *live* `self.env` (so accumulated `!global` writes
    /// and re-reads stay current); only cross-module calls fall back to the
    /// definition-time `mixin_env`/`function_env` snapshot for lexical resolution.
    pub(crate) mixin_module: HashMap<String, String>,
    pub(crate) function_module: HashMap<String, String>,
    /// Live, persistent environment per instantiated module (keyed by canonical
    /// path). Cross-module `@include`/function calls run against this env — not a
    /// definition-time clone — so `!global` writes into a shared helper module's
    /// globals (e.g. the BEM `$B`/`$E` state in `mixins/mixins`) survive and are
    /// visible to every later call into that module.
    pub(crate) module_envs: HashMap<String, Env>,
    /// Which module the current `self.env` belongs to. Nested closures defined in
    /// the same module run directly against it; entering a different module parks
    /// this env and swaps in that module's live env.
    pub(crate) env_owner: String,
    /// Parked caller `(env, owner)` stack while running cross-module closures.
    pub(crate) parked: Vec<(Env, String)>,
    /// Stack of active `@include` content blocks; `@content` replays the top.
    pub(crate) content_stack: Vec<ContentCtx>,
    /// `@extend` sites: `(placeholder target, concrete selector, optional)`.
    pub extends: Vec<(String, String, bool)>,
    /// Placeholder rules held back for the global `@extend` pass.
    pub(crate) placeholder_rules: Vec<PlaceholderRule>,
    /// Set while evaluating the *immediate* children of a `@at-root` block: the
    /// next ruleset lifts to the top level (drops the descendant prefix) while
    /// still resolving `&` against the outer parent, then clears the flag so its
    /// own nested body combines normally.
    pub(crate) at_root: bool,
    /// The current rule's resolved parent selectors, active while its body (and
    /// any `@content` replayed into it) is evaluated. `&` inside a `#{…}`
    /// *expression* (e.g. `$arrow-selector: #{& + '__arrow'}`) resolves to these,
    /// so the value is baked at definition time rather than re-interpolated at
    /// each use site — matching the goldens. Bare `&` combinators in a selector
    /// remain literal `Piece::Text` and are still handled by `combine_selectors`.
    pub(crate) current_parents: Vec<String>,
    /// Sass `--load-path` roots consulted (after the importer-relative search)
    /// when resolving `@use`/`@forward` URLs. Empty by default; set by the
    /// pipeline from [`crate::Options::load_paths`].
    pub(crate) load_paths: Vec<PathBuf>,
}

/// Result alias for eval fallibility (errors ride the value channel).
pub(crate) type Ev<T> = Result<T, SassError>;

impl EvalState {
    pub fn new(file: FileId, name: String, content: String) -> Self {
        let owner = name.clone();
        Self {
            env: Env::new(),
            file,
            name,
            content,
            bus: ModuleBus::new(),
            prepared: HashMap::new(),
            stack: Vec::new(),
            links: Vec::new(),
            forward_buf: Vec::new(),
            config_stack: Vec::new(),
            config_offers: Vec::new(),
            config_sites: Vec::new(),
            mixins: HashMap::new(),
            functions: HashMap::new(),
            bound_modules: HashMap::new(),
            mixin_env: HashMap::new(),
            function_env: HashMap::new(),
            mixin_module: HashMap::new(),
            function_module: HashMap::new(),
            module_envs: HashMap::new(),
            env_owner: owner,
            parked: Vec::new(),
            content_stack: Vec::new(),
            extends: Vec::new(),
            placeholder_rules: Vec::new(),
            at_root: false,
            current_parents: Vec::new(),
            load_paths: Vec::new(),
        }
    }

    /// Render an interpolated text run (`Vec<Piece>`) to CSS text, evaluating any
    /// `#{expr}` against the current environment. Used for selectors, property
    /// names and at-rule parameters.
    pub fn render_interp(&mut self, pieces: &[Piece]) -> Ev<String> {
        let mut out = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Expr(src) => out.push_str(&self.eval_source(src)?.interp_text()),
            }
        }
        Ok(out)
    }

    /// Re-lex/re-parse a raw `#{…}` source fragment as an expression value and
    /// evaluate it. The parser exposes statements, so the fragment is wrapped as
    /// a throwaway variable declaration and its value expression is evaluated.
    pub fn eval_source(&mut self, src: &str) -> Ev<crate::value::SassValue> {
        let wrapped = format!("$v: {src};");
        let tokens = crate::lexer::lex(self.file, self.name.clone(), wrapped.clone())?;
        let program = parse(self.file, self.name.clone(), wrapped, tokens)?;
        match program.first() {
            Some(Stmt::Variable(v)) => {
                let expr = v.value.clone();
                self.eval_expr(&expr)
            }
            _ => Err(SassError::new(
                crate::error::ErrorKind::Eval,
                "expected an expression",
            )),
        }
    }

    /// Enter a closure whose defining module is `def`. If we are already running
    /// inside `def` (the common nested same-module case), do nothing — the body
    /// shares the live `self.env`. Otherwise park the caller env and swap in that
    /// module's persistent live env so `!global` writes propagate across the whole
    /// call tree. Returns whether a swap happened (paired with `end_closure`).
    pub(crate) fn begin_closure(&mut self, def: &str) -> bool {
        if def == self.env_owner {
            return false;
        }
        let Some(live) = self.module_envs.get(def).cloned() else {
            // Unknown/uninstantiated module: fall back to the current env.
            return false;
        };
        // Commit the current module's live env before parking it, so a later
        // re-entry (e.g. a nested `@include` into this same module reached through
        // `@content`) reads the in-progress globals rather than a stale snapshot.
        self.module_envs
            .insert(self.env_owner.clone(), self.env.clone());
        self.parked
            .push((std::mem::take(&mut self.env), self.env_owner.clone()));
        self.env = live;
        self.env_owner = def.to_string();
        true
    }

    /// Leave a closure entered by [`begin_closure`](Self::begin_closure): write the
    /// (possibly mutated) module env back so later calls see the updates, then
    /// restore the parked caller env.
    pub(crate) fn end_closure(&mut self, swapped: bool) {
        if !swapped {
            return;
        }
        self.module_envs
            .insert(self.env_owner.clone(), self.env.clone());
        if let Some((env, owner)) = self.parked.pop() {
            self.env = env;
            self.env_owner = owner;
        }
    }
}

/// Fold the whole program into owned CSS chunks. A thin synchronous drain of
/// [`eval_stream`] through the standard `mpsc` terminal, so a test harness reads
/// the very stream the pipeline runs rather than a second implementation.
pub fn eval_all(
    file: FileId,
    name: String,
    content: String,
    program: Vec<Stmt>,
) -> Result<Vec<String>, SassError> {
    crate::drain::drain(eval_stream(file, name, content, program, Vec::new()))
}

/// Owned accumulator of the eval fold: the live [`EvalState`], the chunks it has
/// emitted, and the first error. The error is *parked* rather than unwound so the
/// fold keeps transporting one value by ownership (no `Clone` of the arena, no
/// shared cell); statements after the failure simply do not run.
pub(crate) struct EvalFold {
    st: EvalState,
    chunks: Vec<String>,
    failure: Option<SassError>,
}

impl EvalFold {
    /// Run one top-level statement, appending the chunks it produced.
    fn step(mut self, stmt: Stmt) -> Self {
        if self.failure.is_none() {
            let outcome = self.st.eval_top_level(&stmt, &mut self.chunks);
            if let Err(err) = outcome {
                self.failure = Some(err);
            }
        }
        self
    }

    /// Boundary emission: the global `@extend` pass (D4 — placeholders and their
    /// sites are only fully known once the whole module ran) substitutes each
    /// marker in place, then the resolved chunks leave as their own stream. A
    /// parked error short-circuits onto the value channel instead.
    fn finish(mut self) -> LocalBoxedObservable<'static, Ev<String>, Infallible> {
        // The batch pass is its own pipeline stage: one span with the module it
        // resolved and how many chunks left it, nested in the `eval` stage span.
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "extend",
            stage = "extend",
            module = %self.st.name,
            chunks = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = StopWatch::start();
        if self.failure.is_none() {
            let outcome = self.st.extend_pass(&mut self.chunks);
            if let Err(err) = outcome {
                self.failure = Some(err);
            }
        }
        span.record("chunks", self.chunks.len());
        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
        let Self {
            chunks,
            failure,
            st: _,
        } = self;
        match failure {
            Some(err) => Local::of(Err::<String, SassError>(err)).box_it(),
            None => Local::from_iter(chunks.into_iter().map(Ok::<String, SassError>)).box_it(),
        }
    }
}

/// Cold CSS-event stream, shaped like the lexer/parser transducers: the owned
/// program is driven statement-by-statement by `from_iter`, the [`EvalFold`]
/// moves by value through `reduce_initial` (which emits exactly its final
/// accumulator, so the state leaves the fold owned rather than borrowed back),
/// and `flat_map` re-emits the chunks resolved at that boundary.
pub fn eval_stream(
    file: FileId,
    name: String,
    content: String,
    program: Vec<Stmt>,
    load_paths: Vec<PathBuf>,
) -> LocalBoxedObservable<'static, Ev<String>, Infallible> {
    let mut st = EvalState::new(file, name, content);
    st.load_paths = load_paths;
    // The entry stylesheet is a body like any other: fan out its top-level
    // `@use`/`@forward` branches before its statements run (manual §2).
    st.prefetch_branches(&program);
    Local::from_iter(program)
        .reduce_initial(
            EvalFold {
                st,
                chunks: Vec::new(),
                failure: None,
            },
            |fold, stmt| fold.step(stmt),
        )
        .flat_map(|fold| fold.finish())
        .box_it()
}
