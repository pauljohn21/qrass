//! The modern single-argument CSS color call rebuild (`rgb`/`rgba`/`hsl`/
//! `hsla`/`hwb`, bare or `color.`-prefixed) for [`EvalState`] — the space-color
//! counterpart of the Color 4 literal parser in [`super::eval_color4`]. Split
//! out of `eval_function.rs` for the 500-line budget (round 65). The dispatch
//! order it implements comes straight from the `one_arg` / `special_functions`
//! goldens: shape errors (empty / bracketed / comma / slash-count) before any
//! evaluation, then per-slot classification — symbolic (`var()`/`calc()`, a
//! relative `from` anchor) renders the call verbatim with the alpha slash kept
//! literal, concrete non-numbers raise the goldens `\`$channels:\`` type errors, wrong
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
    /// the goldens raise a `$channels:` type error
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
    items.iter().fold((Vec::new(), None), |(mut ch, mut al), item| {
        match item {
            Expr::Binary {
                op: BinOp::Div,
                left,
                right,
                ..
            } => {
                ch.push(left);
                al = Some(right);
            }
            other => ch.push(other),
        }
        (ch, al)
    })
}
/// Classification of the `eval_space_color` arg0 shape match: the channel
/// tuple of a well-formed space list, a golden error to raise, or a
/// passthrough. Keeps the dispatch single-exit (RFP contract R3/S2).
enum Shape<'a> {
    Good {
        channels: Vec<&'a Expr>,
        alpha: Option<&'a Expr>,
        verbatim_items: Option<&'a [Expr]>,
        via_pair: bool,
    },
    Fail(SassError),
    Pass,
}

/// The golden's space label for the channel-count error text.
fn space_label(rgb_family: bool, is_hwb: bool) -> &'static str {
    match (rgb_family, is_hwb) {
        (true, _) => "rgb",
        (false, true) => "hwb",
        (false, false) => "hsl",
    }
}

/// `color.hwb(0 30 40%)`: hwb's w/b channels only accept `%` (or none).
fn hwb_channel_err(channels: &[&Expr], is_hwb: bool) -> Option<SassError> {
    is_hwb
        .then(|| {
            [(1usize, "whiteness"), (2, "blackness")]
                .iter()
                .find_map(|(i, key)| match channels.get(*i) {
                    Some(Expr::Literal(Literal::Number { value, unit }))
                        if unit.as_deref() != Some("%") =>
                    {
                        let shown = format!("{value}{}", unit.as_deref().unwrap_or(""));
                        Some(SassError::new(
                            ErrorKind::Builtins,
                            format!("${key}: Expected {shown} to have unit \"%\"."),
                        ))
                    }
                    _ => None,
                })
        })
        .flatten()
}

/// `slash(…)` / `list.slash(…)` call shape: classify the channels argument
/// against the trailing alpha slot.
fn slash_call_shape<'a>(ch: &'a Expr, a: Option<&'a Expr>) -> Shape<'a> {
    match ch {
        Expr::List { items, .. } => {
            let (c, inner_alpha) = split_channels(items);
            Shape::Good {
                channels: c,
                alpha: inner_alpha.or(a),
                verbatim_items: Some(items.as_slice()),
                via_pair: inner_alpha.is_none() && a.is_some(),
            }
        }
        other => Shape::Good {
            channels: vec![other],
            alpha: a,
            verbatim_items: Some(std::slice::from_ref(other)),
            via_pair: a.is_some(),
        },
    }
}

impl EvalState {
    /// Handle the modern CSS `rgb(r g b / a)`, `hsl(…)` and `hwb(…)`
    /// space-separated single-argument forms — bare or under the `color`
    /// namespace (round-28 rgb mechanism, generalized in rounds 64/65 to the
    /// `$channels` one_arg golden error family). Returns `Ok(None)` for every
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
        // Single-exit arity filter (the contract's one-guard exemption): every
        // non-modern spelling routes to ordinary dispatch.
        let arg0 = match (
            rgb_family || hsl_family,
            call.args.len(),
            call.args.first(),
        ) {
            (true, 1, Some(Arg::Positional(a))) => a,
            _ => return Ok(None),
        };
        let slash_msg = rgb_family || call.name == "hwb";
        let err = |m: String| SassError::new(ErrorKind::Builtins, m);
        // Normalize the argument into channel ASTs plus the optional alpha and
        // raise the golden shape errors before any channel is inspected.
        let shape = match arg0 {
            Expr::List {
                items,
                sep,
                bracketed,
                ..
            } if items.is_empty() => {
                Shape::Fail(err("$channels: Color component list may not be empty.".into()))
            }
            Expr::List {
                bracketed: true, ..
            } => Shape::Fail(err(format!(
                "$channels: Expected an unbracketed list, was {}",
                self.eval_expr(arg0)?.to_css()
            ))),
            Expr::List {
                sep: ListSep::Comma,
                ..
            } => Shape::Fail(err(format!(
                "$channels: Expected a space-{}separated list, was ({})",
                if slash_msg { " or slash-" } else { "-" },
                self.eval_expr(arg0)?.to_css()
            ))),
            Expr::List {
                sep: ListSep::Slash,
                items,
                ..
            } => {
                let n = items.len();
                if n != 2 {
                    Shape::Fail(err(format!(
                        "$channels: Only 2 slash-separated elements allowed, but {n} {} passed.",
                        if n == 1 { "was" } else { "were" }
                    )))
                } else {
                    // A top-level slash list (e.g. from `list.append(…, $separator:
                    // slash)`) is the `(channels, alpha)` pair.
                    let ch = &items[0];
                    match ch {
                        Expr::List { items: inner, .. } => {
                            let (c, a) = split_channels(inner);
                            Shape::Good {
                                channels: c,
                                alpha: a.or(Some(&items[1])),
                                verbatim_items: Some(inner.as_slice()),
                                via_pair: a.is_none(),
                            }
                        }
                        other => Shape::Good {
                            channels: vec![other],
                            alpha: Some(&items[1]),
                            verbatim_items: Some(std::slice::from_ref(other)),
                            via_pair: true,
                        },
                    }
                }
            }
            Expr::List { items, .. } => {
                let (c, a) = split_channels(items);
                Shape::Good {
                    channels: c,
                    alpha: a,
                    verbatim_items: Some(items.as_slice()),
                    via_pair: false,
                }
            }
            Expr::FunctionCall(inner)
                if inner.name == "slash" || inner.name == "list.slash" =>
            {
                let n = inner.args.len();
                if n != 2 {
                    Shape::Fail(err(format!(
                        "$channels: Only 2 slash-separated elements allowed, but {n} {} passed.",
                        if n == 1 { "was" } else { "were" }
                    )))
                } else {
                    match (&inner.args[0], &inner.args[1]) {
                        (Arg::Positional(ch), Arg::Positional(a)) => slash_call_shape(ch, Some(a)),
                        (Arg::Positional(ch), _) => slash_call_shape(ch, None),
                        _ => Shape::Pass,
                    }
                }
            }
            // A lone value: `rgb(1)` / `color.hwb(0)` are one-channel lists in
            // the golden's message (`but 1 has 1`, no parentheses); anything
            // else (variables, symbolic constructs) keeps today's routing.
            other => match slot_kind(other) {
                Slot::Number => Shape::Fail(err(format!(
                    "$channels: The {} color space has 3 channels but {} has 1.",
                    space_label(rgb_family, call.name == "hwb"),
                    self.eval_expr(other)?.to_css()
                ))),
                _ => Shape::Pass,
            },
        };
        match shape {
            Shape::Pass => Ok(None),
            Shape::Fail(e) => Err(e),
            good => self.finish_space_color(call, good, rgb_family, arg0),
        }
    }

    /// Validation + dispatch tail of the modern space-color form: the golden
    /// error union is assembled lazily in the original order and the final
    /// routing is one `match` (contract R3/S2 single exit).
    fn finish_space_color<'a>(
        &mut self,
        call: &FunctionCall,
        shape: Shape<'a>,
        rgb_family: bool,
        arg0: &'a Expr,
    ) -> Ev<Option<SassValue>> {
        // the upstream dispatch already claimed Pass/Fail; destructuring here
        // keeps the arity budget and the single-exit shape
        let (channels, alpha, verbatim_items, alpha_via_pair) = match shape {
            Shape::Good {
                channels,
                alpha,
                verbatim_items,
                via_pair,
            } => (channels, alpha, verbatim_items, via_pair),
            _ => return Ok(None),
        };
        let is_hwb = call.name == "hwb";
        let channel_names: &[&str; 3] = match (rgb_family, is_hwb) {
            (true, _) => &["red", "green", "blue"],
            (false, true) => &["hue", "whiteness", "blackness"],
            (false, false) => &["hue", "saturation", "lightness"],
        };
        let space_label = space_label(rgb_family, is_hwb);
        // Slot classification drives the remaining errors (round-64 rules):
        // symbolic/relative keeps the call verbatim, concrete non-numbers are
        // `$channels:` type errors, and a clean 3-channel list goes to strict
        // units and the legacy dispatch.
        let (verb, first_concrete) = channels.iter().enumerate().fold(
            (false, None::<(usize, &Expr)>),
            |(verb, first), (i, e)| match slot_kind(e) {
                Slot::Number => (verb, first),
                Slot::Verbatim => (true, first),
                Slot::Concrete => (verb, first.or((i < 3).then_some((i, e)))),
            },
        );
        let verbatim = verb || alpha.is_some_and(|a| matches!(slot_kind(a), Slot::Verbatim));
        let err = |m: String| SassError::new(ErrorKind::Builtins, m);
        let verdict = (!verbatim)
            .then(|| -> Ev<Option<SassError>> {
                let concrete = first_concrete
                    .map(|(i, e)| {
                        self.eval_expr(e).map(|v| {
                            err(format!(
                                "$channels: Expected {} channel to be a number, was {}.",
                                channel_names[i],
                                v.to_css()
                            ))
                        })
                    })
                    .transpose()?;
                let count = (concrete.is_none() && channels.len() != 3)
                    .then(|| -> Ev<Option<SassError>> {
                        // A wrong channel count over an otherwise-concrete list; the
                        // alpha (when split out) joins the rendering after ` / `.
                        let shown = channels
                            .iter()
                            .map(|e| self.render_css(e))
                            .collect::<Ev<Vec<String>>>()?;
                        let shown = match alpha {
                            Some(a) => format!("{} / {}", shown.join(" "), self.render_css(a)?),
                            None => shown.join(" "),
                        };
                        Ok(Some(err(format!(
                            "$channels: The {space_label} color space has 3 channels but ({shown}) has {}.",
                            channels.len()
                        ))))
                    })
                    .transpose()?
                    .flatten();
                Ok(concrete.or(count).or_else(|| hwb_channel_err(&channels, is_hwb)))
            })
            .transpose()?
            .flatten();
        match verdict {
            Some(e) => Err(e),
            // Shape/type errors above already claimed the concrete forms; what
            // remains (a clean list, symbolic channels, a wrong count) keeps the
            // historical builtins dispatch untouched.
            None if rgb_family => self.legacy_dispatch(call, &channels, alpha),
            None if channels.len() == 3 && !verbatim => self.legacy_dispatch(call, &channels, alpha),
            None => {
                self.symbolic_color_tail(call, &channels, alpha, verbatim_items, alpha_via_pair, arg0)
            }
        }
    }

    /// hsl-family tail: a symbolic comma-serializable slot re-emits the legacy
    /// COMMA form; anything else renders verbatim with a pair alpha keeping
    /// the literal ` / ` slash.
    fn symbolic_color_tail<'a>(
        &mut self,
        call: &FunctionCall,
        channels: &[&'a Expr],
        alpha: Option<&'a Expr>,
        verbatim_items: Option<&'a [Expr]>,
        alpha_via_pair: bool,
        arg0: &'a Expr,
    ) -> Ev<Option<SassValue>> {
        // A symbolic hsl/hsla slot (calc/var/env/attr) re-emits the call in
        // the legacy COMMA form — `hsl(1, calc(2%), 3%, 0.4)`
        // (`hsl/one_arg/special_functions/*`). `from` anchors and `/`-product
        // slots are not leaf-ok, so the verbatim slash form below keeps them.
        let comma = (channels.len() == 3)
            .then(|| self.try_comma_symbolic_color(call, channels, alpha))
            .transpose()?
            .flatten();
        match comma {
            Some(text) => Ok(Some(SassValue::Str { text, quoted: false })),
            None => {
                // Symbolic / relative colors render verbatim with the alpha slash
                // kept literal (`hwb(0 30% var(--c)/0.5)`). The hsl family only.
                let items = verbatim_items.unwrap_or(std::slice::from_ref(arg0));
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        Expr::Binary {
                            op: BinOp::Div,
                            left,
                            right,
                            ..
                        } => Ok(format!(
                            "{}/{}",
                            self.render_css(left)?,
                            self.render_css(right)?
                        )),
                        other => self.render_css(other),
                    })
                    .collect::<Ev<Vec<String>>>()?;
                let text = parts.join(" ");
                let text = match (alpha_via_pair, alpha) {
                    (true, Some(a)) => format!("{text} / {}", self.render_css(a)?),
                    _ => text,
                };
                Ok(Some(SassValue::Str {
                    text: format!("{}({})", call.name, text),
                    quoted: false,
                }))
            }
        }
    }

    /// positional builtins (which keep the goldens' serialization forms),
    /// normalizing a percentage alpha to `0..=1`.
    pub(super) fn legacy_dispatch(
        &mut self,
        call: &FunctionCall,
        channels: &[&Expr],
        alpha: Option<&Expr>,
    ) -> Ev<Option<SassValue>> {
        // percentage alpha normalizes to `0..=1` on its way into the slots
        let pct = |v: SassValue| match v {
            SassValue::Number {
                value,
                unit: Some(ref u),
            } if u == "%" => SassValue::Number {
                value: value / 100.0,
                unit: None,
            },
            other => other,
        };
        let mut positional: Vec<SassValue> = channels
            .iter()
            .map(|e| self.eval_expr(e))
            .collect::<Ev<Vec<SassValue>>>()?;
        positional.extend(alpha.map(|a| self.eval_expr(a).map(pct)).transpose()?);
        let named = std::collections::HashMap::new();
        // A base name with no global builtin (e.g. bare `hwb`, which only
        // exists known-spelling) falls through to the Color4 literal /
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
    // Tuple-`match` classification (no else-if chain): the stray-keyword guard only
    // matters on the named-only branch, so it folds into the first predicate.
    let sorted_named = raw.positional().is_empty()
        && !raw.named().is_empty()
        && !raw.named().keys().any(|k| PARAMS.contains(&k.as_str()));
    let lone_number = raw.named().is_empty()
        && raw.positional().len() == 1
        && matches!(&raw.positional()[0], SassValue::Number { .. })
        && name != "alpha";
    let parts: Vec<String> = match (sorted_named, lone_number) {
        (true, _) => {
            let mut keys: Vec<&String> = raw.named().keys().collect();
            keys.sort();
            Some(
                keys.into_iter()
                    .map(|k| format!("{k}={}", raw.named()[k]))
                    .collect(),
            )
        }
        (false, true) => Some(vec![raw.positional()[0].to_css()]),
        _ => None,
    }?;
    Some(SassValue::Str {
        text: format!("{name}({})", parts.join(", ")),
        quoted: false,
    })
}
