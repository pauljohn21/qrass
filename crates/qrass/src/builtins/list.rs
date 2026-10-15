//! `list.*` builtins (task 8.3): append / join / indexing / separator on Sass
//! lists. A list is a [`SassValue::List`]; a map iterates as its key/value pairs
//! and a single non-list value is treated as a one-element list, matching sass.
//! Results are freshly built (values move by value; no input is mutated).

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.ns("list", "append", 2, Some(3), append);
    r.ns("list", "join", 2, Some(3), join);
    r.ns("list", "length", 1, Some(1), length);
    r.ns("list", "nth", 2, Some(2), nth);
    r.ns("list", "set-nth", 3, Some(3), set_nth);
    r.ns("list", "index", 2, Some(2), index);
    r.ns("list", "separator", 1, Some(1), separator);
    r.ns("list", "is-bracketed", 1, Some(1), is_bracketed);
    r.ns("list", "zip", 1, None, zip);
    // Legacy global spellings (bare names used across many stylesheets).
    r.def("append", 2, Some(3), append);
    r.def("join", 2, Some(3), join);
    r.def("length", 1, Some(1), length);
    r.def("nth", 2, Some(2), nth);
    r.def("set-nth", 3, Some(3), set_nth);
    r.def("index", 2, Some(2), index);
}

/// Flatten an argument into its item vector (map → pairs, scalar → one item).
fn items_of(v: &SassValue) -> Vec<SassValue> {
    match v {
        SassValue::List { items, .. } => items.clone(),
        SassValue::Map { entries } => entries
            .iter()
            .map(|(k, val)| SassValue::List {
                items: vec![k.clone(), val.clone()],
                sep: ListSep::Space,
            })
            .collect(),
        SassValue::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

/// Separator of an argument list (comma if a map, else the list's own).
fn sep_of(v: &SassValue) -> ListSep {
    match v {
        SassValue::List { sep, .. } => *sep,
        SassValue::Map { .. } => ListSep::Comma,
        _ => ListSep::Space,
    }
}

/// Pick the effective separator for append/join: an explicit `$separator`
/// string argument wins; otherwise fall back to the first list's separator,
/// promoting a lone comma list to comma.
fn effective_sep(c: &super::Call, primary: ListSep) -> ListSep {
    if let Some(SassValue::Str { text, .. }) = c.positional.get(2) {
        match text.as_str() {
            "comma" => return ListSep::Comma,
            "slash" => return ListSep::Slash,
            "space" => return ListSep::Space,
            _ => {}
        }
    }
    primary
}

fn list(items: Vec<SassValue>, sep: ListSep) -> SassValue {
    SassValue::List { items, sep }
}

fn append(c: &super::Call) -> Or<SassValue> {
    let mut items = items_of(&c.positional[0]);
    let sep = effective_sep(c, sep_of(&c.positional[0]));
    items.push(c.positional[1].clone());
    Ok(list(items, sep))
}

fn join(c: &super::Call) -> Or<SassValue> {
    let mut items = items_of(&c.positional[0]);
    let sep = effective_sep(c, sep_of(&c.positional[0]));
    items.extend(items_of(&c.positional[1]));
    Ok(list(items, sep))
}

fn length(c: &super::Call) -> Or<SassValue> {
    Ok(SassValue::Number {
        value: items_of(&c.positional[0]).len() as f64,
        unit: None,
    })
}

/// Resolve a 1-based index (negatives count from the end) to a `0`-based slot.
fn resolve_index(len: usize, raw: f64) -> Option<usize> {
    let n = raw as i64;
    let idx = if n > 0 {
        n
    } else if n < 0 {
        len as i64 + n + 1
    } else {
        return None;
    };
    if idx >= 1 && idx <= len as i64 {
        Some(idx as usize - 1)
    } else {
        None
    }
}

fn nth(c: &super::Call) -> Or<SassValue> {
    let items = items_of(&c.positional[0]);
    let (n, _) = num(c, 1)?;
    resolve_index(items.len(), n)
        .and_then(|i| items.get(i).cloned())
        .ok_or_else(|| {
            SassError::new(
                ErrorKind::Builtins,
                format!(
                    "{n} is not a valid index in a list of {} items",
                    items.len()
                ),
            )
        })
}

fn set_nth(c: &super::Call) -> Or<SassValue> {
    let mut items = items_of(&c.positional[0]);
    let sep = sep_of(&c.positional[0]);
    let (n, _) = num(c, 1)?;
    if let Some(i) = resolve_index(items.len(), n) {
        items[i] = c.positional[2].clone();
    }
    Ok(list(items, sep))
}

fn index(c: &super::Call) -> Or<SassValue> {
    let items = items_of(&c.positional[0]);
    let needle = &c.positional[1];
    Ok(items
        .iter()
        .position(|it| it == needle)
        .map(|p| SassValue::Number {
            value: p as f64 + 1.0,
            unit: None,
        })
        .unwrap_or(SassValue::Null))
}

fn separator(c: &super::Call) -> Or<SassValue> {
    Ok(str_value(match sep_of(&c.positional[0]) {
        ListSep::Comma => "comma",
        ListSep::Slash => "slash",
        ListSep::Space => "space",
    }))
}

fn is_bracketed(c: &super::Call) -> Or<SassValue> {
    let _ = c;
    // Bracketed lists are not modeled distinctly in the value type → always false.
    Ok(SassValue::Bool(false))
}

/// `list.zip($lists...)`: element-wise grouping into an outer comma list.
fn zip(c: &super::Call) -> Or<SassValue> {
    let lists: Vec<Vec<SassValue>> = c.positional.iter().map(items_of).collect();
    let shortest = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let out = (0..shortest)
        .map(|i| SassValue::List {
            items: lists.iter().map(|l| l[i].clone()).collect(),
            sep: ListSep::Comma,
        })
        .collect();
    Ok(list(out, ListSep::Comma))
}

fn num(c: &super::Call, i: usize) -> Or<(f64, Option<String>)> {
    match c.positional.get(i) {
        Some(SassValue::Number { value, unit }) => Ok((*value, unit.clone())),
        Some(other) => Err(super::type_err("list", "index", "a number", other)),
        None => Err(SassError::new(ErrorKind::Builtins, "expected a number")),
    }
}

fn str_value(s: &str) -> SassValue {
    SassValue::Str {
        text: s.to_string(),
        quoted: false,
    }
}
