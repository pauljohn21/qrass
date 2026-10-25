//! Exec (push): consume the structural `Event` stream forward with an **owned accumulator**
//! whose nesting state is a *data* stack `scopes: Vec<Scope>` (moved through `scan_map`, not a
//! `&mut self` object driving a tree walk, and not a call-stack recursion). A rule is **pushed
//! out** when its `Close` fires; document order is recovered by an owned `order` key (assigned
//! at `Open`, pre-order), not by re-walking anything.

use std::convert::Infallible;

use rxrust::prelude::*;

use crate::lexer::{Event, Tok, render, render_raw};
use crate::value::Value;
use crate::expr;

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

/// Owned `scan_map` accumulator: the open-scope stack is plain data living here.
#[derive(Default)]
pub struct Exec {
    scopes: Vec<Scope>,
    next: u32,
}

impl Exec {
    fn advance(&mut self, ev: Event) -> Vec<StyleRule> {
        match ev {
            Event::Open(head) => {
                let parent = self.scopes.last().map(|s| s.selector.clone()).unwrap_or_default();
                let selector = combine(&parent, &render(&head));
                let order = self.next;
                self.next += 1;
                self.scopes.push(Scope { selector, decls: Vec::new(), order });
                Vec::new()
            }
            Event::Decl { prop, value } => {
                if let Some(top) = self.scopes.last_mut() {
                    let name = render(&prop);
                    let value = decl_value(&name, &value);
                    top.decls.push((name, value));
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
}

/// Build the declaration's owned value. A custom property (`--x: …`) carries **raw CSS
/// text**, so a `//` comment inside its value belongs to the value. Any other value becomes a
/// typed [`Value`] when — and only when — the run is unambiguous arithmetic; a run that does
/// not parse, or whose evaluation errors (incompatible units), keeps its verbatim text for
/// now, which is what protects slash-separated shorthands and CSS-native functions.
fn decl_value(prop: &str, value: &[Tok]) -> Value {
    match prop {
        p if p.starts_with("--") => Value::from_text(render_raw(value)),
        _ => expr::from_tokens(value)
            .and_then(|expr| expr.eval().ok())
            .unwrap_or_else(|| Value::from_text(render(value))),
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
