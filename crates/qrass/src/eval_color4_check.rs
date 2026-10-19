//! Structural `$channels` / `$description` argument guards for the modern
//! Color 4 literal parser (sibling of [`crate::eval_color4`], split to keep
//! both files within the 500-line budget).
//!
//! The rule these encode (rounds 60/61): a guard may only fire on an argument
//! shape that can *never* be a valid literal or a meaningful plain-CSS
//! passthrough, and an argument is evaluated solely inside an already-rejected
//! branch to render dart's message. The hot path — well-formed literals and
//! the symbolic (`var()`) passthrough family — is never touched (the W4b
//! lesson: a pre-guard that evaluated every list regressed 273 goldens).

use crate::ast_expr::{Arg, Expr, FunctionCall, ListSep};
use crate::error::{ErrorKind, SassError};
use crate::eval::EvalState;
use crate::value::SassValue;

use super::Ev;

/// A value that must keep falling through to plain-CSS passthrough rather than
/// becoming a Color 4 argument error: a `var()`/`calc()`/`min()`/`env()`/… symbolic
/// construct (the round-28 rgb() lesson — an unresolvable channel is CSS, not a
/// Sass color argument). `none` and concrete numbers/strings are *not* symbolic.
pub(crate) fn is_symbolic(v: &SassValue) -> bool {
    match v {
        SassValue::Str { text, .. } => {
            ["var(", "calc(", "min(", "max(", "clamp(", "env(", "url(", "attr("]
                .iter()
                .any(|f| text.contains(f))
        }
        _ => false,
    }
}

/// A dart-shaped argument error for a modern color literal (arity, list shape,
/// slash element count or channel count).
pub(crate) fn shape_err(msg: String) -> SassError {
    SassError::new(ErrorKind::Builtins, msg)
}

/// Whether a bare-space call is relative color syntax — `lab(from …)`, the
/// case-mutated `lab(From …)`, or the `list.slash(from …, α)` wrapper. None of
/// these are channel literals; the goldens keep them as verbatim CSS, so the
/// strict `$channels` guards must not claim them.
pub(crate) fn relative_anchor(arg: Option<&Arg>) -> bool {
    let from_word = |items: &[Expr]| {
        items.first().is_some_and(|f| {
            matches!(f, Expr::Literal(crate::ast_expr::Literal::Ident(w)) if w.eq_ignore_ascii_case("from"))
        })
    };
    match arg {
        Some(Arg::Positional(Expr::List { items, .. })) => from_word(items),
        Some(Arg::Positional(Expr::FunctionCall(inner)))
            if inner.name == "slash" || inner.name == "list.slash" =>
        {
            matches!(
                inner.args.first(),
                Some(Arg::Positional(Expr::List { items, .. })) if from_word(items)
            )
        }
        _ => false,
    }
}

/// Coerce an evaluated component into a channel's canonical scale. The outer
/// `None` marks a value that can never be a channel (the caller then lets the
/// literal fall through to plain-CSS passthrough); the inner `None` is a
/// powerless `none`.
pub(crate) fn channel_value(v: &SassValue, ch: &crate::color4::Channel) -> Option<Option<f64>> {
    use crate::color4::Unit;
    match v {
        SassValue::Str {
            text,
            quoted: false,
        } if text == "none" => Some(None),
        SassValue::Number { value, unit } => {
            let u = unit.as_deref();
            let out = match ch.unit {
                Unit::Pct => match u {
                    None | Some("%") => *value,
                    Some(_) => return None,
                },
                Unit::Deg => match u {
                    None | Some("deg") => *value,
                    Some("rad") => value.to_degrees(),
                    Some("grad") => *value * 0.9,
                    Some("turn") => *value * 360.0,
                    Some(_) => return None,
                },
                Unit::Num => match u {
                    None => *value,
                    Some("%") => *value / 100.0,
                    Some(_) => return None,
                },
            };
            Some(Some(out))
        }
        _ => None,
    }
}

/// The bare perceptual spaces (`lab`/`lch`/`oklab`/`oklch`) take exactly one
/// `$channels` argument; reject the malformed shapes the goldens name. `Ok(())`
/// means "no structural complaint — continue the normal parse".
pub(crate) fn guard_bare4(st: &mut EvalState, call: &FunctionCall) -> Ev<()> {
    match call.args.len() {
        0 => return Err(shape_err("Missing argument $channels.".into())),
        n if n > 1 => {
            return Err(shape_err(format!(
                "Only 1 argument allowed, but {n} were passed."
            )));
        }
        _ => {}
    }
    if let Some(Arg::Positional(e)) = call.args.first() {
        match e {
            Expr::List { items, .. } if items.is_empty() => {
                return Err(shape_err(
                    "$channels: Color component list may not be empty.".into(),
                ));
            }
            Expr::List { bracketed: true, .. } => {
                let v = st.eval_expr(e)?;
                return Err(shape_err(format!(
                    "$channels: Expected an unbracketed list, was {}",
                    v.to_css()
                )));
            }
            Expr::List { sep: ListSep::Comma, .. } => {
                let v = st.eval_expr(e)?;
                return Err(shape_err(format!(
                    "$channels: Expected a space- or slash-separated list, was ({})",
                    v.to_css()
                )));
            }
            Expr::FunctionCall(inner) if inner.name == "slash" || inner.name == "list.slash" => {
                let n = inner.args.len();
                if n != 2 {
                    return Err(shape_err(format!(
                        "$channels: Only 2 slash-separated elements allowed, but {n} {} passed.",
                        if n == 1 { "was" } else { "were" }
                    )));
                }
            }
            Expr::List {
                sep: ListSep::Space,
                items,
                ..
            } if items.len() < 3 => {
                let vals: Vec<SassValue> = items
                    .iter()
                    .map(|i| st.eval_expr(i))
                    .collect::<Ev<_>>()?;
                if vals.iter().all(|v| !is_symbolic(v)) {
                    let shown = vals
                        .iter()
                        .map(|v| v.to_css())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let space = crate::color4::lookup(&call.name).expect("bare4 checked");
                    return Err(shape_err(format!(
                        "$channels: The {} color space has 3 channels but ({shown}) has {}.",
                        space.name,
                        vals.len()
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The `color(…)` wrapper takes one `$description` argument and dart rejects
/// malformed shapes the same way, with the `$description:` prefix (round-60
/// color/error goldens). Relative syntax (`color(from …)`) is exempt at the
/// call site; a plain-CSS `color(…)` filter is claimed by the passthrough
/// layer before this code ever runs. `Ok(())` means "keep parsing".
pub(crate) fn guard_wrapper(st: &mut EvalState, call: &FunctionCall) -> Ev<()> {
    if call.args.is_empty() {
        return Err(shape_err("Missing argument $description.".into()));
    }
    // A lone registered space name is a zero-channel description.
    if let Some(Arg::Positional(Expr::Literal(crate::ast_expr::Literal::Ident(word)))) =
        call.args.first()
        && let Some(space) = crate::color4::lookup(word)
    {
        return Err(shape_err(format!(
            "$description: The {} color space has 3 channels but {word} has 0.",
            space.name
        )));
    }
    if let Some(Arg::Positional(e)) = call.args.first() {
        match e {
            Expr::List { items, .. } if items.is_empty() => {
                return Err(shape_err(
                    "$description: Color component list may not be empty.".into(),
                ));
            }
            Expr::List { bracketed: true, .. } => {
                let v = st.eval_expr(e)?;
                return Err(shape_err(format!(
                    "$description: Expected an unbracketed list, was {}",
                    v.to_css()
                )));
            }
            Expr::List { sep: ListSep::Comma, .. } => {
                let v = st.eval_expr(e)?;
                return Err(shape_err(format!(
                    "$description: Expected a space- or slash-separated list, was ({})",
                    v.to_css()
                )));
            }
            Expr::FunctionCall(inner) if inner.name == "slash" || inner.name == "list.slash" => {
                let n = inner.args.len();
                if n != 2 {
                    return Err(shape_err(format!(
                        "$description: Only 2 slash-separated elements allowed, but {n} {} passed.",
                        if n == 1 { "was" } else { "were" }
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
