//! Legacy color builtins (`color.rgb`/`color.hsl`/`color.mix`/`color.alpha`/
//! `color.opacity`/`color.ie-hex-str` plus their global spellings). Split out of
//! `color.rs` to keep that module within the source-size budget; the shared
//! argument helpers (`err`, `color_at`, `make`) stay there as `pub(super)`.
//! `color.to-space` also lives here (round-62 budget split).

use super::color::{color_at, err, make, read_channel};
use super::Or;
use crate::value::{Color, Hsl, SassValue};

/// `color.to-space($color, $space)`: convert a color into another space.
///
/// Modern targets (`lab`/`lch`/`oklab`/`oklch`/`xyz`/`xyz-d50`/`srgb`/gamut
/// spaces) carry a `Color4` and serialize in CSS Color 4 form. The legacy `rgb`
/// target keeps the historical representation (an unchanged legacy color, or a
/// concrete legacy color built from sRGB when the source is Color 4).
pub(super) fn to_space(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let space = match c.opt("space", Some(1)) {
        Some(SassValue::Str { text, .. }) => text.clone(),
        Some(other) => other.to_css(),
        None => return Err(err("expected a color space")),
    };

    // The Color 4 source for the conversion: the color's own payload, or a legacy
    // sRGB color promoted through its 0..255 display channels.
    let src: crate::color4::Color4 = match &col.c4 {
        Some(c4) => (**c4).clone(),
        None => {
            let mut c4 = crate::color4::Color4::new(
                crate::color4::lookup("srgb").expect("srgb space registered"),
                [col.r / 255.0, col.g / 255.0, col.b / 255.0],
            );
            if col.a != 1.0 {
                c4.alpha = Some(col.a);
            }
            c4
        }
    };

    let hsl_like = matches!(src.space.name, "hsl" | "hwb");

    if space == "rgb" {
        // Legacy target: a legacy color is returned untouched (hex/rgb form).
        if col.c4.is_none() {
            return Ok(make(col));
        }
        // `to-space(_, rgb)` always yields the golden's *legacy* rgb form: a missing
        // channel resolves to 0 (`to_rgb` reads `raw()`), so the result is a
        // concrete rgb color serialized as keyword/`#hex`/`rgb(%)` — never a
        // `color(srgb …)` wrapper (confirmed across the to-space rgb goldens).
        // An hsl/hwb source also carries its verbatim HSL triple so an
        // out-of-gamut `hsl()` fallback emits the golden's exact channels.
        let converted = src.convert(crate::color4::lookup("srgb").expect("srgb space"));
        let rgb = converted.to_rgb();
        let legacy_hsl = if hsl_like {
            hsl_from_legacy_src(&src).map(wrap_hue)
        } else {
            None
        };
        return Ok(make(Color {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
            a: converted.alpha(),
            hex: false,
            c4: None,
            name: None,
            legacy: Some("rgb"),
            legacy_hsl,
        }));
    }

    let Some(target) = crate::color4::lookup(&space) else {
        return Err(err(format!("Unknown color space: {space}")));
    };
    let converted = if target.name == src.space.name {
        src
    } else {
        src.convert(target)
    };
    // `to-space(hsl|hwb, hsl|hwb)` with every channel concrete yields the golden's
    // *legacy* color built directly from the source's HSL-formula channels: an
    // out-of-gamut result serializes as an unclamped `hsl()` (channels past 1,
    // a negative saturation flipped to hue +180° — see `legacy_to_css`), and the
    // carried triple is the golden's exact inversion (`hsl(200, 11.1111111111%,
    // 212.5%)`), which the RGB round trip cannot recover. Only an `hsl`/`hwb`
    // *source* takes this path; a modern source (srgb/xyz/…) keeps the Color 4
    // payload so `.space`/`.channel` still report the target space.
    let legacy_target = matches!(target.name, "hsl" | "hwb");
    let missing = converted.comps.iter().any(|c| c.is_none()) || converted.alpha_missing;
    let modern = converted.space.name == "hsl" && missing;
    if hsl_like && legacy_target && !modern && !missing {
        let [r, g, b] = converted.to_rgb();
        let carried = hsl_from_legacy_src(&converted).map(wrap_hue);
        return Ok(make(Color {
            r,
            g,
            b,
            a: converted.alpha(),
            hex: false,
            c4: Some(Box::new(converted)),
            name: None,
            legacy: Some(target.name),
            legacy_hsl: carried,
        }));
    }
    // Every other result keeps the Color 4 payload. A legacy `hsl`/`hwb` target
    // from a *modern* source still serializes in the golden's legacy CSS form (keyword
    // /`#hex`/`rgb(%)`/`hsl()`); a modern target, or an `hsl`/`hwb` target with a
    // `none` channel (`hwb(10deg none none)`), serializes in the `color(…)` or
    // bare-space form.
    let rgb = converted.to_rgb();
    Ok(make(Color {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
        a: converted.alpha(),
        hex: false,
        c4: Some(Box::new(converted)),
        name: None,
        legacy: legacy_target.then_some(target.name).filter(|_| !hsl_like),
        legacy_hsl: None,
    }))
}

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

/// `color.mix($color1, $color2, $weight, $method)`.
fn mix(c: &super::Call) -> Or<SassValue> {
    let a = color_at(c, 0)?;
    let b = color_at(c, 1)?;
    super::color_mix::validate_mix_method(c, &a, &b)?;
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
    r.nsp("color", "same", 2, Some(2), &["color1", "color2"], super::color_same::same);
    // Legacy global `mix($color1, $color2, $weight)` — the arity wording is
    // parameter-name driven (`mix(red)` → `Missing argument $color2.`,
    // `color/mix/error` `too_few_args`).
    r.def_p(
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
    r.def_p("rgb", 1, Some(4), &["red", "green", "blue", "alpha"], rgb);
    r.def_p("rgba", 1, Some(4), &["red", "green", "blue", "alpha"], rgb);
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
