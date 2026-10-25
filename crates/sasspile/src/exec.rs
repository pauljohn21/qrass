//! Exec (push): consume the structural `Event` stream forward with an **owned accumulator**
//! whose nesting state is a *data* stack `scopes: Vec<Scope>` (moved through `scan_map`, not a
//! `&mut self` object driving a tree walk, and not a call-stack recursion). A rule is **pushed
//! out** when its `Close` fires; document order is recovered by an owned `order` key (assigned
//! at `Open`, pre-order), not by re-walking anything.

use std::convert::Infallible;

use rxrust::prelude::*;

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
                    let text = value_text(&name, &value);
                    top.decls.push((name, Value::from_text(text)));
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

/// Declaration value text: a custom property (`--x: …`) carries **raw CSS text**, so a `//`
/// comment inside its value belongs to the value; any other declaration value drops silent
/// comments, which Sass strips before output.
fn value_text(prop: &str, value: &[Tok]) -> String {
    match prop {
        p if p.starts_with("--") => render_raw(value),
        _ => render(value),
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
