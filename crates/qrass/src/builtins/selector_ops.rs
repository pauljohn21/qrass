//! Selector argument coercion (`sel_source`/`sel_arg`) plus the inspection and
//! combination builtins (`is-superselector`, `unify`, `extend`, `replace`), split
//! out of `builtins/selector.rs` for the 500-line budget (round 62). The two
//! coercion helpers are the shared `$selector` path for the rest of the
//! namespace too, hence `pub(super)`.

use super::Or;
use super::selector::parsed_sel_list;
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::selector_ast::SelectorList;
use crate::selector_parse::{self, SelParseError};
use crate::selector_super;
use crate::selector_unify_complex;
use crate::selector_extend;
use crate::value::SassValue;

/// Render a selector argument value to its source text, or `None` when it holds
/// any non-string / non-list leaf (a number, color, …). A comma list joins its
/// members with `, `; a space list (a compound) joins with a single space; a
/// slash list with ` / `. This mirrors the grammar sass accepts for selector
/// arguments: "a string, a list of strings, or a list of lists of strings".
pub(super) fn sel_source(v: &SassValue) -> Option<String> {
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
pub(super) fn sel_arg(v: &SassValue, name: &str) -> Or<SelectorList> {
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
pub(super) fn is_superselector(c: &super::Call) -> Or<SassValue> {
    let sup = sel_arg(&c.positional[0], "super")?;
    let sub = sel_arg(&c.positional[1], "sub")?;
    Ok(SassValue::Bool(selector_super::list_is_super(&sup, &sub)))
}

/// `selector.unify($selector1, $selector2)`: the selector matching only elements
/// matched by both inputs, or `null` when no element can match.
pub(super) fn unify(c: &super::Call) -> Or<SassValue> {
    let first = sel_arg(&c.positional[0], "selector1")?;
    let second = sel_arg(&c.positional[1], "selector2")?;
    Ok(match selector_unify_complex::unify_lists(&[first, second]) {
        Some(list) => parsed_sel_list(&list),
        None => SassValue::Null,
    })
}

/// `selector.extend($selectors, $extendee, $extender)`: every selector in
/// `$selectors` with each occurrence of `$extendee` replaced by `$extender`.
pub(super) fn extend(c: &super::Call) -> Or<SassValue> {
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

/// `selector.replace($selectors, $original, $replacement)`: like `extend`, but
/// non-additive — each selector that actually matches `$original` has that match
/// replaced by `$replacement` (the untouched form is dropped), while a selector
/// with no match is left as-is.
pub(super) fn replace(c: &super::Call) -> Or<SassValue> {
    let selectors = sel_arg(&c.positional[0], "selector")?;
    let original = sel_arg(&c.positional[1], "original")?;
    let replacement = sel_arg(&c.positional[2], "replacement")?;
    // `$original` must be compound-only; a complex selector has no single slot to
    // splice the replacement into (same guard, wording and reason as `extend`).
    if let Some(complex) = original.complexes.iter().find(|c| c.components.len() > 1) {
        return Err(SassError::new(
            ErrorKind::Builtins,
            format!("Can't extend complex selector {complex}."),
        ));
    }
    Ok(parsed_sel_list(&selector_extend::replace_list(
        &selectors,
        &original,
        &replacement,
    )))
}
