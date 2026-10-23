//! `color.to-space` and its serialization helpers, split out of [`super::color_ops`]
//! for the source-size budget (OpenSpec `refactor-rfp-strict` task 5b.2). Pure code
//! motion — the conversion, the legacy hue/`Hsl` normalization, and the lossy
//! `color-mix(…, black)` trigger predicate are byte-for-byte the prior definitions.

use super::color::{color_at, err, make};
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
        src.clone()
    } else {
        src.convert(target)
    };
    // the goldens' lossy serialization: when the destination's *lightness* escapes
    // its declared range, or the source already breaks its declared channel
    // bounds, the color prints as `color-mix(in {tgt}, color(xyz …) 100%,
    // black)` with the source's own XYZ verbatim inside (`to_space/*`
    // `out_of_range/near|far`, `white` sections; the same input triggers the
    // form for every destination — no per-target variation in the evidence).
    // Undeclared-by-the-goldens dimensions stay unchecked: rgb-family r/g/b
    // (`srgb -1 0.4 2` prints plainly), lab/lch a/b/C of the *destination*,
    // and hue. `None` in the lightness-relevant source slot joins the form
    // (`missing/y`, hsl `s`/`l`), while RGB gamut `none` sources keep the
    // computed path (`a98-rgb none …` passes through today).
    if mix_form_needed(&src, &converted) {
        // The mix carries the source's own XYZ: missing slots floor to 0 for
        // the display too (`xyz 0.1 none 0.3` → `color(xyz 0.1 0 0.3)`,
        // `xyz/lab` `missing/y`) — exactly what `to_xyz_d65` computes.
        let xyz = crate::color4::Color4 {
            space: crate::color4::lookup("xyz").expect("xyz space"),
            comps: src.to_xyz_d65().map(Some),
            text: [None, None, None],
            alpha: src.alpha,
            alpha_missing: src.alpha_missing,
        };
        return Ok(SassValue::Str {
            text: format!(
                "color-mix(in {}, {} 100%, black)",
                converted.space.name,
                xyz.to_css()
            ),
            quoted: false,
        });
    }
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
fn wrap_hue(h: Hsl) -> Hsl {
    Hsl { h: ((h.h % 360.0) + 360.0) % 360.0, ..h }
}

/// The verbatim HSL triple the golden computes for an `hsl`/`hwb` Color4 source:
/// an `hsl` source's own channels, or the HWB→HSL inversion. The HWB saturation
/// is *signed*: `s = (100 − b − w) / (100 − |2·lightness − 100|) · 100`, so an
/// out-of-range lightness drives the denominator negative and the channels past
/// gamut (`hwb(20deg 200% -125%)` → `hsl(20, -11.11%, 212.5%)`, which the golden
/// then normalizes on emit by flipping the hue +180° — see `legacy_to_css`).
fn hsl_from_legacy_src(c4: &crate::color4::Color4) -> Option<Hsl> {
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

/// Whether `to-space` must emit the goldens' lossy `color-mix(…, black)` form.
/// Evidence (all 89 MIX sections of `to_space/*.hrx`): the trigger is the
/// *destination lightness* escaping its declared range (`xyz 1 1 1` → L
/// 100.115; prophoto `-1 .4 2` → negative L), plus an `xyz`/`xyz-d50`
/// source whose `y` is missing. Channel excursions that keep L inside the
/// range never trigger (`srgb -1 0.4 2` and `hsl 20 999999% 50%` print
/// plainly), and rgb-family excursions are exempt throughout.
fn mix_form_needed(
    src: &crate::color4::Color4,
    converted: &crate::color4::Color4,
) -> bool {
    let outside = |v: f64, lo: f64, hi: f64| v < lo - 1e-9 || v > hi + 1e-9;
    // Destination lightness on the polar/perceptual serialization paths.
    if matches!(converted.space.name, "lab" | "lch" | "oklab" | "oklch") {
        let ch = converted.space.channels[0];
        if let Some(l) = converted.comps[0] {
            // all-missing sources keep the `lab(none none none)` form (existing
            // PASS path); only a *computed* lightness past the bounds triggers.
            if outside(l, ch.lo, ch.hi) {
                return true;
            }
        }
    }
    // xyz/xyz-d50 source with a missing luminance heading for a lightness
    // destination: L is not computable (`xyz/lab` `missing/y` mixes, while
    // the same source to a gamut target keeps `none` channels computed-plain
    // — `xyz/a98_rgb` `missing/y`). Gamut destinations never mix.
    // A D65 `xyz` source feeds lab without a matrix step, so a missing
    // `y` leaves the lightness undetermined → mix (`xyz/lab`
    // `missing/y`). An `xyz-d50` source recovers y from x/z through the
    // adaptation matrix and prints plainly (`xyz_d50/lab` computes
    // `lab(0% 165.9436315393 -115.1609314454)`).
    if matches!(converted.space.name, "lab" | "lch" | "oklab" | "oklch")
        && src.space.name == "xyz"
        && src.comps[1].is_none()
        && !src.comps.iter().all(|c| c.is_none())
    {
        return true;
    }
    false
}
