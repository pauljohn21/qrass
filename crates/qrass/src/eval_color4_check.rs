//! Structural `$channels` / `$description` argument guards for the modern
//! Color 4 literal parser (sibling of [`crate::eval_color4`], split to keep
//! both files within the 500-line budget).
//!
//! The rule these encode (rounds 60/61): a guard may only fire on an argument
//! shape that can *never* be a valid literal or a meaningful plain-CSS
//! passthrough, and an argument is evaluated solely inside an already-rejected
//! branch to render the goldens' message. The hot path — well-formed literals and
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

/// A golden-shaped argument error for a modern color literal (arity, list shape,
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
            // per-unit coercion: `Some(_)` a converted channel, `None` a unit that
            // can never be this channel (the caller falls through to plain CSS).
            let out: Option<f64> = match ch.unit {
                Unit::Pct => match u {
                    None | Some("%") => Some(*value),
                    Some(_) => None,
                },
                Unit::Deg => match u {
                    None | Some("deg") => Some(*value),
                    Some("rad") => Some(value.to_degrees()),
                    Some("grad") => Some(*value * 0.9),
                    Some("turn") => Some(*value * 360.0),
                    Some(_) => None,
                },
                Unit::Num => match u {
                    None => Some(*value),
                    Some("%") => Some(*value / 100.0),
                    Some(_) => None,
                },
            };
            // `out.map(Some)`: unit mismatch (None) stays the outer None, a coerced
            // value becomes Some(Some(..)); the powerless `none` arm above is Some(None).
            out.map(Some)
        }
        _ => None,
    }
}

/// The bare perceptual spaces (`lab`/`lch`/`oklab`/`oklch`) take exactly one
/// `$channels` argument; reject the malformed shapes the goldens name. `Ok(())`
/// means "no structural complaint — continue the normal parse".
pub(crate) fn guard_bare4(st: &mut EvalState, call: &FunctionCall) -> Ev<()> {
    // arity gate first: 0 → missing, >1 → only-one-allowed; short-circuits before
    // any argument is evaluated, exactly like the old guard `return`s.
    (match call.args.len() {
        0 => Some(shape_err("Missing argument $channels.".into())),
        n if n > 1 => Some(shape_err(format!("Only 1 argument allowed, but {n} were passed."))),
        _ => None,
    })
    .map_or(Ok(()), Err)?;
    // the channel-shape gate: `?` propagation is R3's sanctioned early-failure
    // carrier, so the whole body is one expression returning the match result.
    match call.args.first() {
        Some(Arg::Positional(e)) => bare4_channel(st, &call.name, e),
        _ => Ok(()),
    }
}

/// The single positional `$channels` shape check: each malformed form yields its
/// golden error, well-formed shapes fall through to `Ok(())`.
fn bare4_channel(st: &mut EvalState, name: &str, e: &Expr) -> Ev<()> {
    match e {
        Expr::List { items, .. } if items.is_empty() => {
            Err(shape_err(
                "$channels: Color component list may not be empty.".into(),
            ))
        }
        Expr::List { bracketed: true, .. } => {
            let v = st.eval_expr(e)?;
            Err(shape_err(format!(
                "$channels: Expected an unbracketed list, was {}",
                v.to_css()
            )))
        }
        Expr::List { sep: ListSep::Comma, .. } => {
            let v = st.eval_expr(e)?;
            Err(shape_err(format!(
                "$channels: Expected a space- or slash-separated list, was ({})",
                v.to_css()
            )))
        }
        Expr::FunctionCall(inner) if inner.name == "slash" || inner.name == "list.slash" => {
            let n = inner.args.len();
            match n == 2 {
                false => Err(shape_err(format!(
                    "$channels: Only 2 slash-separated elements allowed, but {n} {} passed.",
                    if n == 1 { "was" } else { "were" }
                ))),
                true => Ok(()),
            }
        }
        Expr::List {
            sep: ListSep::Space,
            items,
            ..
        } if items.len() < 3 =>
        {
            let vals: Vec<SassValue> = items
                .iter()
                .map(|i| st.eval_expr(i))
                .collect::<Ev<_>>()?;
            match vals.iter().all(|v| !is_symbolic(v)) {
                true => {
                    let shown = vals
                        .iter()
                        .map(|v| v.to_css())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let space = crate::color4::lookup(name).expect("bare4 checked");
                    Err(shape_err(format!(
                        "$channels: The {} color space has 3 channels but ({shown}) has {}.",
                        space.name,
                        vals.len()
                    )))
                }
                false => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

/// The `color(…)` wrapper takes one `$description` argument and the goldens reject
/// malformed shapes the same way, with the `$description:` prefix (round-60
/// color/error goldens). Relative syntax (`color(from …)`) is exempt at the
/// call site; a plain-CSS `color(…)` filter is claimed by the passthrough
/// layer before this code ever runs. `Ok(())` means "keep parsing".
pub(crate) fn guard_wrapper(st: &mut EvalState, call: &FunctionCall) -> Ev<()> {
    // empty args → missing $description (short-circuits before any eval)
    (match call.args.is_empty() {
        true => Some(shape_err("Missing argument $description.".into())),
        false => None,
    })
    .map_or(Ok(()), Err)?;
    // A lone registered space name is a zero-channel description.
    (match call.args.first() {
        Some(Arg::Positional(Expr::Literal(crate::ast_expr::Literal::Ident(word)))) => {
            crate::color4::lookup(word).map(|space| {
                shape_err(format!(
                    "$description: The {} color space has 3 channels but {word} has 0.",
                    space.name
                ))
            })
        }
        _ => None,
    })
    .map_or(Ok(()), Err)?;
    match call.args.first() {
        Some(Arg::Positional(e)) => wrapper_channel(st, e),
        _ => Ok(()),
    }
}

/// The single positional `$description` list-shape check (empty / bracketed /
/// comma / slash arity), mirroring the bare4 gate with the `$description:` prefix.
fn wrapper_channel(st: &mut EvalState, e: &Expr) -> Ev<()> {
    match e {
        Expr::List { items, .. } if items.is_empty() => {
            Err(shape_err(
                "$description: Color component list may not be empty.".into(),
            ))
        }
        Expr::List { bracketed: true, .. } => {
            let v = st.eval_expr(e)?;
            Err(shape_err(format!(
                "$description: Expected an unbracketed list, was {}",
                v.to_css()
            )))
        }
        Expr::List { sep: ListSep::Comma, .. } => {
            let v = st.eval_expr(e)?;
            Err(shape_err(format!(
                "$description: Expected a space- or slash-separated list, was ({})",
                v.to_css()
            )))
        }
        Expr::FunctionCall(inner) if inner.name == "slash" || inner.name == "list.slash" => {
            let n = inner.args.len();
            match n == 2 {
                false => Err(shape_err(format!(
                    "$description: Only 2 slash-separated elements allowed, but {n} {} passed.",
                    if n == 1 { "was" } else { "were" }
                ))),
                true => Ok(()),
            }
        }
        _ => Ok(()),
    }
}
