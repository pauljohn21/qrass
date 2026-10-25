//! State — the running program state, carried as **the item the stream emits**.
//!
//! This is the framework's own shape for stateful forward computation: `scan` emits the
//! accumulation itself, so nothing is hidden inside a private accumulator (that was `scan_map`)
//! and nothing has to be smuggled alongside each declaration (that was an `Arc<Env>` snapshot
//! per decl, invented to let evaluation be deferred). The env, the open blocks and the rule
//! this step produced all live inside the value that flows, so a declaration is evaluated at
//! the position it appears — which is exactly Sass's document-order semantics.
//!
//! Nesting depth is data inside this value (`scopes`), never a call stack; no observable type
//! appears here, because `compile` (crate root) owns the one operator chain.

use crate::env::Env;
use crate::expr::{self, Expr};
use crate::lexer::{Event, Tok, render, render_raw};
use crate::value::Value;

/// A resolved CSS rule in document order.
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: String,
    pub decls: Vec<(String, Value)>,
    pub order: u32,
}

impl StyleRule {
    /// Serialise the rule to an expanded CSS block. [`Value::to_css`] stays the one place a
    /// value becomes text.
    pub fn to_css(&self) -> String {
        let body = self
            .decls
            .iter()
            .map(|(prop, value)| format!("  {prop}: {};\n", value.to_css()))
            .collect::<String>();
        format!("{} {{\n{}}}\n", self.selector, body)
    }
}

/// One currently open block, as data inside the flowing state.
#[derive(Debug, Clone)]
struct Scope {
    selector: String,
    decls: Vec<(String, Value)>,
    order: u32,
}

/// The value `scan` carries and emits: open blocks, the lexical environment, the document
/// order counter, and the rule (if any) that the latest event settled.
#[derive(Debug, Clone, Default)]
pub struct State {
    scopes: Vec<Scope>,
    env: Env,
    next: u32,
    emitted: Option<StyleRule>,
}

impl State {
    /// Take the rule the latest event settled, if any — the only thing the chain reads off a
    /// state value.
    pub fn into_rule(self) -> Option<StyleRule> {
        self.emitted
    }

    /// Move one event into the state and hand the whole state back to the stream. Only the
    /// settled rule is ever read downstream, through `filter_map` in `compile`.
    pub fn step(mut self, ev: Event) -> State {
        self.emitted = None;
        match ev {
            Event::Open(head) => {
                let parent = self.scopes.last().map(|s| s.selector.clone()).unwrap_or_default();
                let selector = combine(&parent, &self.interp_text(&head));
                let order = self.next;
                self.next += 1;
                self.scopes.push(Scope { selector, decls: Vec::new(), order });
            }
            Event::Decl { prop, value } => {
                // The left side of `$x: …` is a binding target: its name is taken verbatim and
                // is never interpolated, unlike an ordinary property name.
                let name = match prop.as_slice() {
                    [Tok::Var(var)] => format!("${var}"),
                    _ => self.interp_text(&prop),
                };
                match name.strip_prefix('$') {
                    // A binding updates the environment inside the flowing value; a later
                    // declaration reads the value this position has, so document order holds
                    // without any snapshot being carried around.
                    Some(var) => self.env.bind(var, self.value_of(&value)),
                    None => {
                        let owned = self.decl_value(&name, &value);
                        if let Some(top) = self.scopes.last_mut() {
                            top.decls.push((name, owned));
                        }
                    }
                }
            }
            Event::Close => {
                self.emitted = self
                    .scopes
                    .pop()
                    .filter(|s| !s.decls.is_empty() && !s.selector.is_empty())
                    .map(|s| StyleRule { selector: s.selector, decls: s.decls, order: s.order });
            }
        }
        self
    }

    /// The declaration's owned value. A custom property (`--x: …`) and a value carrying a CSS
    /// native function (`var()`, `calc()`, `url()`, …) hold **raw CSS text** that Sass must not
    /// interpret; the first is stored verbatim, the others bail inside [`State::evaluated`].
    /// Any other value becomes a typed [`Value`] when — and only when — the run is
    /// unambiguous; a run that does not parse, or whose evaluation errors, keeps its verbatim
    /// text for now.
    fn decl_value(&self, prop: &str, value: &[Tok]) -> Value {
        match prop {
            p if p.starts_with("--") => Value::from_text(render_raw(value)),
            _ => self
                .evaluated(value)
                .unwrap_or_else(|| Value::from_text(render(value))),
        }
    }

    fn value_of(&self, toks: &[Tok]) -> Value {
        self.evaluated(toks).unwrap_or_else(|| Value::from_text(render(toks)))
    }

    /// Fold a token run into a typed value. A lone number is **not** an expression yet — only
    /// an operation (or an interpolation) earns evaluation, so `w: 10px` keeps its exact source
    /// digits instead of being normalised to `10px` from a float.
    fn evaluated(&self, toks: &[Tok]) -> Option<Value> {
        let arithmetic = expr::from_tokens(toks).and_then(|expr| match expr {
            Expr::Num(_, _) => None,
            other => Some(other),
        });
        arithmetic
            .or_else(|| expr::interpolated(toks))
            .and_then(|expr| expr.eval(&self.env).ok())
    }

    /// A token run that only **interpolates**: a selector or property name may resolve `#{}`
    /// and `$var`, but its lexemes must never be read as numbers (`.2xl` is an identifier).
    fn interp_text(&self, toks: &[Tok]) -> String {
        expr::interpolated(toks)
            .and_then(|expr| expr.eval(&self.env).ok())
            .map(|value| value.to_css())
            .unwrap_or_else(|| render(toks))
    }
}

/// Parent/child selector combination (Sass semantics): `&` replaces the parent, else descendant.
fn combine(parent: &str, child: &str) -> String {
    match parent {
        "" => child.to_string(),
        p if child.contains('&') => child.replace('&', p),
        p => format!("{p} {child}"),
    }
}
