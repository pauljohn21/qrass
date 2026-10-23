//! Color-4 channel editing for the `adjust`/`scale`/`change` trio — a
//! sibling of `builtins/color.rs` split for the 500-line budget
//! (round 85). Gamut channels are *not* clamped (out-of-range values are
//! legal, matching the `above_max`/`below_min` spec cases); hues wrap; alpha
//! clamps to 0..=1. The validation chain (domain names, types, the scale
//! unit/bound ladder, the missing-channel guard, `calc(nan)` no-ops) is read
//! off `scale/error/*` + `adjust/error/*` + `change/unusual_numbers`.

use super::color::{Mode, err, make};
use super::color::amount;
use super::Or;
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
    let mut c4 = base.c4.as_ref().expect("checked by caller").as_ref().clone();
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
    // Channel edits bind one space (the color's own); iteration is sorted so
    // multi-channel calls report the same name the goldens do
    // (`scale/error/*` families).
    let mut keys: Vec<&String> = c.named.keys().collect();
    keys.sort();
    for key in keys {
        if key == "alpha" {
            continue;
        }
        if key == "space" {
            // `$space:` selects the edited space; only an unquoted known/legacy
            // name is accepted (round-84 `scale/error/space`).
            if let Some(SassValue::Str { text, quoted: true }) = c.named.get("space") {
                return Err(err(format!(
                    "$space: Expected \"{text}\" to be an unquoted string."
                )));
            }
            continue;
        }
        let val = &c.named[key];
        let Some(idx) = c4.space.channels.iter().position(|ch| ch.name == *key) else {
            if LEGACY.contains(&key.as_str()) {
                return Err(err(format!(
                    "${key}: Color space {} doesn't have a channel with this name.",
                    c4.space.name
                )));
            }
            continue;
        };
        let ch = &c4.space.channels[idx];
        let Some((v, unit)) = val.as_number() else {
            // `change($channel: none)` writes the missing slot on a Color 4
            // color (`change/hsl` `hue/none`: `hsl(none 100% 50%)`). Other
            // modes blame the non-number (`scale/error/type` `none/error`).
            if matches!(val, SassValue::Str { text, .. } if text == "none")
                && matches!(mode, Mode::Change)
            {
                c4.comps[idx] = None;
            } else if let SassValue::Str { text, quoted: false } = val
                && text.to_ascii_lowercase().starts_with("calc(")
            {
                // `calc(NaN)` edits are a golden no-op (`change/unusual_numbers`
                // `nan/linear`: value preserved); `calc(±infinity)` writes the
                // verbatim *text* channel slot under `change` (`infinity/linear`:
                // `lab(50% calc(infinity) 0)`).
                if matches!(mode, Mode::Change)
                    && crate::color4::degenerate_channel_text(text)
                    && !text.to_ascii_lowercase().replace(" ", "").contains("nan")
                {
                    c4.comps[idx] = Some(0.0);
                    c4.text[idx] = Some(text.trim().to_string());
                }
                continue;
            } else {
                return Err(err(format!(
                    "${key}: {} is not a number.",
                    val.to_css()
                )));
            }
            continue;
        };
        if mode == Mode::Scale {
            if ch.name == "hue" {
                return Err(err("$hue: Channel isn't scalable."));
            }
            if unit != Some("%") {
                return Err(err(format!(
                    "${key}: Expected {} to have unit \"%\".",
                    val.to_css()
                )));
            }
            if v < -100.0 || v > 100.0 {
                return Err(err(format!(
                    "${key}: Expected {} to be within -100% and 100%.",
                    val.to_css()
                )));
            }
        } else {
            // Adjust/change unit ladder per channel unit (`adjust/error/units/*`):
            // percent-only channels (w/b/s/l) reject other units; numeric
            // channels accept `%` or unitless; hue accepts angle units.
            match ch.unit {
                crate::color4::Unit::Pct => {
                    if unit.is_some() && unit != Some("%") {
                        return Err(err(format!(
                            "${key}: Expected {} to have unit \"%\".",
                            val.to_css()
                        )));
                    }
                }
                crate::color4::Unit::Deg => {
                    if !matches!(
                        unit,
                        None | Some("deg") | Some("grad") | Some("rad") | Some("turn")
                    ) {
                        return Err(err(format!(
                            "${key}: Expected {} to have an angle unit (deg, grad, rad, turn).",
                            val.to_css()
                        )));
                    }
                }
                crate::color4::Unit::Num => {
                    if !matches!(unit, None | Some("%")) {
                        return Err(err(format!(
                            "${key}: Expected {} to have unit \"%\" or no units.",
                            val.to_css()
                        )));
                    }
                }
            }
        }
        if mode == Mode::Scale && c4.comps[idx].is_none() {
            // Scaling a MISSING channel is the goldens' unresolved-behavior guard
            // (`scale/error/missing`); adjust floors at the channel minimum
            // (`change/unusual_numbers` `powerless_none_adjusts`, pinned in
            // tests/color4.rs) and change overwrites unconditionally.
            return Err(err(format!(
                "${key}: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently support modifying missing channels (color: {}).",
                base.c4.as_ref().expect("c4 base").to_css()
            )));
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
        let deg_space = (ch.unit == crate::color4::Unit::Deg, mode == Mode::Adjust && ch.name == "lightness");
        c4.comps[idx] = Some(match deg_space {
            (true, _) => (next % 360.0 + 360.0) % 360.0,
            // `adjust(lab(30% …), $lightness: 120%)` → `lab(100% 60 -100)`
            // (`color/adjust/{lab,lch,oklab,oklch}` `lightness/arg_above_max`):
            // adjust clamps the resulting lightness into the channel bounds;
            // `change` overflows into the lossy mix form instead.
            (false, true) => next.clamp(ch.lo, ch.hi),
            (false, false) => next,
        });
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
