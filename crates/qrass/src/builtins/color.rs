//! `color.*` builtins (task 8.2): channel access (`color.channel`, used by
//! element-plus `roundColor`) plus construction and adjustment helpers
//! (`rgb`/`hsl`/`mix`/`adjust`/`scale`/`change`/`alpha`/`opacity`/`ie-hex-str`).
//! Colors are RGBA f64 with on-demand HSL conversion (D2).

use super::{Or, Registry};
use super::color_ops::to_space;
use crate::error::{ErrorKind, SassError};
use crate::value::{Color, SassValue};

pub(crate) fn register(r: &mut Registry) {
    // `nsp` declares the leading `$color` parameter name so named calls
    // (`color.channel($color: …, $channel: …)`, `color.scale($color: …,
    // $lightness: …)`) fold `$color` into positional slot 0. The channel kwargs
    // (`$red`/`$lightness`/…) are *not* declared: for these variadic setters
    // (`max == None`) `fold_named` leaves unknown names in `Call::named`, where
    // `amount`/`c.opt` still read them.
    r.nsp(
        "color",
        "channel",
        2,
        Some(3),
        &["color", "channel", "space"],
        channel,
    );
    r.nsp("color", "space", 1, Some(1), &["color"], space);
    r.nsp("color", "is-missing", 2, Some(2), &["color", "channel"], is_missing);
    r.nsp("color", "to-space", 1, None, &["color", "space"], to_space);
    r.nsp(
        "color",
        "is-in-gamut",
        1,
        Some(2),
        &["color", "space"],
        super::color_gamut::is_in_gamut,
    );
    r.nsp(
        "color",
        "to-gamut",
        1,
        Some(3),
        &["color", "space", "method"],
        super::color_gamut::to_gamut,
    );
    r.nsp("color", "adjust", 1, None, &["color"], |c| transform(c, Mode::Adjust));
    r.nsp("color", "scale", 1, None, &["color"], |c| transform(c, Mode::Scale));
    r.nsp("color", "change", 1, None, &["color"], |c| transform(c, Mode::Change));
    // Legacy global spellings.
    r.def("adjust-color", 1, None, |c| transform(c, Mode::Adjust));
    r.def("scale-color", 1, None, |c| transform(c, Mode::Scale));
    r.def("change-color", 1, None, |c| transform(c, Mode::Change));
    // Legacy constructors / mix / alpha / ie-hex-str live in `color_ops`.
    super::color_ops::register(r);
}

pub(super) fn err(msg: impl Into<String>) -> SassError {
    SassError::new(ErrorKind::Builtins, msg)
}

pub(super) fn color_at(c: &super::Call, i: usize) -> Or<Color> {
    match c.positional.get(i) {
        Some(SassValue::Color(col)) => Ok(col.clone()),
        Some(other) => Err(err(format!(
            "$color: {} is not a color.",
            other.to_css()
        ))),
        None => Err(err("expected a color")),
    }
}

/// `color.space($color)`: the Color 4 space name, or `rgb` for legacy colors.
fn space(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let name = col.c4.as_ref().map(|c4| c4.space.name).unwrap_or("rgb");
    Ok(SassValue::Str { text: name.to_string(), quoted: false })
}

/// `color.is-missing($color, $channel)`: true only for a `none` Color 4 channel.
fn is_missing(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let name = match c.positional.get(1) {
        Some(SassValue::Str { text, .. }) => text.clone(),
        Some(other) => {
            return Err(err(format!("expected a string, given {}", other.type_name())));
        }
        None => return Err(err("expected a string")),
    };
    let missing = match &col.c4 {
        Some(c4) => {
            if name == "alpha" {
                c4.alpha_missing
            } else {
                match c4.space.channels.iter().position(|ch| ch.name == name) {
                    Some(idx) => c4.comps[idx].is_none(),
                    None => return Err(err(format!("unknown color channel \"{name}\""))),
                }
            }
        }
        // Legacy colors always carry concrete channels (powerless components are
        // still serialized with values).
        None => false,
    };
    Ok(SassValue::Bool(missing))
}

/// A numeric-valued argument (named or default-free) with its unit, if any.
fn amount(c: &super::Call, key: &str) -> Option<(f64, Option<String>)> {
    c.named
        .get(key)
        .and_then(|v| v.as_number().map(|(n, u)| (n, u.map(str::to_string))))
}

pub(super) fn make(col: Color) -> SassValue {
    SassValue::Color(col)
}

/// `color.channel($color, $channel, $space: rgb)`: read one channel value.
fn channel(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let name = match c.positional.get(1) {
        Some(SassValue::Str { text, .. }) => text.clone(),
        Some(other) => other.to_css(),
        None => return Err(err("expected a channel name")),
    };
    let space = c.named.get("space").map(|v| v.to_css());
    read_channel(&col, &name, space.as_deref())
}

/// Core single-channel read for a color. `space` is the resolved `$space`
/// argument (`None` → the color's own space for a Color 4 color, or the
/// historical RGB/HSL/HWB-from-RGB behaviour for a legacy color). Shared by
/// `color.channel` and the deprecated single-channel accessors (`color.red`,
/// `color.blackness`, …) which pass no explicit space.
pub(super) fn read_channel(col: &Color, name: &str, space: Option<&str>) -> Or<SassValue> {
    // The *resolved* space string used for the legacy fallback and the promotion
    // decision (`None` behaves as `rgb`); the `Option` itself is retained below
    // to distinguish "no explicit space" from an explicit `$space: rgb`.
    let resolved = space.unwrap_or("rgb");
    // Color 4 colors read channels through the requested space (`$space` defaults
    // to the color's own space; `rgb` maps to `srgb`).
    //
    // A legacy color carries no space (`c4 == None`). An explicit `$space` other
    // than `rgb` promotes its sRGB display channels (0..255 → 0..1) into a Color4
    // so the conversion path can read them; an absent `$space` (or `rgb`) keeps
    // the historical 0..255 / HSL behaviour below.
    let mut promoted: Option<crate::color4::Color4> = None;
    if col.c4.is_none() && resolved != "rgb" && crate::color4::lookup(resolved).is_some() {
        let mut c4 = crate::color4::Color4::new(
            crate::color4::lookup("srgb").expect("srgb space registered"),
            [col.r / 255.0, col.g / 255.0, col.b / 255.0],
        );
        if col.a != 1.0 {
            c4.alpha = Some(col.a);
        }
        promoted = Some(c4);
    }
    let c4 = col.c4.as_deref().or(promoted.as_ref());
    if let Some(c4) = c4 {
        let own = c4.space.name;
        let target_name: String = match space {
            None => own.to_string(),
            Some(s) => {
                if s == "rgb" { "srgb".to_string() } else { s.to_string() }
            }
        };
        let Some(target) = crate::color4::lookup(&target_name) else {
            return Err(err(format!("Unknown color space: {target_name}")));
        };
        let borrowed;
        let src = if target.name == own {
            c4
        } else {
            borrowed = c4.convert(target);
            &borrowed
        };
        if name == "alpha" {
            return Ok(SassValue::Number { value: src.alpha(), unit: None });
        }
        let idx = src.space.channels.iter().position(|ch| ch.name == name);
        let Some(idx) = idx else {
            return Err(err(format!("Invalid channel name: {name}")));
        };
        let Some(v) = src.comps[idx] else {
            return Ok(SassValue::Null); // missing channels read as null
        };
        let unit = match src.space.channels[idx].unit {
            crate::color4::Unit::Pct => Some("%".into()),
            crate::color4::Unit::Deg => Some("deg".into()),
            crate::color4::Unit::Num => None,
        };
        return Ok(SassValue::Number { value: v, unit });
    }
    let hsl = if matches!(resolved, "hsl" | "hwb") || is_hsl_channel(name) {
        Some(col.to_hsl())
    } else {
        None
    };
    // Legacy `whiteness`/`blackness` recover exactly from the RGB extremes: HWB
    // mixes the pure hue toward white by `W` and toward black by `B`, so `W` is
    // the smallest display channel and `B` is the shortfall below the largest.
    let mn = col.r.min(col.g).min(col.b);
    let mx = col.r.max(col.g).max(col.b);
    let (value, unit) = match name {
        "red" => (col.r, None),
        "green" => (col.g, None),
        "blue" => (col.b, None),
        "alpha" => (col.a, None),
        "hue" => (hsl.map(|h| h.h).unwrap_or(0.0), Some("deg".into())),
        "saturation" => (hsl.map(|h| h.s).unwrap_or(0.0), Some("%".into())),
        "lightness" => (hsl.map(|h| h.l).unwrap_or(0.0), Some("%".into())),
        "whiteness" => (mn / 255.0 * 100.0, Some("%".into())),
        "blackness" => ((255.0 - mx) / 255.0 * 100.0, Some("%".into())),
        other => return Err(err(format!("unknown color channel \"{other}\""))),
    };
    Ok(SassValue::Number { value, unit })
}

fn is_hsl_channel(name: &str) -> bool {
    matches!(name, "hue" | "saturation" | "lightness")
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Adjust,
    Scale,
    Change,
}

/// Apply `hue`/`saturation`/`lightness`/`red`/`green`/`blue`/`alpha` arguments in
/// the given mode (absolute delta, relative scaling, or absolute set).
fn transform(c: &super::Call, mode: Mode) -> Or<SassValue> {
    let base = color_at(c, 0)?;
    if base.c4.is_some() {
        return transform_c4(&base, c, mode);
    }
    let mut col = base.clone();
    // Editing any channel invalidates the cached *keyword* identity (a named
    // color keeps `name: Some(..)`, which serialization would otherwise prefer
    // over the edited numeric channels) and routes the result through the
    // goldens' legacy form: integer channels re-spell as keyword/hex,
    // fractional in-gamut ones stay `rgb(%…)` (`scale/rgb` `red/low`), and
    // out-of-range ones fall back to the `hsl(…)` expression (`change/rgb`
    // `out_of_range`). Only bases actually edited here take the tag; HSL- and
    // alpha-edited colors keep their existing serialization untouched.
    let mut rgb_edited = false;
    // RGB channels (independent of the HSL round-trip). The legacy channel
    // domain is 0..255 and is *not* clamped here: out-of-range edits survive in
    // the color and `legacy_to_css` re-emits them as the goldens' HSL
    // expression (`change/rgb` `out_of_range`), while a `none` edit makes the
    // whole color serialize as its Color-4 `rgb(…)` spelling (`red/none`).
    let mut missing: Option<usize> = None;
    for (i, key) in ["red", "green", "blue"].iter().enumerate() {
        let Some(v) = c.named.get(*key) else { continue };
        match v {
            SassValue::Number { value, .. } => {
                rgb_edited = true;
                let val = *value;
                match i {
                    0 => col.r = edit(base.r, val, mode, 0.0, 255.0),
                    1 => col.g = edit(base.g, val, mode, 0.0, 255.0),
                    _ => col.b = edit(base.b, val, mode, 0.0, 255.0),
                }
            }
            SassValue::Str { text, .. } if text == "none" && matches!(mode, Mode::Change) => {
                missing = Some(i);
            }
            other => {
                return Err(err(format!(
                    "${key}: {} is not a number.",
                    other.to_css()
                )));
            }
        }
    }
    if let Some(i) = missing {
        let show = |v: f64| crate::value_number::format_number(v);
        let ch = [
            if i == 0 { "none".into() } else { show(col.r) },
            if i == 1 { "none".into() } else { show(col.g) },
            if i == 2 { "none".into() } else { show(col.b) },
        ];
        return Ok(SassValue::Str {
            text: format!("rgb({} {} {})", ch[0], ch[1], ch[2]),
            quoted: false,
        });
    }
    if rgb_edited {
        col.name = None;
        if col.legacy.is_none() {
            col.legacy = Some("rgb");
        }
    }
    // The alpha edit intentionally does NOT retag: alpha-only edits keep the
    // base's existing serialization path (`adjust/space` none families).
    if let Some((v, _)) = amount(c, "alpha") {
        col.a = match mode {
            Mode::Change => v,
            Mode::Adjust => base.a + v,
            Mode::Scale => {
                if v >= 0.0 {
                    base.a + (1.0 - base.a) * (v / 100.0)
                } else {
                    base.a * (1.0 + v / 100.0)
                }
            }
        }
        .clamp(0.0, 1.0);
    }

    // HSL channels require a round-trip through the HSL space.
    let hsl_edits = ["hue", "saturation", "lightness"]
        .iter()
        .any(|k| amount(c, k).is_some());
    if hsl_edits {
        let mut hsl = col.to_hsl();
        if let Some((v, _)) = amount(c, "hue") {
            hsl.h = match mode {
                Mode::Change => v,
                _ => hsl.h + v,
            };
        }
        if let Some((v, _)) = amount(c, "saturation") {
            hsl.s = hsl_channel_set(hsl.s, v, mode);
        }
        if let Some((v, _)) = amount(c, "lightness") {
            hsl.l = hsl_channel_set(hsl.l, v, mode);
        }
        col = Color::from_hsla(hsl.h, hsl.s, hsl.l, col.a);
        // The goldens' shape ladder for HSL edits (`change/hsl`, `adjust/hsl`):
        // an edited triple outside lightness [0,100] or above saturation 100
        // emits the verbatim `hsl(h, s%, l%)` expression (the RGB round-trip
        // would clamp it away); in-range results serialize through the legacy
        // rgb form — integer channels as keyword/hex, fractional ones keeping
        // `rgb(%…)` (negative saturations clamp to 0 inside `from_hsla`,
        // matching `saturation/below_range`, and scale edits stay bounded).
        col.name = None;
        if hsl.l < -1e-4 || hsl.l > 100.0001 || hsl.s > 100.0001 {
            col.legacy = Some("hsl");
            col.legacy_hsl = Some(hsl);
        } else {
            col.legacy = Some("rgb");
        }
        return Ok(make(col));
    }
    Ok(make(col))
}

/// Saturation/lightness edits for the legacy transform: `change` sets and
/// `adjust` accumulates *without* clamping (the out-of-range goldens serialize
/// the excursion through the carried `hsl(…)` fallback), while `scale` keeps
/// the bounded interpretation.
fn hsl_channel_set(cur: f64, v: f64, mode: Mode) -> f64 {
    match mode {
        Mode::Change => v,
        Mode::Adjust => cur + v,
        Mode::Scale => edit(cur, v, mode, 0.0, 100.0),
    }
}

/// Color 4 channel editing: the named channels of the color's own space. gamut
/// channels are *not* clamped (out-of-range values are legal, matching the
/// `above_max`/`below_min` spec cases); hues wrap; alpha clamps to `0..=1`.
fn transform_c4(base: &Color, c: &super::Call, mode: Mode) -> Or<SassValue> {
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
    for (key, val) in c.named.iter() {
        if key == "alpha" {
            continue;
        }
        let Some((v, unit)) = val.as_number() else {
            // `change($channel: none)` writes the missing slot on a Color 4
            // color (`change/hsl` `hue/none`: `hsl(none 100% 50%)`).
            if matches!(val, SassValue::Str { text, .. } if text == "none")
                && matches!(mode, Mode::Change)
                && let Some(idx) = c4.space.channels.iter().position(|ch| ch.name == *key) {
                c4.comps[idx] = None;
            }
            continue;
        };
        let Some(idx) = c4.space.channels.iter().position(|ch| ch.name == *key) else {
            if LEGACY.contains(&key.as_str()) {
                return Err(err(format!("{} color has no {key} channel", c4.space.name)));
            }
            continue;
        };
        let ch = &c4.space.channels[idx];
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
        c4.comps[idx] = Some(if ch.unit == crate::color4::Unit::Deg {
            (next % 360.0 + 360.0) % 360.0
        } else {
            next
        });
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

/// Combine the current value `cur` with edit `v` in `[lo, hi]` by mode.
///
/// `change` stores the requested value verbatim: the goldens let a legacy color
/// keep out-of-gamut channels (e.g. `change(rgb(0,100,0), $red: -50)` → the red
/// stays `-50`), deferring any clamping to serialization. `adjust` and `scale`
/// stay bounded to `[lo, hi]` as before.
fn edit(cur: f64, v: f64, mode: Mode, lo: f64, hi: f64) -> f64 {
    let out = match mode {
        Mode::Change => return v,
        Mode::Adjust => cur + v,
        Mode::Scale => {
            if v >= 0.0 {
                cur + (hi - cur) * (v / 100.0)
            } else {
                cur + (cur - lo) * (v / 100.0)
            }
        }
    };
    out.clamp(lo, hi)
}

