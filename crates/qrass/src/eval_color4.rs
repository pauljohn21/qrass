//! CSS Color 4 literal parsing for [`EvalState`] (task 11.3, round 2).
//!
//! Turns space-separated literals (`color(srgb 0.1 0.2 0.3 / 0.4)`, `lab(...)`,
//! `lch(...)`, `oklab(...)`, `oklch(...)`, `hsl(...)`, `hwb(...)`) into owned
//! [`crate::color4::Color4`] values. Anything that does not parse as a strict
//! Color 4 literal falls through untouched to the plain-CSS passthrough.

use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep};
use crate::value::SassValue;

use super::{Ev, EvalState};

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
                if wrapper && matches!(inner.name.as_str(), "slash" | "list.slash") =>
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
                    None => return Ok(None),
                },
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
            return Ok(None);
        }
        let alpha_expr = alpha_expr.or(extra_alpha);
        let mut parsed: [Option<f64>; 3] = [None, None, None];
        for (idx, (v, ch)) in comps.iter().zip(&space.channels).enumerate() {
            match channel_value(v, ch) {
                Some(cv) => parsed[idx] = cv,
                None => return Ok(None),
            }
        }
        let mut alpha_missing = false;
        let alpha = match alpha_expr {
            Some(e) => match self.eval_expr(e)? {
                SassValue::Str { text, quoted: false } if text == "none" => {
                    alpha_missing = true;
                    None
                }
                SassValue::Number { value, unit } => Some(match unit.as_deref() {
                    Some("%") => value / 100.0,
                    _ => value,
                }),
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
