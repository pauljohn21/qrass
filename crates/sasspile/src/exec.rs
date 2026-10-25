//! Fold — the exec stage's **owned accumulator value** plus the data moves it makes.
//!
//! No observable type crosses into or out of this file: `compile` in the crate root owns the
//! operator chain, so the chain — not a method on my struct — is where control flow reads.
//! Nesting depth lives here as a `Vec<Scope>` **value** carried by `scan_map` (depth is data,
//! not a call stack and not an object that decides what to visit next), and a closing block
//! hands back the rule it settled as an `Option`: whether that becomes CSS is the chain's
//! `filter_map`, not this type's call.

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

/// One currently open block, as data.
#[derive(Debug)]
struct Scope {
    selector: String,
    decls: Vec<(String, Value)>,
    order: u32,
}

/// The value `scan_map` carries while the `Event` stream moves forward.
#[derive(Default)]
pub struct Fold {
    scopes: Vec<Scope>,
    next: u32,
    env: Env,
}

impl Fold {
    /// Move one event into the accumulator and report the block that just closed, if any.
    /// Pure bookkeeping: it emits nothing and decides nothing about CSS.
    pub fn step(&mut self, ev: Event) -> Option<StyleRule> {
        match ev {
            Event::Open(head) => {
                let parent = self.scopes.last().map(|s| s.selector.clone()).unwrap_or_default();
                let selector = combine(&parent, &interp_text(&self.env, &head));
                let order = self.next;
                self.next += 1;
                self.scopes.push(Scope { selector, decls: Vec::new(), order });
                None
            }
            Event::Decl { prop, value } => {
                // The left side of `$x: …` is a binding target: its name is taken verbatim and
                // is never interpolated, unlike an ordinary property name.
                let name = match prop.as_slice() {
                    [Tok::Var(var)] => format!("${var}"),
                    _ => interp_text(&self.env, &prop),
                };
                match name.strip_prefix('$') {
                    // `$x: …` is a binding in the environment, never a CSS declaration.
                    Some(var) => {
                        let bound = evaluated(&self.env, &value)
                            .unwrap_or_else(|| Value::from_text(render(&value)));
                        self.env.bind(var, bound);
                    }
                    None => {
                        let owned = decl_value(&self.env, &name, &value);
                        if let Some(top) = self.scopes.last_mut() {
                            top.decls.push((name, owned));
                        }
                    }
                }
                None
            }
            Event::Close => self
                .scopes
                .pop()
                .filter(|s| !s.decls.is_empty() && !s.selector.is_empty())
                .map(|s| StyleRule { selector: s.selector, decls: s.decls, order: s.order }),
        }
    }
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

/// The declaration's owned value. A custom property (`--x: …`) and a value carrying a CSS
/// native function (`var()`, `calc()`, `url()`, …) hold **raw CSS text** that Sass must not
/// interpret; the first is stored verbatim, the others bail inside [`evaluated`]. Any other
/// value becomes a typed [`Value`] when — and only when — the run is unambiguous; a run that
/// does not parse, or whose evaluation errors, keeps its verbatim text for now.
fn decl_value(env: &Env, prop: &str, value: &[Tok]) -> Value {
    match prop {
        p if p.starts_with("--") => Value::from_text(render_raw(value)),
        _ => evaluated(env, value).unwrap_or_else(|| Value::from_text(render(value))),
    }
}

/// Fold a token run into a typed value. A lone number is **not** an expression yet — only an
/// operation (or an interpolation) earns evaluation, so `w: 10px` keeps its exact source
/// digits instead of being normalised to `10px` from a float.
fn evaluated(env: &Env, toks: &[Tok]) -> Option<Value> {
    let arithmetic = expr::from_tokens(toks).and_then(|expr| match expr {
        Expr::Num(_, _) => None,
        other => Some(other),
    });
    arithmetic
        .or_else(|| expr::interpolated(toks))
        .and_then(|expr| expr.eval(env).ok())
}

/// A token run that only **interpolates**: a selector or property name may resolve `#{}` and
/// `$var`, but its lexemes must never be read as numbers (`.2xl` is an identifier).
fn interp_text(env: &Env, toks: &[Tok]) -> String {
    expr::interpolated(toks)
        .and_then(|expr| expr.eval(env).ok())
        .map(|value| value.to_css())
        .unwrap_or_else(|| render(toks))
}

/// Parent/child selector combination (Sass semantics): `&` replaces the parent, else descendant.
fn combine(parent: &str, child: &str) -> String {
    match parent {
        "" => child.to_string(),
        p if child.contains('&') => child.replace('&', p),
        p => format!("{p} {child}"),
    }
}
