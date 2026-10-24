//! Color-4 channel editing for the `adjust`/`scale`/`change` trio — a
//! sibling of `builtins/color.rs` split for the 500-line budget
//! (round 85). Gamut channels are *not* clamped (out-of-range values are
//! legal, matching the `above_max`/`below_min` spec cases); hues wrap; alpha
//! clamps to 0..=1. The validation chain (domain names, types, the scale
//! unit/bound ladder, the missing-channel guard, `calc(nan)` no-ops) is read
//! off `scale/error/*` + `adjust/error/*` + `change/unusual_numbers`.

use std::collections::HashMap;

use rxrust::prelude::*;

use super::color::{Mode, err, make};
use super::color::amount;
use super::Or;
use crate::drain::drain_one;
use crate::error::SassError;
use crate::value::{Color, SassValue};

/// Color 4 channel editing: the named channels of the color's own space. gamut
/// channels are *not* clamped (out-of-range values are legal, matching the
/// `above_max`/`below_min` spec cases); hues wrap; alpha clamps to `0..=1`.
pub(super) fn transform_c4(base: &Color, c: &super::Call, mode: Mode) -> Or<SassValue> {
    if c.positional.len() > 1 {
        return Err(err(
            "Only one positional argument is allowed. All other arguments must be passed by name.",
        ));
    }
    let c4 = base.c4.as_ref().expect("checked by caller").as_ref().clone();
    // Channel edits bind one space (the color's own); iteration is sorted so
    // multi-channel calls report the same name the goldens do
    // (`scale/error/*` families). Folded as an rxrust `reduce_initial` stream
    // (task 5.5/C5, R4 zero-exemption): the accumulator carries the owned
    // `Color4` plus a latched first error, and `drain_one` runs it inline. The
    // first error stops applying further edits (the old early `return`); a
    // skipped key (the old `continue`) is an unchanged `(c4, None)` step.
    let named = c.named.clone();
    let base_css = base.c4.as_ref().expect("c4 base").to_css();
    let mut keys: Vec<&String> = named.keys().collect();
    keys.sort();
    let (mut c4, channel_err) = drain_one(
        Local::from_iter(keys.into_iter().cloned().collect::<Vec<String>>())
            .reduce_initial(
                (c4, None::<SassError>),
                move |(c4, err), key| {
                    if err.is_some() {
                        return (c4, err);
                    }
                    apply_channel_edit(c4, &key, &named, mode, &base_css)
                },
            )
            .map(Ok::<_, SassError>)
            .box_it(),
    );
    if let Some(e) = channel_err {
        return Err(e);
    }
    if let Some(a) = c.named.get("alpha") {
        // `$alpha` must be a number — even `none` is rejected here
        // (`scale/error/type` `none/error`: `$alpha: none is not a number.`).
        let Some((v, _)) = a.as_number() else {
            return Err(err(format!(
                "$alpha: {} is not a number.",
                a.to_css()
            )));
        };
        let _ = v;
        if mode == Mode::Scale && c4.alpha_missing {
            return Err(err(
                "$alpha: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently support modifying missing channels (color: {})".to_string()
                    + &base.c4.as_ref().expect("c4 base").to_css() + ").",
            ));
        }
    }
    if let Some((v, _)) = amount(c, "alpha") {
        c4.alpha_missing = false;
        c4.alpha = Some(
            match mode {
                Mode::Change => v,
                Mode::Adjust => c4.alpha() + v,
                Mode::Scale => {
                    if v >= 0.0 {
                        c4.alpha() + (1.0 - c4.alpha()) * (v / 100.0)
                    } else {
                        c4.alpha() * (1.0 + v / 100.0)
                    }
                }
            }
            .clamp(0.0, 1.0),
        );
    }
    let [r, g, b] = c4.to_rgb();
    let a = c4.alpha();
    Ok(make(Color {
        r,
        g,
        b,
        a,
        hex: false,
        c4: Some(Box::new(c4)),
        name: None,
        legacy: None,
            legacy_hsl: None,
    }))
}

/// One channel edit for one named key — the `reduce_initial` fold step that
/// carries the running `Color4` plus a latched error. The old loop body's
/// `return Err(..)` becomes `(c4, Some(..))`, its `continue` becomes
/// `(c4, None)`; short-circuit order and every golden message are unchanged.
fn apply_channel_edit(
    mut c4: crate::color4::Color4,
    key: &str,
    named: &HashMap<String, SassValue>,
    mode: Mode,
    base_css: &str,
) -> (crate::color4::Color4, Option<SassError>) {
    const LEGACY: [&str; 8] = [
        "red",
        "green",
        "blue",
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
    ];
    if key == "alpha" {
        return (c4, None);
    }
    if key == "space" {
        // `$space:` selects the edited space; only an unquoted known/legacy
        // name is accepted (round-84 `scale/error/space`).
        if let Some(SassValue::Str { text, quoted: true }) = named.get("space") {
            return (
                c4,
                Some(err(format!(
                    "$space: Expected \"{text}\" to be an unquoted string."
                ))),
            );
        }
        return (c4, None);
    }
    let val = &named[key];
    let Some(idx) = c4.space.channels.iter().position(|ch| ch.name == key) else {
        if LEGACY.contains(&key) {
            let msg = format!(
                "${key}: Color space {} doesn't have a channel with this name.",
                c4.space.name
            );
            return (c4, Some(err(msg)));
        }
        return (c4, None);
    };
    let ch = &c4.space.channels[idx];
    let (v, unit) = match val.as_number() {
        Some(vu) => vu,
        None => {
            // The non-number value is classified once (three golden classes) and
            // routed by a single `match` — the step carries no else-if chain.
            return match classify_non_number(val, mode) {
                NonNumber::Missing => {
                    c4.comps[idx] = None;
                    (c4, None)
                }
                NonNumber::Calc { write: Some(text) } => {
                    c4.comps[idx] = Some(0.0);
                    c4.text[idx] = Some(text.trim().to_string());
                    (c4, None)
                }
                NonNumber::Calc { write: None } => (c4, None),
                NonNumber::NotNumber => (
                    c4,
                    Some(err(format!("${key}: {} is not a number.", val.to_css()))),
                ),
            };
        }
    };
    let err_or_none = if mode == Mode::Scale {
        match (
            ch.name == "hue",
            unit != Some("%"),
            v < -100.0 || v > 100.0,
        ) {
            (true, _, _) => Some(err("$hue: Channel isn't scalable.")),
            (false, true, _) => Some(err(format!(
                "${key}: Expected {} to have unit \"%\".",
                val.to_css()
            ))),
            (false, false, true) => Some(err(format!(
                "${key}: Expected {} to be within -100% and 100%.",
                val.to_css()
            ))),
            (false, false, false) => None,
        }
    } else {
        // Adjust/change unit ladder per channel unit (`adjust/error/units/*`):
        // percent-only channels (w/b/s/l) reject other units; numeric channels
        // accept `%` or unitless; hue accepts angle units.
        match ch.unit {
            crate::color4::Unit::Pct => {
                if unit.is_some() && unit != Some("%") {
                    Some(err(format!(
                        "${key}: Expected {} to have unit \"%\".",
                        val.to_css()
                    )))
                } else {
                    None
                }
            }
            crate::color4::Unit::Deg => {
                if !matches!(
                    unit,
                    None | Some("deg") | Some("grad") | Some("rad") | Some("turn")
                ) {
                    Some(err(format!(
                        "${key}: Expected {} to have an angle unit (deg, grad, rad, turn).",
                        val.to_css()
                    )))
                } else {
                    None
                }
            }
            crate::color4::Unit::Num => {
                if !matches!(unit, None | Some("%")) {
                    Some(err(format!(
                        "${key}: Expected {} to have unit \"%\" or no units.",
                        val.to_css()
                    )))
                } else {
                    None
                }
            }
        }
    };
    if let Some(e) = err_or_none {
        return (c4, Some(e));
    }
    if mode == Mode::Scale && c4.comps[idx].is_none() {
        // Scaling a MISSING channel is the goldens' unresolved-behavior guard
        // (`scale/error/missing`); adjust floors at the channel minimum
        // (`change/unusual_numbers` `powerless_none_adjusts`) and change
        // overwrites unconditionally.
        return (c4, Some(err(format!(
            "${key}: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently support modifying missing channels (color: {}).",
            base_css
        ))));
    }
    let pct = unit == Some("%");
    let cur = c4.comps[idx].unwrap_or(ch.lo);
    let next = match mode {
        Mode::Change => {
            if pct && ch.unit == crate::color4::Unit::Num {
                v / 100.0
            } else {
                v
            }
        }
        Mode::Adjust => {
            let delta = if pct && ch.unit == crate::color4::Unit::Num {
                v / 100.0 * (ch.hi - ch.lo)
            } else {
                v
            };
            cur + delta
        }
        Mode::Scale => {
            if v >= 0.0 {
                cur + (ch.hi - cur) * (v / 100.0)
            } else {
                cur + (cur - ch.lo) * (v / 100.0)
            }
        }
    };
    let deg_space = (
        ch.unit == crate::color4::Unit::Deg,
        mode == Mode::Adjust && ch.name == "lightness",
    );
    c4.comps[idx] = Some(match deg_space {
        (true, _) => (next % 360.0 + 360.0) % 360.0,
        // `adjust(lab(30% …), $lightness: 120%)` → `lab(100% 60 -100)`
        // (`color/adjust/{lab,lch,oklab,oklch}` `lightness/arg_above_max`):
        // adjust clamps the resulting lightness into the channel bounds;
        // `change` overflows into the lossy mix form instead.
        (false, true) => next.clamp(ch.lo, ch.hi),
        (false, false) => next,
    });
    (c4, None)
}

/// One of the three golden classes a *non-number* `$channel:` edit value falls
/// into (the edit loop routes them with a single `match`).
enum NonNumber<'a> {
    /// `change($channel: none)` writes the missing slot on a Color 4 color
    /// (`change/hsl` `hue/none`: `hsl(none 100% 50%)`)
    Missing,
    /// an unquoted `calc(…)` spelling; `write` is the verbatim *text* channel
    /// slot the goldens demand, or nothing at all
    Calc { write: Option<&'a str> },
    /// anything else — the `${key}: X is not a number.` blame
    NotNumber,
}

/// Classification of a non-number edit value (`scale/error/type` `none/error`,
/// `change/unusual_numbers` `nan/linear`, `infinity/linear`).
fn classify_non_number(val: &SassValue, mode: Mode) -> NonNumber<'_> {
    let calc = match val {
        SassValue::Str { text, quoted: false }
            if text.to_ascii_lowercase().starts_with("calc(") =>
        {
            Some(text.as_str())
        }
        _ => None,
    };
    let none_write = matches!(val, SassValue::Str { text, .. } if text == "none")
        && matches!(mode, Mode::Change);
    // `calc(NaN)` edits are a golden no-op (value preserved); a degenerate
    // non-nan text writes its verbatim slot, and only under `change`.
    let write = calc.filter(|text| {
        matches!(mode, Mode::Change)
            && crate::color4::degenerate_channel_text(text)
            && !text.to_ascii_lowercase().replace(" ", "").contains("nan")
    });
    match (none_write, calc, write) {
        (true, _, _) => NonNumber::Missing,
        (false, Some(_), Some(text)) => NonNumber::Calc { write: Some(text) },
        (false, Some(_), None) => NonNumber::Calc { write: None },
        _ => NonNumber::NotNumber,
    }
}
