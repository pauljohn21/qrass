//! Eval stage (task 5.3): fold the owned statement tree into a stream of CSS
//! rule chunks. The reactive shape mirrors the lexer/parser: `from_iter(program)`
//! drives a `scan_map(&mut EvalState, …)` transducer that carries the [`Env`]
//! **by value**, emitting the owned chunks produced by each top-level statement
//! (which `flat_map` spreads into the event stream that render reduces).
//!
//! Nested traversal lives in [`eval_stmt`] (an `impl EvalState` sibling) and
//! expression evaluation in [`eval_expr`] — structural recursion over owned
//! `&[Stmt]` / `&Expr` with the environment threaded through `&mut self.env`,
//! never a shared/global mutable cell.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;

use rxrust::prelude::*;

use crate::ast::Stmt;
use crate::ast::{FunctionDecl, MixinDecl};
use crate::env::{Env, ScopeId};
use crate::error::SassError;
use crate::module::Module;
use crate::parser::parse;
use crate::span::FileId;
use crate::token::Piece;

#[path = "eval_control.rs"]
mod eval_control;
#[path = "eval_color4.rs"]
mod eval_color4;
#[path = "eval_expr.rs"]
mod eval_expr;
#[path = "eval_extend.rs"]
mod eval_extend;
#[path = "eval_function.rs"]
mod eval_function;
#[path = "eval_if.rs"]
mod eval_if;
#[path = "eval_mixin.rs"]
mod eval_mixin;
#[path = "eval_module.rs"]
mod eval_module;
#[path = "eval_stmt.rs"]
mod eval_stmt;

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
    /// Instantiated modules keyed by canonical path (instantiate-once).
    pub modules: HashMap<String, Module>,
    /// Canonical paths on the current `@use` chain, for cycle detection.
    pub stack: Vec<String>,
    /// Per-instantiation-frame buffer of `@forward`ed exports (task 6.3).
    pub forward_buf: Vec<Vec<(String, crate::value::SassValue)>>,
    /// Per-instantiation-frame set of `@use … with` config names still awaiting
    /// a matching `!default` declaration (unconsumed ⇒ error).
    pub pending_config: Vec<std::collections::HashSet<String>>,
    /// Mixin registry keyed by name (module mixins are rebound per namespace).
    pub mixins: HashMap<String, MixinDecl>,
    /// Function registry keyed by name (task 7.2).
    pub functions: HashMap<String, FunctionDecl>,
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
            modules: HashMap::new(),
            stack: Vec::new(),
            forward_buf: Vec::new(),
            pending_config: Vec::new(),
            mixins: HashMap::new(),
            functions: HashMap::new(),
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
        self.module_envs.insert(self.env_owner.clone(), self.env.clone());
        self.parked.push((std::mem::take(&mut self.env), self.env_owner.clone()));
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
        self.module_envs.insert(self.env_owner.clone(), self.env.clone());
        if let Some((env, owner)) = self.parked.pop() {
            self.env = env;
            self.env_owner = owner;
        }
    }
}

/// Fold the whole program into owned CSS chunks (synchronous: used by tests and
/// by the pipeline, which drives [`eval_stream`] and collects it).
pub fn eval_all(
    file: FileId,
    name: String,
    content: String,
    program: Vec<Stmt>,
) -> Result<Vec<String>, SassError> {
    let mut st = EvalState::new(file, name, content);
    let mut chunks: Vec<String> = Vec::new();
    for stmt in &program {
        st.eval_top_level(stmt, &mut chunks)?;
    }
    // Global `@extend` pass (D4): placeholders and extend sites are only fully
    // known once the whole module is traversed, so resolution substitutes each
    // placeholder's source-position marker in place.
    st.extend_pass(&mut chunks)?;
    Ok(chunks)
}

/// Cold CSS-event stream. `@extend` is a global constraint — placeholder rules
/// and their extend sites are only fully known after the whole module is
/// traversed — so per-statement chunks are buffered and each placeholder's
/// source-position marker is substituted in place by the `@extend` pass before
/// the resolved chunks are emitted and the stream completes. Errors
/// short-circuit via the value channel.
pub fn eval_stream(
    file: FileId,
    name: String,
    content: String,
    program: Vec<Stmt>,
    load_paths: Vec<PathBuf>,
) -> LocalBoxedObservable<'static, Ev<String>, Infallible> {
    Local::create(
        move |emitter: &mut dyn rxrust::observer::Emitter<Ev<String>, Infallible>| {
            let mut st = EvalState::new(file, name, content);
            st.load_paths = load_paths;
            let mut chunks: Vec<String> = Vec::new();
            for stmt in &program {
                if let Err(e) = st.eval_top_level(stmt, &mut chunks) {
                    emitter.next(Err(e));
                    emitter.complete();
                    return;
                }
            }
            if let Err(e) = st.extend_pass(&mut chunks) {
                emitter.next(Err(e));
                emitter.complete();
                return;
            }
            for chunk in chunks {
                emitter.next(Ok(chunk));
            }
            emitter.complete();
        },
    )
    .box_it()
}
