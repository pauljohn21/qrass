//! CSS Color 4 literal parsing for [`EvalState`] (task 11.3, round 2).
//!
//! Turns space-separated literals (`color(srgb 0.1 0.2 0.3 / 0.4)`, `lab(...)`,
//! `lch(...)`, `oklab(...)`, `oklch(...)`, `hsl(...)`, `hwb(...)`) into owned
//! [`crate::color4::Color4`] values. Anything that does not parse as a strict
//! Color 4 literal falls through untouched to the plain-CSS passthrough.

use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep};
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{Ev, EvalState};

/// A value that must keep falling through to plain-CSS passthrough rather than
/// becoming a Color 4 argument error: a `var()`/`calc()`/`min()`/`env()`/… symbolic
/// construct (the round-28 rgb() lesson — an unresolvable channel is CSS, not a
/// Sass color argument). `none` and concrete numbers/strings are *not* symbolic.
fn is_symbolic(v: &SassValue) -> bool {
    match v {
        SassValue::Str { text, .. } => {
            ["var(", "calc(", "min(", "max(", "clamp(", "env(", "url(", "attr("]
                .iter()
                .any(|f| text.contains(f))
        }
        _ => false,
    }
}

/// A dart-shaped `$channels` argument error (arity, list shape, slash element
/// count or channel count of a modern literal).
fn shape_err(msg: String) -> SassError {
    SassError::new(ErrorKind::Builtins, msg)
}

/// Whether a bare-space call is relative color syntax — `lab(from …)`, the
/// case-mutated `lab(From …)`, or the `list.slash(from …, α)` wrapper. None of
/// these are channel literals; the goldens keep them as verbatim CSS, so the
/// strict `$channels` guards must not claim them.
fn relative_anchor(arg: Option<&Arg>) -> bool {
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
fn channel_value(v: &SassValue, ch: &crate::color4::Channel) -> Option<Option<f64>> {
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

impl EvalState {
    /// Parse CSS Color 4 space-separated literals into a Color 4 value:
    /// `color(<space> c1 c2 c3 [/ a])` plus the bare forms `lab`/`lch`/`oklab`/
    /// `oklch`/`hsl`/`hwb`. Returns `Ok(None)` for any other shape or for a
    /// component that does not parse as a channel value, leaving the existing
    /// plain-CSS passthrough untouched.
    pub(crate) fn eval_color4_literal(&mut self, call: &FunctionCall) -> Ev<Option<SassValue>> {
        if call.namespace.is_some() {
            return Ok(None);
        }
        let wrapper = call.name == "color";
        if !wrapper && crate::color4::lookup(&call.name).is_none() {
            return Ok(None);
        }
        // The four bare perceptual spaces take exactly one `$channels` argument
        // and dart rejects malformed shapes by *structure* — arity, bracketed /
        // comma / empty lists, a wrong-length slash list or too few channels.
        // Every guard below fires only on a shape that can never be a valid
        // literal or a meaningful passthrough, and evaluation happens solely on
        // those already-rejected branches (the W4b lesson: never pre-evaluate the
        // shared list on the hot path). A symbolic channel still passes through.
        // Relative color syntax (`lab(from #aaa l a b)`) is exempt wholesale: its
        // `from` anchor makes it not a channel literal, and the goldens keep it as
        // verbatim CSS (the channel items are bare identifiers, so the channel
        // pipeline below never claims them anyway).
        let bare4 = !wrapper
            && matches!(call.name.as_str(), "lab" | "lch" | "oklab" | "oklch")
            && !relative_anchor(call.args.first());
        if bare4 {
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
                        let v = self.eval_expr(e)?;
                        return Err(shape_err(format!(
                            "$channels: Expected an unbracketed list, was {}",
                            v.to_css()
                        )));
                    }
                    Expr::List { sep: ListSep::Comma, .. } => {
                        let v = self.eval_expr(e)?;
                        return Err(shape_err(format!(
                            "$channels: Expected a space- or slash-separated list, was ({})",
                            v.to_css()
                        )));
                    }
                    Expr::FunctionCall(inner)
                        if inner.name == "slash" || inner.name == "list.slash" =>
                    {
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
                            .map(|i| self.eval_expr(i))
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
        }
        // The `color(…)` wrapper takes one `$description` argument and dart
        // rejects malformed shapes the same way, with the `$description:`
        // prefix (round-60 color/error goldens). Relative syntax
        // (`color(from …)`) is again exempt; a plain-CSS `color(…)` filter is
        // claimed by the passthrough layer before this code ever runs.
        let wstrict = wrapper && !relative_anchor(call.args.first());
        if wstrict {
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
                        let v = self.eval_expr(e)?;
                        return Err(shape_err(format!(
                            "$description: Expected an unbracketed list, was {}",
                            v.to_css()
                        )));
                    }
                    Expr::List { sep: ListSep::Comma, .. } => {
                        let v = self.eval_expr(e)?;
                        return Err(shape_err(format!(
                            "$description: Expected a space- or slash-separated list, was ({})",
                            v.to_css()
                        )));
                    }
                    Expr::FunctionCall(inner)
                        if inner.name == "slash" || inner.name == "list.slash" =>
                    {
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
        }
        // The channel source is either a single positional Space list, the
        // legacy `$description:` named spelling, or a `list.slash(<space list>,
        // <alpha>)` wrapper that flattens to the same shape.
        let mut slash_wrap: Option<&FunctionCall> = None;
        let space_list: Option<&Vec<Expr>> = match call.args.first() {
            Some(Arg::Positional(Expr::List {
                items,
                sep: ListSep::Space,
                ..
            })) if items.len() >= 3 => Some(items),
            Some(Arg::Named { name, value }) if name == "description" => match value {
                // `$description: srgb 0.1 0.2 0.3 / 0.4` — the legacy named
                // spelling wraps the same space-list value expression.
                Expr::List {
                    items,
                    sep: ListSep::Space,
                    ..
                } if items.len() >= 3 => Some(items),
                _ => None,
            },
            Some(Arg::Positional(Expr::FunctionCall(inner)))
                if (wrapper || bare4) && matches!(inner.name.as_str(), "slash" | "list.slash") =>
            {
                match inner.args.first() {
                    Some(Arg::Positional(Expr::List {
                        items,
                        sep: ListSep::Space,
                        ..
                    })) if items.len() >= 3 => {
                        slash_wrap = Some(inner);
                        Some(items)
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        // A `list.slash(..., $alpha)` wrapper carries the alpha as its own
        // second argument.
        let extra_alpha: Option<&Expr> = match slash_wrap {
            Some(inner) => match inner.args.get(1) {
                Some(Arg::Positional(e)) => Some(e),
                _ => None,
            },
            None => None,
        };
        let Some(items) = space_list else {
            return Ok(None);
        };
        let mut iter = items.iter();
        let space: &'static crate::color4::Space = if wrapper {
            let Some(first) = iter.next() else { return Ok(None) };
            match self.eval_expr(first)? {
                SassValue::Str { text, quoted: false } => match crate::color4::lookup(&text) {
                    Some(s) => s,
                    // A concrete unknown word is dart's error, not a CSS
                    // passthrough (`Unknown color space "foo".`); symbolic
                    // constructs keep falling through.
                    None if wstrict && !text.contains('(') => {
                        return Err(shape_err(format!(
                            "$description: Unknown color space \"{text}\"."
                        )));
                    }
                    None => return Ok(None),
                },
                // A quoted space slot is the relative-syntax spelling dart
                // rejects verbatim; a number is simply not a string.
                SassValue::Str { .. } if wstrict => {
                    let shown = self.eval_expr(first)?.to_css();
                    return Err(shape_err(format!(
                        "$description: Expected {shown} to be an unquoted string."
                    )));
                }
                v @ SassValue::Number { .. } if wstrict && !is_symbolic(&v) => {
                    return Err(shape_err(format!(
                        "$description: {} is not a string.",
                        v.to_css()
                    )));
                }
                _ => return Ok(None),
            }
        } else {
            crate::color4::lookup(&call.name).expect("checked above")
        };
        let mut comps: Vec<SassValue> = Vec::new();
        let mut alpha_expr: Option<&Expr> = None;
        for item in iter {
            if let Expr::Binary {
                op: BinOp::Div,
                left,
                right,
                ..
            } = item
            {
                comps.push(self.eval_expr(left)?);
                alpha_expr = Some(right);
            } else {
                comps.push(self.eval_expr(item)?);
            }
        }
        if comps.len() != 3 {
            // The four bare names and the wrapper reach here only with a
            // wrong-length space list (shorter lists were rejected by the
            // structural guards above); dart reports the mismatch as a
            // channel-count error over the whole list — the wrapper's copy
            // shows the space name as the first element.
            if (bare4 || wstrict) && !comps.iter().any(is_symbolic) {
                let shown = if wstrict {
                    let mut all = vec![self.eval_expr(items.first().expect("wrapper has items"))?.to_css()];
                    all.extend(comps.iter().map(|v| v.to_css()));
                    all.join(" ")
                } else {
                    comps
                        .iter()
                        .map(|v| v.to_css())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                let space = if wrapper {
                    space.name
                } else {
                    crate::color4::lookup(&call.name).expect("bare4 checked").name
                };
                let prefix = if wstrict { "description" } else { "channels" };
                return Err(shape_err(format!(
                    "${prefix}: The {space} color space has 3 channels but ({shown}) has {}.",
                    comps.len()
                )));
            }
            return Ok(None);
        }
        let alpha_expr = alpha_expr.or(extra_alpha);
        // dart names the failing argument differently per spelling: the wrapper
        // reports its `$description` argument, bare forms the `$channels`.
        let arg_name = if wrapper { "description" } else { "channels" };
        let mut parsed: [Option<f64>; 3] = [None, None, None];
        for (idx, (v, ch)) in comps.iter().zip(&space.channels).enumerate() {
            match channel_value(v, ch) {
                Some(cv) => parsed[idx] = cv,
                // A symbolic CSS value keeps the passthrough; a concrete-but-wrong
                // channel raises dart's argument error (per-channel unit vs type).
                None if is_symbolic(v) => return Ok(None),
                None => {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        match v {
                            SassValue::Number { .. } if matches!(ch.unit, crate::color4::Unit::Deg) => {
                                format!(
                                    "${}: Expected {} to have an angle unit (deg, grad, rad, turn).",
                                    ch.name,
                                    v.to_css()
                                )
                            },
                            SassValue::Number { .. } => format!(
                                "${}: Expected {} to have unit \"%\" or no units.",
                                ch.name,
                                v.to_css()
                            ),
                            _ => format!(
                                "${arg_name}: Expected {} channel to be a number, was {}.",
                                ch.name,
                                v.to_css()
                            ),
                        },
                    ));
                }
            }
        }
        let mut alpha_missing = false;
        let alpha = match alpha_expr {
            Some(e) => match self.eval_expr(e)? {
                SassValue::Str { text, quoted: false } if text == "none" => {
                    alpha_missing = true;
                    None
                }
                // dart validates the `$alpha` slot: unitless or `%` only; a bare
                // string reports the `$channels:`-prefixed type error (golden
                // quirk), a symbolic construct keeps the passthrough.
                v @ SassValue::Number { .. } => match &v {
                    SassValue::Number { value, unit } => match unit.as_deref() {
                        None => Some(*value),
                        Some("%") => Some(value / 100.0),
                        Some(_) => {
                            return Err(shape_err(format!(
                                "$alpha: Expected {} to have unit \"%\" or no units.",
                                v.to_css()
                            )));
                        }
                    },
                    _ => unreachable!(),
                },
                v @ SassValue::Str { .. } => {
                    if is_symbolic(&v) {
                        return Ok(None);
                    }
                    return Err(shape_err(format!(
                        "${arg_name}: {} is not a number.",
                        v.to_css()
                    )));
                }
                _ => return Ok(None),
            },
            None => None,
        };
        let c4 = crate::color4::Color4 {
            space,
            comps: parsed,
            alpha,
            alpha_missing,
        };
        let [r, g, b] = c4.to_rgb();
        let a = c4.alpha();
        Ok(Some(SassValue::Color(crate::value::Color {
            r,
            g,
            b,
            a,
            hex: false,
            c4: Some(Box::new(c4)),
            name: None,
            legacy: None,
            legacy_hsl: None,
        })))
    }
}
