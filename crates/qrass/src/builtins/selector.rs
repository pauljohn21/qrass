//! `selector.*` builtins (task 8.3): selector parsing and nesting. Selectors are
//! carried as unquoted strings (this compiler keeps `&` resolution in eval for
//! rules; these functions operate purely on the textual form). `parse` splits a
//! comma-separated selector list; `nest` resolves `&` against a parent; `simple-
//! selectors` breaks a compound selector into its constituents.

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.ns("selector", "parse", 1, Some(1), parse);
    r.ns("selector", "nest", 1, None, nest);
    r.ns("selector", "simple-selectors", 1, Some(1), simple_selectors);
    r.def("selector-nest", 1, None, nest);
    r.def("selector-append", 2, None, append);
    r.def("selector-parse", 1, Some(1), parse);
}

fn text(v: &SassValue) -> String {
    match v {
        SassValue::List { items, .. } => items
            .iter()
            .map(|i| match i {
                SassValue::List { items, .. } => {
                    items.iter().map(text).collect::<Vec<_>>().join(" ")
                }
                other => text(other),
            })
            .collect::<Vec<_>>()
            .join(", "),
        // Selectors ignore the quoting of their source string: `.a` and `".a"`
        // both denote the compound selector `.a`.
        SassValue::Str { text, .. } => text.clone(),
        other => other.to_css(),
    }
}

fn comma(items: Vec<SassValue>) -> SassValue {
    SassValue::List {
        items,
        sep: ListSep::Comma,
    }
}

fn unquoted(s: String) -> SassValue {
    SassValue::Str {
        text: s,
        quoted: false,
    }
}

/// Split a selector list on top-level commas (brackets/parens protected).
pub(crate) fn split_commas(sel: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in sel.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth <= 0 => {
                parts.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(ch),
        }
    }
    let last = cur.trim().to_string();
    if !last.is_empty() || parts.is_empty() {
        parts.push(last);
    }
    parts
}

/// Insert `parent` into `child` at each `&`, or append when `&` is absent.
fn combine(parent: &str, child: &str) -> String {
    if child.contains('&') {
        child.replace('&', parent)
    } else {
        format!("{parent} {child}")
    }
}

fn parse(c: &super::Call) -> Or<SassValue> {
    let sel = text(&c.positional[0]);
    Ok(comma(
        split_commas(&sel).into_iter().map(unquoted).collect(),
    ))
}

/// `selector.nest($selectors...)`: fold left, each later selector nested into the
/// accumulated parents via `&`.
fn nest(c: &super::Call) -> Or<SassValue> {
    let mut acc: Vec<String> = split_commas(&text(&c.positional[0]));
    for arg in &c.positional[1..] {
        let children = split_commas(&text(arg));
        let mut next = Vec::new();
        for parent in &acc {
            for child in &children {
                next.push(combine(parent, child));
            }
        }
        acc = next;
    }
    Ok(comma(acc.into_iter().map(unquoted).collect()))
}

/// `selector-append($selectors...)`: concatenate compound selectors directly,
/// substituting `&` in the last with the joined prefix.
fn append(c: &super::Call) -> Or<SassValue> {
    let mut out = String::new();
    for arg in c.positional {
        let s = text(arg);
        if s.contains('&') {
            out = s.replace('&', out.trim());
        } else {
            out.push_str(s.trim());
        }
    }
    Ok(comma(vec![unquoted(out)]))
}

/// Break one compound selector into its simple selectors (`.a`, `#b`, `:hover`,
/// `[attr]`, `%ph`, element names). Combinators/whitespace are not part of a
/// compound selector, so each token runs from a sigil (or element start) up to
/// the next sigil.
fn simple_selectors(c: &super::Call) -> Or<SassValue> {
    let sel = text(&c.positional[0]);
    let chars: Vec<char> = sel.trim().chars().collect();
    let is_name = |ch: char| ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '\\';
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        match chars[i] {
            '.' | '#' | '%' | '&' => {
                i += 1;
                while i < chars.len() && is_name(chars[i]) {
                    i += 1;
                }
            }
            ':' => {
                i += 1;
                if chars.get(i) == Some(&':') {
                    i += 1;
                }
                while i < chars.len() && is_name(chars[i]) {
                    i += 1;
                }
                // functional pseudo-class: consume the parenthesized argument.
                if chars.get(i) == Some(&'(') {
                    while i < chars.len() && chars[i] != ')' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '[' => {
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                i += 1;
            }
            _ => {
                while i < chars.len() && !matches!(chars[i], '.' | '#' | ':' | '[' | '%') {
                    i += 1;
                }
            }
        }
        if i > start {
            out.push(chars[start..i].iter().collect());
        } else {
            i += 1; // guarantee progress on stray separators
        }
    }
    Ok(comma(out.into_iter().map(unquoted).collect()))
}
