//! Legacy color builtins (`color.rgb`/`color.hsl`/`color.mix`/`color.alpha`/
//! `color.opacity`/`color.ie-hex-str` plus their global spellings). Split out of
//! `color.rs` to keep that module within the source-size budget; the shared
//! argument helpers (`err`, `color_at`, `make`) stay there as `pub(super)`.

use super::color::{color_at, err, make, read_channel};
use super::Or;
use crate::value::{Color, Hsl, SassValue};

/// Hue normalization the goldens' legacy color applies on construction.
pub(super) fn wrap_hue(h: Hsl) -> Hsl {
    Hsl { h: ((h.h % 360.0) + 360.0) % 360.0, ..h }
}

/// The verbatim HSL triple the golden computes for an `hsl`/`hwb` Color4 source:
/// an `hsl` source's own channels, or the HWB→HSL inversion. The HWB saturation
/// is *signed*: `s = (100 − b − w) / (100 − |2·lightness − 100|) · 100`, so an
/// out-of-range lightness drives the denominator negative and the channels past
/// gamut (`hwb(20deg 200% -125%)` → `hsl(20, -11.11%, 212.5%)`, which the golden
/// then normalizes on emit by flipping the hue +180° — see `legacy_to_css`).
pub(super) fn hsl_from_legacy_src(c4: &crate::color4::Color4) -> Option<Hsl> {
    let g = |i: usize| c4.comps[i].unwrap_or(0.0);
    match c4.space.name {
        "hsl" => Some(Hsl { h: g(0), s: g(1), l: g(2) }),
        "hwb" => {
            let (w, b) = (g(1), g(2));
            let total = w + b;
            let (w, b) = if total > 100.0 {
                (w / total * 100.0, b / total * 100.0)
            } else {
                (w, b)
            };
            let num = 100.0 - b - w;
            let l = w + num / 2.0;
            let den = 100.0 - (2.0 * l - 100.0).abs();
            let s = if num == 0.0 || den == 0.0 { 0.0 } else { num / den * 100.0 };
            Some(Hsl { h: g(0), s, l })
        }
        _ => None,
    }
}

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
        // the goldens inherit the first operand's representation form.
        hex: a.hex,
        c4: None,
        name: None,
        legacy: None,
            legacy_hsl: None,
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
    // Legacy `rgb()` special-function passthrough: if a channel/alpha argument
    // is an *unresolvable* value (a `calc()`/`var()`/`env()`/`attr()` or an
    // unquoted string) dart does NOT build a Color — it returns the `rgb(…)`
    // call verbatim (comma-joined, keeping the `rgb(` name even with an alpha).
    // `none` is NOT such a value: it resolves to 0 and produces a concrete color
    // (channel_byte maps it), so it must stay on the numeric path or it would
    // regress `rgb(none …)` cases. Currently the unresolvable calls error, so
    // emitting the literal is a pure gain; all-numeric runs the Color path.
    if c.positional.iter().any(|v| !is_channel_value(v)) {
        let inner = c
            .positional
            .iter()
            .map(|v| v.to_css())
            .collect::<Vec<_>>()
            .join(", ");
        return Ok(SassValue::Str {
            text: format!("rgb({inner})"),
            quoted: false,
        });
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

/// `color.hwb($hue, $whiteness, $blackness, $alpha)`: construct a **legacy** color
/// from HWB channels. The math is the pure W3C HWB→HSL inversion (normalize when
/// `w + b > 100`), so the result carries `legacy = "hwb"` plus the verbatim HSL
/// triple; `legacy_to_css` renders integer-byte results as keyword/`#hex` and
/// fractional ones as `hsl(…)`, exactly like the goldens. In-gamut only — no XYZ
/// (a `var()`/symbolic hue fails `hue_deg` and stays an unresolved call, which is
/// the pre-round behavior, so it cannot regress). The bare global `hwb()` is NOT
/// registered: it is a CSS color function whose plain-CSS passthrough must hold.
fn hwb(c: &super::Call) -> Or<SassValue> {
    let h = hue_deg(c, 0)?;
    let w = number_arg(c, 1)?;
    let b = number_arg(c, 2)?;
    let a = if c.positional.len() >= 4 {
        alpha_arg(c, 3)?
    } else {
        1.0
    };
    let total = w + b;
    let (w, b) = if total > 100.0 {
        (w / total * 100.0, b / total * 100.0)
    } else {
        (w, b)
    };
    let num = 100.0 - b - w;
    let l = w + num / 2.0;
    let den = 100.0 - (2.0 * l - 100.0).abs();
    let s = if num == 0.0 || den == 0.0 {
        0.0
    } else {
        num / den * 100.0
    };
    let rgb = Color::from_hsla(h, s, l, a);
    Ok(make(Color {
        legacy: Some("hwb"),
        legacy_hsl: Some(Hsl {
            h: ((h % 360.0) + 360.0) % 360.0,
            s,
            l,
        }),
        ..rgb
    }))
}

/// The hue argument in **degrees**: `rad`/`grad`/`turn` are converted, `deg` and
/// a bare number pass through, and any other unit (e.g. `1in`) is treated as its
/// magnitude — matching the `hwb` units goldens (`1rad` → 57.295…, `1in` → 1).
fn hue_deg(c: &super::Call, i: usize) -> Or<f64> {
    let (v, unit) = c
        .positional
        .get(i)
        .and_then(|a| a.as_number())
        .ok_or_else(|| err("expected a number for $hue"))?;
    Ok(match unit {
        Some("rad") => v.to_degrees(),
        Some("grad") => v * 0.9,
        Some("turn") => v * 360.0,
        _ => v,
    })
}

fn channel_byte(c: &super::Call, i: usize) -> Or<u8> {
    Ok(number_arg(c, i)?.round().clamp(0.0, 255.0) as u8)
}

/// `color.is-legacy($color)`: a legacy color (`#hex`, a named color, or an
/// `rgb`/`hsl`/`hwb` literal) carries no `Color4` payload; only a CSS Color 4
/// `color(…)` / `lab(…)` / `lch(…)` / … value does. So the predicate is exactly
/// "has no Color 4 representation".
fn is_legacy(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    Ok(SassValue::Bool(col.c4.is_none()))
}

/// A channel argument that resolves to a concrete byte: a number, or the
/// keyword `none` (which the legacy `rgb()` treats as 0). Anything else — a
/// `calc()`/`var()`/`env()`/`attr()` string — is unresolvable and drives the
/// `rgb(…)` passthrough.
fn is_channel_value(v: &SassValue) -> bool {
    match v {
        SassValue::Number { .. } => true,
        SassValue::Str { text, .. } => text == "none",
        _ => false,
    }
}

fn number_arg(c: &super::Call, i: usize) -> Or<f64> {
    match c.positional.get(i) {
        Some(SassValue::Number { value, .. }) => Ok(*value),
        // In the legacy `rgb()`/`hsl()` constructors a `none` channel resolves
        // to 0 (the goldens), producing a concrete color rather than an error.
        Some(SassValue::Str { text, .. }) if text == "none" => Ok(0.0),
        Some(other) => Err(err(format!(
            "expected a number, given {}",
            other.type_name()
        ))),
        None => Err(err("expected a number")),
    }
}

/// Registers the legacy color builtins into `r`. Called by `color::register`.
pub(crate) fn register(r: &mut super::Registry) {
    r.nsp(
        "color",
        "mix",
        2,
        Some(4),
        &["color1", "color2", "weight", "method"],
        mix,
    );
    // Deprecated single-channel accessors (`color.red($color)`, …): thin shims
    // over `color.channel` that read one channel in its natural space. `$color`
    // folds from a named argument via the shared dispatcher. Named fns (not
    // closures) because a body that forwards `c` and uses `?` defeats the
    // higher-ranked coercion to the `fn`-pointer `BuiltinFn` type.
    r.nsp("color", "red", 1, Some(1), &["color"], red);
    r.nsp("color", "green", 1, Some(1), &["color"], green);
    r.nsp("color", "blue", 1, Some(1), &["color"], blue);
    r.nsp("color", "hue", 1, Some(1), &["color"], hue);
    r.nsp("color", "saturation", 1, Some(1), &["color"], saturation);
    r.nsp("color", "lightness", 1, Some(1), &["color"], lightness);
    r.nsp("color", "whiteness", 1, Some(1), &["color"], whiteness);
    r.nsp("color", "blackness", 1, Some(1), &["color"], blackness);
    r.nsp("color", "alpha", 1, Some(1), &["color"], alpha);
    r.nsp("color", "opacity", 1, Some(1), &["color"], alpha);
    r.nsp("color", "is-legacy", 1, Some(1), &["color"], is_legacy);
    r.nsp("color", "ie-hex-str", 1, Some(1), &["color"], ie_hex_str);
    r.ns("color", "rgb", 1, Some(3), rgb);
    r.ns("color", "hsl", 1, Some(3), hsl);
    r.nsp(
        "color",
        "hwb",
        3,
        Some(4),
        &["hue", "whiteness", "blackness", "alpha"],
        hwb,
    );
    // Legacy global spellings.
    r.def("ie-hex-str", 1, Some(1), ie_hex_str);
    // Legacy global color constructors element-plus leans on (`rgba($c, $a)`,
    // `rgb(r, g, b)` / `rgba(r, g, b, $alpha)`); both spellings share one impl.
    r.def("rgb", 1, Some(4), rgb);
    r.def("rgba", 1, Some(4), rgb);
    // NOTE: the bare global `hsl()`/`hsla()` constructors are deliberately left
    // unregistered. A real Color for `hsl()` regressed ~100 spec cases whose
    // goldens reproduce the *input* text (channel/slash/var()/calc() forms and
    // out-of-range values the passthrough string matched by construction); the
    // faithful legacy-hsl serialization surface is far too large to converge
    // safely here, so `hsl(…)` stays an unresolved call string as before.
}

/// Shared body of the deprecated single-channel accessors: read one channel of
/// the `$color` argument (positional slot 0) with no explicit `$space`, so a
/// legacy color falls through RGB / HSL / HWB-from-RGB.
fn chan(c: &super::Call, name: &str) -> Or<SassValue> {
    read_channel(&color_at(c, 0)?, name, None)
}

// Generate the eight `color.<channel>` shims. Named fns (not closures) coerce
// cleanly to the `fn`-pointer `BuiltinFn` type the dispatcher stores.
macro_rules! channel_accessor {
    ($($fn:ident => $chan:literal),* $(,)?) => {
        $(fn $fn(c: &super::Call) -> Or<SassValue> { chan(c, $chan) })*
    };
}
channel_accessor! {
    red => "red",
    green => "green",
    blue => "blue",
    hue => "hue",
    saturation => "saturation",
    lightness => "lightness",
    whiteness => "whiteness",
    blackness => "blackness",
}
