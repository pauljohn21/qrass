//! CSS Color 4 literal parsing for [`EvalState`] (task 11.3, round 2).
//!
//! Turns space-separated literals (`color(srgb 0.1 0.2 0.3 / 0.4)`, `lab(...)`,
//! `lch(...)`, `oklab(...)`, `oklch(...)`, `hsl(...)`, `hwb(...)`) into owned
//! [`crate::color4::Color4`] values. Anything that does not parse as a strict
//! Color 4 literal falls through untouched to the plain-CSS passthrough. The
//! structural argument guards and channel coercion helpers live in the sibling
//! [`crate::eval::eval_color4_check`] module (500-line split, round 62).

use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep};
use crate::error::{ErrorKind, SassError};
use super::eval_color4_check::{channel_value, guard_bare4, guard_wrapper, is_symbolic, relative_anchor, shape_err};
use crate::value::SassValue;

use super::{Ev, EvalState};

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
        // The four bare perceptual spaces and the `color()` wrapper take exactly
        // one argument each and dart rejects malformed shapes by *structure* —
        // the sibling `eval_color4_check` guards fire only on shapes that can
        // never be a valid literal or a meaningful passthrough, evaluating
        // solely inside an already-rejected branch to render the message
        // (the W4b lesson: never pre-evaluate the shared list on the hot path).
        // Relative color syntax (`lab(from #aaa l a b)` and its `From` /
        // `list.slash` spellings) is exempt wholesale: the goldens keep it as
        // verbatim CSS.
        let bare4 = !wrapper
            && matches!(call.name.as_str(), "lab" | "lch" | "oklab" | "oklch")
            && !relative_anchor(call.args.first());
        if bare4 {
            guard_bare4(self, call)?;
        }
        let wstrict = wrapper && !relative_anchor(call.args.first());
        if wstrict {
            guard_wrapper(self, call)?;
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
