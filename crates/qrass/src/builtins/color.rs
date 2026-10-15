//! `color.*` builtins (task 8.2): channel access (`color.channel`, used by
//! element-plus `roundColor`) plus construction and adjustment helpers
//! (`rgb`/`hsl`/`mix`/`adjust`/`scale`/`change`/`alpha`/`opacity`/`ie-hex-str`).
//! Colors are RGBA f64 with on-demand HSL conversion (D2).

use super::{Or, Registry};
use crate::error::{ErrorKind, SassError};
use crate::value::{Color, SassValue};

pub(crate) fn register(r: &mut Registry) {
    r.ns("color", "channel", 2, Some(2), channel);
    r.ns("color", "adjust", 1, None, |c| transform(c, Mode::Adjust));
    r.ns("color", "scale", 1, None, |c| transform(c, Mode::Scale));
    r.ns("color", "change", 1, None, |c| transform(c, Mode::Change));
    r.ns("color", "mix", 2, Some(3), mix);
    r.ns("color", "alpha", 1, Some(1), alpha);
    r.ns("color", "opacity", 1, Some(1), alpha);
    r.ns("color", "ie-hex-str", 1, Some(1), ie_hex_str);
    r.ns("color", "rgb", 1, Some(3), rgb);
    r.ns("color", "hsl", 1, Some(3), hsl);
    // Legacy global spellings.
    r.def("adjust-color", 1, None, |c| transform(c, Mode::Adjust));
    r.def("scale-color", 1, None, |c| transform(c, Mode::Scale));
    r.def("change-color", 1, None, |c| transform(c, Mode::Change));
    r.def("ie-hex-str", 1, Some(1), ie_hex_str);
    // Legacy global color constructors element-plus leans on (`rgba($c, $a)`,
    // `rgb(r, g, b)` / `rgba(r, g, b, $a)`); both spellings share one impl.
    r.def("rgb", 1, Some(4), rgb);
    r.def("rgba", 1, Some(4), rgb);
}

fn err(msg: impl Into<String>) -> SassError {
    SassError::new(ErrorKind::Builtins, msg)
}

fn color_at(c: &super::Call, i: usize) -> Or<Color> {
    match c.positional.get(i) {
        Some(SassValue::Color(col)) => Ok(col.clone()),
        Some(other) => Err(err(format!(
            "expected a color, given {}",
            other.type_name()
        ))),
        None => Err(err("expected a color")),
    }
}

/// A numeric-valued argument (named or default-free) with its unit, if any.
fn amount(c: &super::Call, key: &str) -> Option<(f64, Option<String>)> {
    c.named
        .get(key)
        .and_then(|v| v.as_number().map(|(n, u)| (n, u.map(str::to_string))))
}

fn make(col: Color) -> SassValue {
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
    let space = c
        .named
        .get("space")
        .map(|v| v.to_css())
        .unwrap_or_else(|| "rgb".into());
    let hsl = if matches!(space.as_str(), "hsl" | "hwb") || is_hsl_channel(&name) {
        Some(col.to_hsl())
    } else {
        None
    };
    let value = match name.as_str() {
        "red" => col.r,
        "green" => col.g,
        "blue" => col.b,
        "alpha" => col.a,
        "hue" => hsl.map(|h| h.h).unwrap_or(0.0),
        "saturation" => hsl.map(|h| h.s).unwrap_or(0.0),
        "lightness" => hsl.map(|h| h.l).unwrap_or(0.0),
        other => return Err(err(format!("unknown color channel \"{other}\""))),
    };
    let unit = match name.as_str() {
        "alpha" => None,
        "hue" => Some("deg".into()),
        "saturation" | "lightness" => Some("%".into()),
        _ => None,
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

    // RGB channels (independent of the HSL round-trip).
    if let Some((v, _)) = amount(c, "red") {
        col.r = edit(base.r, v, mode, 0.0, 255.0);
    }
    if let Some((v, _)) = amount(c, "green") {
        col.g = edit(base.g, v, mode, 0.0, 255.0);
    }
    if let Some((v, _)) = amount(c, "blue") {
        col.b = edit(base.b, v, mode, 0.0, 255.0);
    }
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
            hsl.s = edit(hsl.s, v, mode, 0.0, 100.0);
        }
        if let Some((v, _)) = amount(c, "lightness") {
            hsl.l = edit(hsl.l, v, mode, 0.0, 100.0);
        }
        col = Color::from_hsla(hsl.h, hsl.s, hsl.l, col.a);
    }
    Ok(make(col))
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
        let Some((v, unit)) = val.as_number() else { continue };
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
    }))
}

/// Combine the current value `cur` with edit `v` in `[lo, hi]` by mode.
fn edit(cur: f64, v: f64, mode: Mode, lo: f64, hi: f64) -> f64 {
    let out = match mode {
        Mode::Change => v,
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

/// `color.mix($c1, $c2, $weight: 50%)`: linear RGB blend by `pct` weight of `$c1`.
fn mix(c: &super::Call) -> Or<SassValue> {
    let a = color_at(c, 0)?;
    let b = color_at(c, 1)?;
    let weight = c
        .opt("weight", Some(2))
        .map(|v| v.to_css())
        .unwrap_or_default();
    let p = parse_percent(&weight, 50.0) / 100.0;
    let ch = |x: f64, y: f64| x * p + y * (1.0 - p);
    Ok(make(Color {
        r: ch(a.r, b.r),
        g: ch(a.g, b.g),
        b: ch(a.b, b.b),
        a: ch(a.a, b.a),
        // dart-sass inherits the first operand's representation form.
        hex: a.hex,
        c4: None,
    }))
}

fn parse_percent(text: &str, default: f64) -> f64 {
    let trimmed = text.trim().trim_end_matches('%');
    trimmed.parse::<f64>().unwrap_or(default)
}

fn alpha(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    Ok(SassValue::Number {
        value: col.a,
        unit: None,
    })
}

fn ie_hex_str(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let hex = |v: f64| format!("{:02X}", v.round().clamp(0.0, 255.0) as u8);
    Ok(SassValue::Str {
        text: format!(
            "#{}{}{}{}",
            hex(col.a * 255.0),
            hex(col.r),
            hex(col.g),
            hex(col.b)
        ),
        quoted: false,
    })
}

fn rgb(c: &super::Call) -> Or<SassValue> {
    // `rgb/rgba($color, $alpha)` recomputes alpha; `rgb(r, g, b)` /
    // `rgba(r, g, b, $alpha)` build from channels.
    let n = c.positional.len();
    if n == 1 {
        return Ok(make(color_at(c, 0)?));
    }
    if n == 2 {
        let mut col = color_at(c, 0)?;
        col.a = alpha_arg(c, 1)?;
        return Ok(make(col));
    }
    let r = channel_byte(c, 0)?;
    let g = channel_byte(c, 1)?;
    let b = channel_byte(c, 2)?;
    let mut col = Color::rgb(r, g, b);
    if n >= 4 {
        col.a = alpha_arg(c, 3)?;
    }
    Ok(make(col))
}

/// Read an alpha argument, accepting a `0..=1` number or a percentage.
fn alpha_arg(c: &super::Call, i: usize) -> Or<f64> {
    let (v, unit) = c
        .positional
        .get(i)
        .and_then(|a| a.as_number())
        .ok_or_else(|| err("expected a number for alpha"))?;
    Ok(if unit == Some("%") { v / 100.0 } else { v })
}

fn hsl(c: &super::Call) -> Or<SassValue> {
    if c.positional.len() == 1 {
        return Ok(make(color_at(c, 0)?));
    }
    let h = number_arg(c, 0)?;
    let s = number_arg(c, 1)?;
    let l = number_arg(c, 2)?;
    Ok(make(Color::from_hsl(h, s, l)))
}

fn channel_byte(c: &super::Call, i: usize) -> Or<u8> {
    Ok(number_arg(c, i)?.round().clamp(0.0, 255.0) as u8)
}

fn number_arg(c: &super::Call, i: usize) -> Or<f64> {
    match c.positional.get(i) {
        Some(SassValue::Number { value, .. }) => Ok(*value),
        Some(other) => Err(err(format!(
            "expected a number, given {}",
            other.type_name()
        ))),
        None => Err(err("expected a number")),
    }
}
