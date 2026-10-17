//! `selector.*` builtins (task 8.3): selector parsing and nesting. Selectors are
//! carried as unquoted strings (this compiler keeps `&` resolution in eval for
//! rules; these functions operate purely on the textual form). `parse` splits a
//! comma-separated selector list; `nest` resolves `&` against a parent; `simple-
//! selectors` breaks a compound selector into its constituents.

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::selector_ast::{Combinator, Complex, SelectorList};
use crate::selector_parse::{self, SelParseError};
use crate::selector_super;
use crate::selector_unify_complex;
use crate::selector_extend;
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.nsp("selector", "parse", 1, Some(1), &["selector"], parse);
    r.nsp(
        "selector",
        "is-superselector",
        2,
        Some(2),
        &["super", "sub"],
        is_superselector,
    );
    r.nsp(
        "selector",
        "unify",
        2,
        Some(2),
        &["selector1", "selector2"],
        unify,
    );
    r.nsp(
        "selector",
        "extend",
        3,
        Some(3),
        &["selector", "extendee", "extender"],
        extend,
    );
    // `nest`/`append` take a rest parameter (`$selectors...`); the goldens forbid
    // naming a rest argument, so they stay positional-only.
    r.ns("selector", "nest", 1, None, nest);
    r.nsp("selector", "simple-selectors", 1, Some(1), &["selector"], simple_selectors);
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
        bracketed: false,
    }
}

fn unquoted(s: String) -> SassValue {
    SassValue::Str {
        text: s,
        quoted: false,
    }
}

/// Build the **parsed selector-list value** sass's `selector.*` functions return:
/// a comma list whose elements are the *space lists* of each complex selector —
/// every compound an unquoted string, explicit combinators (`>`, `+`, `~`)
/// interleaved as their own elements, descendant expressed by plain adjacency.
/// This shape round-trips through `==` against a decomposed literal such as
/// `("c" "d.g", "e" "f.g")`, and serializes back to the same CSS text as the
/// flat string form (`c d.g, e f.g`), so string-output cases are unaffected.
fn parsed_sel_list(list: &SelectorList) -> SassValue {
    comma(list.complexes.iter().map(parsed_complex).collect())
}

/// The list element a combinator edge contributes: a descendant is the implicit
/// space between items (empty → no element), while `>`/`+`/`~` become their own.
fn comb_symbol(combinator: Option<Combinator>) -> &'static str {
    match combinator {
        Some(Combinator::Child) => ">",
        Some(Combinator::NextSibling) => "+",
        Some(Combinator::Sibling) => "~",
        _ => "",
    }
}

fn parsed_complex(c: &Complex) -> SassValue {
    let mut items: Vec<SassValue> = Vec::new();
    for comp in &c.components {
        // A combinator edge sits before its compound; a leading one on the first
        // component (`> .c`) is retained front-pinned, exactly as `Display` does.
        let primary = comb_symbol(comp.combinator);
        if !primary.is_empty() {
            items.push(unquoted(primary.to_string()));
        }
        // A bogus doubled-combinator chain (`.c ~ ~ .d`) carries the extra edges.
        for extra in &comp.extras {
            let sym = comb_symbol(Some(*extra));
            if !sym.is_empty() {
                items.push(unquoted(sym.to_string()));
            }
        }
        items.push(unquoted(comp.compound.to_string()));
    }
    // A carried trailing combinator (e.g. an extend `carry` yielding `.d +`).
    for comb in &c.trailing {
        let sym = comb_symbol(Some(*comb));
        if !sym.is_empty() {
            items.push(unquoted(sym.to_string()));
        }
    }
    SassValue::List {
        items,
        sep: ListSep::Space,
        bracketed: false,
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

/// Render a selector argument value to its source text, or `None` when it holds
/// any non-string / non-list leaf (a number, color, …). A comma list joins its
/// members with `, `; a space list (a compound) joins with a single space; a
/// slash list with ` / `. This mirrors the grammar sass accepts for selector
/// arguments: "a string, a list of strings, or a list of lists of strings".
fn sel_source(v: &SassValue) -> Option<String> {
    match v {
        // Selectors ignore their source quoting: `.a` and `".a"` are the same.
        SassValue::Str { text, .. } => Some(text.clone()),
        SassValue::List { items, sep, .. } => {
            let join = match sep {
                ListSep::Comma => ", ",
                ListSep::Slash => " / ",
                ListSep::Space => " ",
            };
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(sel_source(item)?);
            }
            Some(parts.join(join))
        }
        _ => None,
    }
}

/// Coerce one argument into a parsed [`SelectorList`], raising the exact
/// sass-shaped error prefixed with `$<name>:` for an invalid type, a truncated
/// input, or a stray parent selector.
fn sel_arg(v: &SassValue, name: &str) -> Or<SelectorList> {
    let src = sel_source(v).ok_or_else(|| {
        SassError::new(
            ErrorKind::Builtins,
            format!(
                "${name}: {} is not a valid selector: it must be a string,\na list of strings, or a list of lists of strings.",
                v.to_css()
            ),
        )
    })?;
    selector_parse::parse_list(&src).map_err(|e| {
        let reason = match e {
            SelParseError::Truncated => "expected more input.",
            SelParseError::Parent => "Parent selectors aren't allowed here.",
        };
        SassError::new(ErrorKind::Builtins, format!("${name}: {reason}"))
    })
}

/// `selector.is-superselector($super, $sub)`: does `$super` subsume every
/// selector in `$sub`?
fn is_superselector(c: &super::Call) -> Or<SassValue> {
    let sup = sel_arg(&c.positional[0], "super")?;
    let sub = sel_arg(&c.positional[1], "sub")?;
    Ok(SassValue::Bool(selector_super::list_is_super(&sup, &sub)))
}

/// `selector.unify($selector1, $selector2)`: the selector matching only elements
/// matched by both inputs, or `null` when no element can match.
fn unify(c: &super::Call) -> Or<SassValue> {
    let first = sel_arg(&c.positional[0], "selector1")?;
    let second = sel_arg(&c.positional[1], "selector2")?;
    Ok(match selector_unify_complex::unify_lists(&[first, second]) {
        Some(list) => parsed_sel_list(&list),
        None => SassValue::Null,
    })
}

/// `selector.extend($selectors, $extendee, $extender)`: every selector in
/// `$selectors` with each occurrence of `$extendee` replaced by `$extender`.
fn extend(c: &super::Call) -> Or<SassValue> {
    let selectors = sel_arg(&c.positional[0], "selector")?;
    let extendee = sel_arg(&c.positional[1], "extendee")?;
    let extender = sel_arg(&c.positional[2], "extender")?;
    // `$extendee` must be compound-only; a complex selector has no single slot to
    // splice the extender into.
    if let Some(complex) = extendee.complexes.iter().find(|c| c.components.len() > 1) {
        return Err(SassError::new(
            ErrorKind::Builtins,
            format!("Can't extend complex selector {complex}."),
        ));
    }
    Ok(parsed_sel_list(&selector_extend::extend_list(
        &selectors,
        &extendee,
        &extender,
    )))
}
