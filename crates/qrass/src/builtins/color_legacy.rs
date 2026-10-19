//! Legacy color operations that predate CSS Color 4: `complement`, `grayscale`,
//! `invert`. They are defined purely in the HSL / sRGB legacy model (no gamut
//! conversion), matching the `.hrx` goldens for a *legacy* color input.
//!
//! Scope note: a Color 4 input (one that carries a `c4` payload) is rejected
//! with a clean error rather than guessed at, because the cross-space goldens
//! (`complement(lab(..), $space: oklch)` etc.) need the per-space dependency
//! model that is deliberately out of scope here. Those cases already fail today
//! (the function is undefined), so erroring is a strict non-regression.
//!
//! The result of every legacy op is tagged `legacy = "rgb"` so serialization
//! reproduces the goldens' exact form (keyword / `#hex` / `rgb(%…)`) — the same
//! path `color.to-space(_, rgb)` uses.

use super::color::{color_at, err, make};
use super::{Or, Registry};
use crate::value::{Color, SassValue};

pub(crate) fn register(r: &mut Registry) {
    // Namespaced spellings (`color.*`) plus the `nsp` hyphen alias. The bare
    // globals `complement`/`grayscale`/`invert` are deliberately NOT registered:
    // `invert()`/`grayscale()` are also CSS filter functions, and the goldens
    // rely on an *unregistered* bare call passing through verbatim as plain CSS
    // (`invert(10%)`, `invert(var(--c))`, …). Shadowing them with a builtin breaks
    // that passthrough, so only the `color.` namespace is claimed here.
    r.nsp(
        "color",
        "complement",
        1,
        Some(2),
        &["color", "space"],
        complement,
    );
    r.nsp("color", "grayscale", 1, Some(1), &["color"], grayscale);
    r.nsp(
        "color",
        "invert",
        1,
        Some(3),
        &["color", "weight", "space"],
        invert,
    );
}

/// Hue in `0..360`.
fn wrap(h: f64) -> f64 {
    ((h % 360.0) + 360.0) % 360.0
}

/// Re-tag a freshly computed legacy color for the golden's `rgb()`/keyword/hex
/// serialization path. `from_hsla` yields an untagged color; dropping `name`/`hex`
/// is intentional — the value is derived, not a literal, and `named_for_rgb` in
/// serialization restores a keyword when the channels hit one.
fn as_legacy_rgb(col: Color) -> SassValue {
    make(Color {
        legacy: Some("rgb"),
        ..col
    })
}

/// `color.complement($color, $space)`: rotate the hue 180° in HSL. A modern Color 4
/// input is rejected (its goldens need gamut conversion). The optional `$space`
/// (hsl/hwb) selects the same hue rotation for a legacy color, so it is accepted
/// and ignored here.
fn complement(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    if col.c4.is_some() {
        return Err(err(
            "complement of a Color 4 color in another space is not supported",
        ));
    }
    let hsl = col.to_hsl();
    Ok(as_legacy_rgb(Color::from_hsla(
        wrap(hsl.h + 180.0),
        hsl.s,
        hsl.l,
        col.a,
    )))
}

/// `color.grayscale($color)`: drop the HSL saturation to `0`, keeping lightness.
fn grayscale(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    if col.c4.is_some() {
        return Err(err("grayscale of a Color 4 color is not supported"));
    }
    let hsl = col.to_hsl();
    Ok(as_legacy_rgb(Color::from_hsla(hsl.h, 0.0, hsl.l, col.a)))
}

/// `color.invert($color, $weight, $space)`: blend each sRGB channel toward its
/// complement `255 - c` by the weight ratio (default `100%` = full inversion).
fn invert(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    if let Some(c4) = &col.c4 {
        // dart's guard (invert/error goldens): channel-modifying operations
        // reject a color with a missing channel — in the modified space, so a
        // same-or-unspecified `$space` names the color's own channel.
        if let Some((i, _)) = c4.comps.iter().enumerate().find(|(_, v)| v.is_none()) {
            let space = match c.opt("space", Some(2)) {
                Some(SassValue::Str { text, .. }) => text.clone(),
                _ => c4.space.name.to_string(),
            };
            if space == c4.space.name {
                return Err(err(format!(
                    "${}: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently support modifying missing channels (color: {}).",
                    c4.space.channels[i].name,
                    col.to_css()
                )));
            }
        }
        return Err(err("invert of a Color 4 color is not supported"));
    }
    let ratio = weight_ratio(c.opt("weight", Some(1)))?;
    let inv = |v: f64| v * (1.0 - ratio) + (255.0 - v) * ratio;
    Ok(as_legacy_rgb(Color {
        r: inv(col.r),
        g: inv(col.g),
        b: inv(col.b),
        a: col.a,
        hex: false,
        c4: None,
        name: None,
        legacy: Some("rgb"),
        legacy_hsl: None,
    }))
}

/// The `$weight` as a `0..=1` ratio. Sass expects a percentage; a unitless number
/// is treated as an already-scaled magnitude (`50` → `0.5`). Absent → full invert.
fn weight_ratio(v: Option<&SassValue>) -> Or<f64> {
    let Some(v) = v else { return Ok(1.0) };
    let (n, _) = v.as_number().ok_or_else(|| {
        err(format!("$weight: {} is not a number.", v.to_css()))
    })?;
    Ok((n / 100.0).clamp(0.0, 1.0))
}
