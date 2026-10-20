//! The modern single-argument CSS color call rebuild (`rgb`/`rgba`/`hsl`/
//! `hsla`/`hwb`, bare or `color.`-prefixed) for [`EvalState`] — the space-color
//! counterpart of the Color 4 literal parser in [`super::eval_color4`]. Split
//! out of `eval_function.rs` for the 500-line budget (round 65). The dispatch
//! order it implements comes straight from the `one_arg` / `special_functions`
//! goldens: shape errors (empty / bracketed / comma / slash-count) before any
//! evaluation, then per-slot classification — symbolic (`var()`/`calc()`, a
//! relative `from` anchor) renders the call verbatim with the alpha slash kept
//! literal, concrete non-numbers raise dart `\`$channels:\`` type errors, wrong
//! counts raise the space-count error, and a clean channel list dispatches to
//! the legacy positional builtins.

use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep, Literal};
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{Ev, EvalState};

/// How one `hsl`/`hwb` argument slot reads for the modern single-argument form.
enum Slot {
    /// a number or `none` — a real channel value
    Number,
    /// symbolic CSS construct (`var()`/`calc()`, a `/` product, a relative
    /// `from` anchor) — the whole call stays verbatim CSS
    Verbatim,
    /// a concrete non-channel value (quoted string, bare word, color, …) —
    /// dart raises a `$channels:` type error
    Concrete,
}

fn slot_kind(e: &Expr) -> Slot {
    match e {
        Expr::Literal(Literal::Number { .. }) => Slot::Number,
        Expr::Literal(Literal::Ident(w)) if w == "none" => Slot::Number,
        Expr::Literal(Literal::Ident(w)) if w.eq_ignore_ascii_case("from") => Slot::Verbatim,
        Expr::FunctionCall(_) => Slot::Verbatim,
        Expr::Binary {
            op, left, right, ..
        } => {
            if matches!(op, BinOp::Div | BinOp::Mul) {
                // `a / b` inside a slot is kept literal by the CSS layer.
                Slot::Verbatim
            } else {
                match (slot_kind(left), slot_kind(right)) {
                    (Slot::Verbatim, _) | (_, Slot::Verbatim) => Slot::Verbatim,
                    (Slot::Number, Slot::Number) => Slot::Number,
                    _ => Slot::Concrete,
                }
            }
        }
        _ => Slot::Concrete,
    }
}

/// Split a space-separated channel list into its channel ASTs plus the alpha
/// pulled out of a trailing `channels / alpha` division (CSS color syntax).
fn split_channels(items: &[Expr]) -> (Vec<&Expr>, Option<&Expr>) {
    let mut channels: Vec<&Expr> = Vec::with_capacity(items.len());
    let mut alpha: Option<&Expr> = None;
    for item in items {
        if let Expr::Binary {
            op: BinOp::Div,
            left,
            right,
            ..
        } = item
        {
            channels.push(left);
            alpha = Some(right);
        } else {
            channels.push(item);
        }
    }
    (channels, alpha)
}
impl EvalState {
    /// Handle the modern CSS `rgb(r g b / a)`, `hsl(…)` and `hwb(…)`
    /// space-separated single-argument forms — bare or under the `color`
    /// namespace (round-28 rgb mechanism, generalized in rounds 64/65 to the
    /// dart `$channels` one_arg error family). Returns `Ok(None)` for every
    /// other shape so the caller falls through to ordinary builtin dispatch /
    /// passthrough unchanged.
    pub(super) fn eval_space_color(&mut self, call: &FunctionCall) -> Ev<Option<SassValue>> {
        let rgb_family =
            call.namespace.is_none() && matches!(call.name.as_str(), "rgb" | "rgba");
        // The modern `hsl`/`hsla`/`hwb` single-argument spelling — bare or under
        // the `color` namespace — routes here too (round-64, hwb/one_arg goldens):
        // split channels go to the legacy builtins (which produce the goldens'
        // `hsl(...)` serialization), symbolic shapes fall back to a verbatim
        // slash-preserving passthrough.
        let hsl_family = matches!(call.name.as_str(), "hsl" | "hsla" | "hwb")
            && matches!(call.namespace.as_deref(), None | Some("color"));
        if !rgb_family && !hsl_family {
            return Ok(None);
        }
        // A single positional argument is the modern `$channels` spelling; every
        // legacy comma-separated spelling (`rgb(1, 2, 3)`) keeps the plain
        // builtins' own validation.
        let Some(Arg::Positional(arg0)) = call.args.first() else {
            return Ok(None);
        };
        if call.args.len() != 1 {
            return Ok(None);
        }
        let slash_msg = rgb_family || call.name == "hwb";
        let channel_names: &[&str; 3] = if rgb_family {
            &["red", "green", "blue"]
        } else if call.name == "hwb" {
            &["hue", "whiteness", "blackness"]
        } else {
            &["hue", "saturation", "lightness"]
        };
        let space_label = if rgb_family {
            "rgb"
        } else if call.name == "hwb" {
            "hwb"
        } else {
            "hsl"
        };
        // Normalize the argument into channel ASTs plus the optional alpha, and
        // raise the shape errors dart raises before any channel is inspected.
        let channels: Vec<&Expr>;
        let alpha: Option<&Expr>;
        let verbatim_items: Option<&[Expr]>;
        // True when the alpha came as the second element of a slash *pair*
        // (`list.slash(channels, α)`) rather than a `channels / α` division
        // inside the space list: the pair's verbatim rendering keeps spaces
        // around the slash, the division's is literal (`b/25%`).
        let mut alpha_via_pair = false;
        match arg0 {
            Expr::List {
                items,
                sep,
                bracketed,
                ..
            } => {
                if items.is_empty() {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        "$channels: Color component list may not be empty.",
                    ));
                }
                if *bracketed {
                    let v = self.eval_expr(arg0)?;
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        format!(
                            "$channels: Expected an unbracketed list, was {}",
                            v.to_css()
                        ),
                    ));
                }
                match sep {
                    ListSep::Comma => {
                        let v = self.eval_expr(arg0)?;
                        return Err(SassError::new(
                            ErrorKind::Builtins,
                            format!(
                                "$channels: Expected a space-{}separated list, was ({})",
                                if slash_msg { " or slash-" } else { "-" },
                                v.to_css()
                            ),
                        ));
                    }
                    ListSep::Slash => {
                        let n = items.len();
                        if n != 2 {
                            return Err(SassError::new(
                                ErrorKind::Builtins,
                                format!(
                                    "$channels: Only 2 slash-separated elements allowed, but {n} {} passed.",
                                    if n == 1 { "was" } else { "were" }
                                ),
                            ));
                        }
                        // A top-level slash list (e.g. from `list.append(…, $separator:
                        // slash)`) is the `(channels, alpha)` pair.
                        let ch = &items[0];
                        match ch {
                            Expr::List { items: inner, .. } => {
                                let (c, a) = split_channels(inner);
                                channels = c;
                                alpha = a.or(Some(&items[1]));
                                alpha_via_pair = a.is_none();
                                verbatim_items = Some(inner.as_slice());
                            }
                            other => {
                                channels = vec![other];
                                alpha = Some(&items[1]);
                                alpha_via_pair = true;
                                verbatim_items = Some(std::slice::from_ref(other));
                            }
                        }
                    }
                    ListSep::Space => {
                        let (c, a) = split_channels(items);
                        channels = c;
                        alpha = a;
                        verbatim_items = Some(items.as_slice());
                    }
                }
            }
            Expr::FunctionCall(inner)
                if inner.name == "slash" || inner.name == "list.slash" =>
            {
                let n = inner.args.len();
                if n != 2 {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        format!(
                            "$channels: Only 2 slash-separated elements allowed, but {n} {} passed.",
                            if n == 1 { "was" } else { "were" }
                        ),
                    ));
                }
                let Arg::Positional(ch) = &inner.args[0] else {
                    return Ok(None);
                };
                let a = match &inner.args[1] {
                    Arg::Positional(a) => Some(a),
                    _ => None,
                };
                match ch {
                    Expr::List { items, .. } => {
                        let (c, inner_alpha) = split_channels(items);
                        channels = c;
                        alpha = inner_alpha.or(a);
                        alpha_via_pair = inner_alpha.is_none() && a.is_some();
                        verbatim_items = Some(items.as_slice());
                    }
                    other => {
                        channels = vec![other];
                        alpha = a;
                        alpha_via_pair = a.is_some();
                        verbatim_items = Some(std::slice::from_ref(other));
                    }
                }
            }
            // A lone value: `rgb(1)` / `color.hwb(0)` are one-channel lists in
            // dart's message (`but 1 has 1`, no parentheses); anything else
            // (variables, symbolic constructs) keeps today's routing.
            other => match slot_kind(other) {
                Slot::Number => {
                    let shown = self.eval_expr(other)?.to_css();
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        format!(
                            "$channels: The {space_label} color space has 3 channels but {shown} has 1.",
                        ),
                    ));
                }
                _ => return Ok(None),
            },
        }
        // Slot classification drives the remaining errors (round-64 rules):
        // symbolic/relative keeps the call verbatim, concrete non-numbers are
        // `$channels:` type errors, and a clean 3-channel list goes to strict
        // units and the legacy dispatch.
        let mut first_concrete: Option<(usize, &Expr)> = None;
        let mut verbatim = false;
        for (i, e) in channels.iter().enumerate() {
            match slot_kind(e) {
                Slot::Number => {}
                Slot::Verbatim => verbatim = true,
                Slot::Concrete => {
                    if i < 3 && first_concrete.is_none() {
                        first_concrete = Some((i, e));
                    }
                }
            }
        }
        if let Some(a) = alpha
            && matches!(slot_kind(a), Slot::Verbatim)
        {
            verbatim = true;
        }
        if !verbatim {
            if let Some((i, e)) = first_concrete {
                let v = self.eval_expr(e)?;
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$channels: Expected {} channel to be a number, was {}.",
                        channel_names[i],
                        v.to_css()
                    ),
                ));
            }
            if channels.len() != 3 {
                // A wrong channel count over an otherwise-concrete list; the
                // alpha (when split out) joins the rendering after ` / `.
                let mut shown: Vec<String> = Vec::with_capacity(channels.len());
                for e in &channels {
                    shown.push(self.render_css(e)?);
                }
                let shown = match alpha {
                    Some(a) => format!("{} / {}", shown.join(" "), self.render_css(a)?),
                    None => shown.join(" "),
                };
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$channels: The {space_label} color space has 3 channels but ({shown}) has {}.",
                        channels.len()
                    ),
                ));
            }
            // `color.hwb(0 30 40%)`: hwb's w/b channels only accept `%` (or none).
            if call.name == "hwb" {
                for (i, key) in [(1usize, "whiteness"), (2, "blackness")] {
                    if let Expr::Literal(Literal::Number { value, unit }) = channels[i]
                        && unit.as_deref() != Some("%")
                    {
                        let shown = format!("{value}{}", unit.as_deref().unwrap_or(""));
                        return Err(SassError::new(
                            ErrorKind::Builtins,
                            format!("${key}: Expected {shown} to have unit \"%\"."),
                        ));
                    }
                }
            }
        }
        if rgb_family {
            // Shape/type errors above already claimed the concrete forms; what
            // remains (a clean list, symbolic channels, a wrong count) keeps the
            // historical builtins dispatch untouched.
            return self.legacy_dispatch(call, &channels, alpha);
        }
        if channels.len() == 3 && !verbatim {
            return self.legacy_dispatch(call, &channels, alpha);
        }
        // A symbolic hsl/hsla slot (calc/var/env/attr) re-emits the call in
        // the legacy COMMA form — `hsl(1, calc(2%), 3%, 0.4)`
        // (`hsl/one_arg/special_functions/*`). `from` anchors and `/`-product
        // slots are not leaf-ok, so the verbatim slash form below keeps them.
        if channels.len() == 3
            && let Some(text) = self.try_comma_symbolic_color(call, &channels, alpha)?
        {
            return Ok(Some(SassValue::Str { text, quoted: false }));
        }
        // Symbolic / relative colors render verbatim with the alpha slash kept
        // literal (`hwb(0 30% var(--c)/0.5)`). The hsl family only.
        let items = verbatim_items.unwrap_or(std::slice::from_ref(arg0));
        let mut parts = Vec::with_capacity(items.len());
        for item in items {
            if let Expr::Binary {
                op: BinOp::Div,
                left,
                right,
                ..
            } = item
            {
                parts.push(format!("{}/{}", self.render_css(left)?, self.render_css(right)?));
            } else {
                parts.push(self.render_css(item)?);
            }
        }
        let mut text = parts.join(" ");
        if alpha_via_pair
            && let Some(a) = alpha
        {
            text = format!("{text} / {}", self.render_css(a)?);
        }
        Ok(Some(SassValue::Str {
            text: format!("{}({})", call.name, text),
            quoted: false,
        }))
    }

    /// positional builtins (which keep the goldens' serialization forms),
    /// normalizing a percentage alpha to `0..=1`.
    pub(super) fn legacy_dispatch(
        &mut self,
        call: &FunctionCall,
        channels: &[&Expr],
        alpha: Option<&Expr>,
    ) -> Ev<Option<SassValue>> {
        let mut positional: Vec<SassValue> = Vec::with_capacity(channels.len() + 1);
        for e in channels {
            positional.push(self.eval_expr(e)?);
        }
        if let Some(a) = alpha {
            positional.push(match self.eval_expr(a)? {
                SassValue::Number {
                    value,
                    unit: Some(u),
                } if u == "%" => SassValue::Number {
                    value: value / 100.0,
                    unit: None,
                },
                other => other,
            });
        }
        let named = std::collections::HashMap::new();
        // A base name with no global builtin (e.g. bare `hwb`, which only
        // exists namespaced in dart) falls through to the Color4 literal /
        // passthrough layers rather than erroring.
        let Some(result) =
            crate::builtins::call(call.namespace.as_deref(), &call.name, &positional, &named)
        else {
            return Ok(None);
        };
        result
            .map_err(|e| e.note(format!("during color `{}` in module {}", call.name, self.name)))
            .map(Some)
    }

}

/// The `color.{grayscale,invert,alpha,opacity}` CSS overloads, exactly as the
/// golden sections spell them out:
/// * `color.grayscale(1)`/`color.invert(1)`/`color.opacity(1)` — a lone
///   **Number** argument — re-emit the plain spelling under the base name
///   (`css_overloads` goldens); a lone symbolic string or bare word does NOT
///   (`color.opacity(var(--c))`, `color.grayscale(c)` keep the Sass
///   `$color: … is not a color.`), and `color.alpha(1)` has no numeric
///   overload at all (`alpha/error/type`).
/// * keyword pairs outside the parameter names (`color.alpha(c=d)`,
///   `color.alpha(c=d, e=f, g=h)`) re-emit verbatim in sorted-key form.
pub(super) fn css_color_overload(raw: &super::eval_mixin::RawArgs, name: &str) -> Option<SassValue> {
    const PARAMS: [&str; 3] = ["color", "weight", "space"];
    if raw.named().keys().any(|k| PARAMS.contains(&k.as_str())) {
        return None;
    }
    let parts: Vec<String> = if raw.positional().is_empty() && !raw.named().is_empty() {
        let mut keys: Vec<&String> = raw.named().keys().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| format!("{k}={}", raw.named()[k]))
            .collect()
    } else if raw.named().is_empty()
        && raw.positional().len() == 1
        && matches!(&raw.positional()[0], SassValue::Number { .. })
        && name != "alpha"
    {
        vec![raw.positional()[0].to_css()]
    } else {
        return None;
    };
    Some(SassValue::Str {
        text: format!("{name}({})", parts.join(", ")),
        quoted: false,
    })
}
