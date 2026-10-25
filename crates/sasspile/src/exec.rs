//! Exec (push): consume the structural `Event` stream forward with an **owned accumulator**
//! whose nesting state is a *data* stack `scopes: Vec<Scope>` (moved through `scan_map`, not a
//! `&mut self` object driving a tree walk, and not a call-stack recursion). A rule is **pushed
//! out** when its `Close` fires; document order is recovered by an owned `order` key (assigned
//! at `Open`, pre-order), not by re-walking anything.

use std::convert::Infallible;

use rxrust::prelude::*;

use crate::env::Env;
use crate::expr;
use crate::lexer::{Event, Tok, render, render_raw};
use crate::value::Value;

/// A resolved CSS rule in document order.
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: String,
    pub decls: Vec<(String, Value)>,
    pub order: u32,
}

#[derive(Debug)]
struct Scope {
    selector: String,
    decls: Vec<(String, Value)>,
    order: u32,
}

/// Owned `scan_map` accumulator: the open-scope stack and the lexical environment are plain
/// data living here, moved through the operator rather than shared behind a handle.
#[derive(Default)]
pub struct Exec {
    scopes: Vec<Scope>,
    next: u32,
    env: Env,
}

impl Exec {
    fn advance(&mut self, ev: Event) -> Vec<StyleRule> {
        match ev {
            Event::Open(head) => {
                let parent = self.scopes.last().map(|s| s.selector.clone()).unwrap_or_default();
                let selector = combine(&parent, &self.interp_text(&head));
                let order = self.next;
                self.next += 1;
                self.scopes.push(Scope { selector, decls: Vec::new(), order });
                Vec::new()
            }
            Event::Decl { prop, value } => {
                // The left side of `$x: …` is a binding target: its name is taken verbatim and
                // is never interpolated, unlike an ordinary property name.
                let name = match prop.as_slice() {
                    [Tok::Var(var)] => format!("${var}"),
                    _ => self.interp_text(&prop),
                };
                match name.strip_prefix('$') {
                    // `$x: …` is a binding in the environment, never a CSS declaration.
                    Some(var) => self.bind(var, &value),
                    None => {
                        let owned = self.decl_value(&name, &value);
                        if let Some(top) = self.scopes.last_mut() {
                            top.decls.push((name, owned));
                        }
                    }
                }
                Vec::new()
            }
            Event::Close => match self.scopes.pop() {
                Some(s) if !s.decls.is_empty() && !s.selector.is_empty() => {
                    vec![StyleRule { selector: s.selector, decls: s.decls, order: s.order }]
                }
                _ => Vec::new(),
            },
        }
    }

    /// The declaration's owned value. A custom property (`--x: …`) and a value carrying a CSS
    /// native function (`var()`, `calc()`, `url()`, …) hold **raw CSS text** that Sass must not
    /// interpret; the first is stored verbatim, the others bail inside `evaluated`. Any other
    /// value becomes a typed [`Value`] when — and only when — the run is unambiguous; a run that
    /// does not parse, or whose evaluation errors, keeps its verbatim text for now.
    fn decl_value(&self, prop: &str, value: &[Tok]) -> Value {
        match prop {
            p if p.starts_with("--") => Value::from_text(render_raw(value)),
            _ => self
                .evaluated(value)
                .unwrap_or_else(|| Value::from_text(render(value))),
        }
    }

    /// Bind `$name: value`, evaluating the right-hand side against the current environment.
    fn bind(&mut self, name: &str, value: &[Tok]) {
        let bound = self
            .evaluated(value)
            .unwrap_or_else(|| Value::from_text(render(value)));
        self.env.bind(name, bound);
    }

    /// Fold a token run into a typed value. A lone number is **not** an expression yet — only
    /// an operation (or an interpolation) earns evaluation, so `w: 10px` keeps its exact source
    /// digits instead of being normalised to `10px` from a float.
    fn evaluated(&self, toks: &[Tok]) -> Option<Value> {
        let arithmetic = expr::from_tokens(toks).and_then(|expr| match expr {
            expr::Expr::Num(_, _) => None,
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

/// The exec stage: `Event` stream → `StyleRule` stream (forward, owned accumulator).
pub fn exec(events: SharedBoxedObservable<'static, Event, Infallible>) -> SharedBoxedObservable<'static, StyleRule, Infallible> {
    events
        .scan_map(Exec::default(), |ex, ev| ex.advance(ev))
        .flat_map(Shared::from_iter)
        .box_it()
}

/// Serialise one rule to an expanded CSS block. A declaration value is serialised through
/// [`Value::to_css`] — the one place a value becomes text.
pub fn render_rule(rule: &StyleRule) -> String {
    let body = rule
        .decls
        .iter()
        .map(|(prop, value)| format!("  {prop}: {};\n", value.to_css()))
        .collect::<String>();
    format!("{} {{\n{}}}\n", rule.selector, body)
}
