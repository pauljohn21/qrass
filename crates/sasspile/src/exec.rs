//! State — the running program state, held by the `Exec` phase kernel's **observer** (`crate::phases`).
//!
//! This is the framework's own shape for stateful forward computation: the state lives inside
//! `ExecObserver` and is moved by `&mut self` per event, while the stream carries only what the
//! phase actually produces (the settled rule). The previous form — `scan(seed, step)` — made the
//! accumulator **be** the emitted item, so every event cloned the whole `State` (open blocks, env,
//! rules) and `filter_map` then threw all but the rule away: correct decomposition, wrong vehicle.
//! Nothing is smuggled alongside a declaration either (no per-decl `Arc<Env>` snapshot), so a
//! declaration is still evaluated at the position it appears — Sass's document-order semantics.
//!
//! Nesting depth is data inside this value (`scopes`), never a call stack; no observable type
//! appears here, because `compile` (crate root) owns the one operator chain.

use crate::env::Env;
use crate::expr::{self, Expr};
use crate::lexer::{Event, Tok, render, render_raw};
use crate::value::Value;

/// A resolved CSS rule: one **declaration run** flushed from a block. A block can yield several
/// rules (`sass-spec` `css/style_rule` `declaration/interleaved/around_style_rule`), so this is not
/// "the block's whole body".
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: String,
    pub decls: Vec<(String, Value)>,
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
}

/// The program state the `Exec` kernel mutates as events flow: open blocks and the lexical
/// environment.
#[derive(Debug, Clone, Default)]
pub struct State {
    scopes: Vec<Scope>,
    env: Env,
}

impl State {
    /// Move one event into the state, returning the rule it settled, if any.
    ///
    /// **When a block's text settles** is what puts the output in document order without an
    /// ordering key and without re-sorting. Per `sass-spec/css/style_rule` `declaration/interleaved/**`:
    /// - opening a nested block first settles the enclosing block's **pending declaration run**
    ///   (`.a {b:c; .d{e:f}}` → `.a{b:c}` before `.a .d{e:f}`);
    /// - closing a block settles whatever it still holds (`.a {.b{c:d}; e:f}` → `.a{e:f}` **after**
    ///   `.a .b{c:d}`);
    /// - so one block split around children becomes several rules (`b:c`, then `g:h`).
    pub fn step(&mut self, ev: Event) -> Option<StyleRule> {
        match ev {
            Event::Open(head) => {
                let parent = self
                    .scopes
                    .last()
                    .map(|s| s.selector.clone())
                    .unwrap_or_default();
                let selector = combine(&parent, &self.interp_text(&head));
                let settled = self.settle_top();
                self.scopes.push(Scope {
                    selector,
                    decls: Vec::new(),
                });
                settled
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
                None
            }
            Event::Close => Self::settled(self.scopes.pop()),
        }
    }

    /// Settle the innermost block's pending declaration run **without** popping it: the block stays
    /// open for later declarations and children, but its text so far becomes a rule right now.
    fn settle_top(&mut self) -> Option<StyleRule> {
        let top = self.scopes.last_mut()?;
        if top.decls.is_empty() {
            return None;
        }
        Self::settled(Some(Scope {
            selector: top.selector.clone(),
            decls: std::mem::take(&mut top.decls),
        }))
    }

    /// Turn a finished or flushed scope into a rule; a declaration-less run, or one at the root
    /// (empty selector), settles to nothing.
    fn settled(scope: Option<Scope>) -> Option<StyleRule> {
        scope
            .filter(|s| !s.decls.is_empty() && !s.selector.is_empty())
            .map(|s| StyleRule {
                selector: s.selector,
                decls: s.decls,
            })
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
        self.evaluated(toks)
            .unwrap_or_else(|| Value::from_text(render(toks)))
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
