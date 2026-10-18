//! `list.*` builtins (task 8.3): append / join / indexing / separator on Sass
//! lists. A list is a [`SassValue::List`]; a map iterates as its key/value pairs
//! and a single non-list value is treated as a one-element list, matching sass.
//! Results are freshly built (values move by value; no input is mutated).

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.nsp("list", "append", 2, Some(3), &["list", "val", "separator"], append);
    r.nsp(
        "list",
        "join",
        2,
        Some(4),
        &["list1", "list2", "separator", "bracketed"],
        join,
    );
    r.nsp("list", "length", 1, Some(1), &["list"], length);
    r.nsp("list", "nth", 2, Some(2), &["list", "n"], nth);
    r.nsp("list", "set-nth", 3, Some(3), &["list", "n", "value"], set_nth);
    r.nsp("list", "index", 2, Some(2), &["list", "value"], index);
    r.nsp("list", "separator", 1, Some(1), &["list"], separator);
    r.nsp("list", "is-bracketed", 1, Some(1), &["list"], is_bracketed);
    r.ns("list", "zip", 0, None, zip);
    r.ns("list", "slash", 0, None, slash);
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
                bracketed: false,
                keywords: None,
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

/// Is this argument a bracketed (`[…]`) list?
fn bracketed_of(v: &SassValue) -> bool {
    matches!(v, SassValue::List { bracketed: true, .. })
}

fn list(items: Vec<SassValue>, sep: ListSep) -> SassValue {
    SassValue::List {
        items,
        sep,
        bracketed: false,
        keywords: None,
    }
}

fn list_br(items: Vec<SassValue>, sep: ListSep, bracketed: bool) -> SassValue {
    SassValue::List {
        items,
        sep,
        bracketed,
        keywords: None,
    }
}

fn append(c: &super::Call) -> Or<SassValue> {
    let mut items = items_of(&c.positional[0]);
    let sep = effective_sep(c, sep_of(&c.positional[0]));
    items.push(c.positional[1].clone());
    // `append` preserves the first list's bracketedness.
    Ok(list_br(items, sep, bracketed_of(&c.positional[0])))
}

fn join(c: &super::Call) -> Or<SassValue> {
    let l1 = &c.positional[0];
    let l2 = &c.positional[1];
    // dart validates the explicit `$separator`: it must be a string, and one of
    // the four recognised keywords. An unquoted bareword is a (string) value, so
    // `e` fails the keyword set rather than the type check. A `Null` here is the
    // `fold_named` interior-gap filler (only `$bracketed` was passed) or an
    // explicit null — dart's "auto", so it is not validated.
    if let Some(sep) = c.positional.get(2)
        && !matches!(sep, SassValue::Null)
    {
        let SassValue::Str { text, .. } = sep else {
            return Err(SassError::new(
                ErrorKind::Builtins,
                format!("$separator: {} is not a string.", sep.to_css()),
            ));
        };
        if !matches!(text.as_str(), "space" | "comma" | "slash" | "auto") {
            return Err(SassError::new(
                ErrorKind::Builtins,
                "$separator: Must be \"space\", \"comma\", \"slash\", or \"auto\".",
            ));
        }
    }
    let mut items = items_of(l1);
    // Separator: an explicit `$separator` string wins; otherwise `list1`'s, except
    // that a single-element `list1` with an undetermined (space) separator defers
    // to `list2`'s determined separator — so `join([1], (2,))` is `1, 2`.
    let mut sep = sep_of(l1);
    if !matches!(c.positional.get(2), Some(SassValue::Str { .. }))
        && items.len() <= 1
        && sep == ListSep::Space
    {
        let s2 = sep_of(l2);
        if s2 != ListSep::Space {
            sep = s2;
        }
    } else {
        sep = effective_sep(c, sep);
    }
    items.extend(items_of(l2));
    // golden: the result is bracketed iff `$bracketed` (default true) holds *and*
    // `list1` was bracketed. `list2`'s bracketedness never carries over.
    let want = match c.positional.get(3) {
        Some(SassValue::Bool(b)) => *b,
        _ => true,
    };
    Ok(list_br(items, sep, want && bracketed_of(l1)))
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

/// Validate + resolve the `$n` index argument shared by `nth`/`set_nth`: a
/// non-number, `0`, or an out-of-range index each raise dart's `$n:`-prefixed
/// message. Returns the resolved in-range (0-based) position otherwise.
fn list_index(c: &super::Call, len: usize) -> Or<usize> {
    let arg = &c.positional[1];
    let SassValue::Number { value: n, .. } = arg else {
        return Err(SassError::new(
            ErrorKind::Builtins,
            format!("$n: {} is not a number.", arg.to_css()),
        ));
    };
    if *n == 0.0 {
        return Err(SassError::new(
            ErrorKind::Builtins,
            "$n: List index may not be 0.",
        ));
    }
    resolve_index(len, *n).ok_or_else(|| {
        SassError::new(
            ErrorKind::Builtins,
            format!(
                "$n: Invalid index {} for a list with {} elements.",
                arg.to_css(),
                len
            ),
        )
    })
}

fn nth(c: &super::Call) -> Or<SassValue> {
    let items = items_of(&c.positional[0]);
    let i = list_index(c, items.len())?;
    Ok(items[i].clone())
}

fn set_nth(c: &super::Call) -> Or<SassValue> {
    let mut items = items_of(&c.positional[0]);
    let sep = sep_of(&c.positional[0]);
    let bracketed = bracketed_of(&c.positional[0]);
    let i = list_index(c, items.len())?;
    items[i] = c.positional[2].clone();
    Ok(list_br(items, sep, bracketed))
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
    Ok(SassValue::Bool(bracketed_of(&c.positional[0])))
}

/// `list.slash($elements...)`: a slash-separated list of its arguments.
fn slash(c: &super::Call) -> Or<SassValue> {
    Ok(SassValue::List {
        items: c.positional.to_vec(),
        sep: ListSep::Slash,
        bracketed: false,
        keywords: None,
    })
}

/// `list.zip($lists...)`: element-wise grouping into an outer comma list whose
/// inner element lists are **space**-separated (dart renders `zip(1 2 3 4, c d)`
/// as `1 c, 2 d`), regardless of the inputs' own separators; they are never
/// bracketed. `zip()` with no arguments yields the empty comma list.
fn zip(c: &super::Call) -> Or<SassValue> {
    let lists: Vec<Vec<SassValue>> = c.positional.iter().map(items_of).collect();
    let shortest = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let out = (0..shortest)
        .map(|i| SassValue::List {
            items: lists.iter().map(|l| l[i].clone()).collect(),
            sep: ListSep::Space,
            bracketed: false,
            keywords: None,
        })
        .collect();
    Ok(list(out, ListSep::Comma))
}

fn str_value(s: &str) -> SassValue {
    SassValue::Str {
        text: s.to_string(),
        quoted: false,
    }
}
