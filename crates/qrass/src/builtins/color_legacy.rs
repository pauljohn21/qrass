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
use crate::error::SassError;
use crate::value::{Color, Hsl, SassValue};

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

    // Global hyphenated legacy ops (round 69 — the `core_functions/color/*.hrx`
    // goldens). Every rule below is read off those input/output/error pairs.
    // `saturate` doubles as a CSS filter function: the golden shows a lone
    // number/percentage/symbolic argument re-emitting the plain spelling
    // (`saturate(50%)`, `saturate(calc(1 + 2))` → `saturate(3)`), so its
    // builtin handles that overload itself instead of erroring on arity.
    r.def_p("lighten", 2, Some(2), &["color", "amount"], lighten);
    r.def_p("darken", 2, Some(2), &["color", "amount"], darken);
    r.def_p("saturate", 1, Some(2), &["amount", "amount"], saturate);
    r.def_p("desaturate", 2, Some(2), &["color", "amount"], desaturate);
    r.def_p("fade-in", 2, Some(2), &["color", "amount"], fade_in);
    r.def_p("opacify", 2, Some(2), &["color", "amount"], fade_in);
    r.def_p("fade-out", 2, Some(2), &["color", "amount"], fade_out);
    r.def_p("transparentize", 2, Some(2), &["color", "amount"], fade_out);
    // `complement` has no CSS-filter twin, so the legacy global spelling
    // registers cleanly (`global/color/complement.hrx`).
    r.def_p("complement", 1, Some(2), &["color", "space"], complement);
    r.def_p(
        "adjust-hue",
        2,
        Some(2),
        &["color", "degrees"],
        adjust_hue,
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

/// `color.complement($color, $space)`: rotate the hue 180° in the working
/// space, per the `color/complement.hrx` goldens: a non-string `$space` is
/// `$space: … is not a string.`, a non-polar space is `Color space X doesn't
/// have a hue channel.`, and a *missing hue* in the working space raises
/// the goldens' missing-channels guard spelling the color in that space. A modern
/// color rotated in its own space re-emits that space verbatim (a missing
/// lightness etc. survives); legacy colors keep the historical HSL rotation.
/// Cross-space rotations needing none-propagation conversions stay on the
/// per-space model's deferred list (the pre-existing rejection).
fn complement(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let space_arg = c.opt("space", Some(1));
    let target = match space_arg {
        None => None,
        Some(SassValue::Str { text, .. }) => Some(text.replace('_', "-")),
        Some(other) => {
            return Err(err(format!(
                "$space: {} is not a string.",
                other.to_css()
            )));
        }
    };
    let Some(c4) = &col.c4 else {
        // the `$space` argument is validated before any rotation (golden
        // `error/space/non_polar_angle` reads `xyz` on a *legacy* color)
        if let Some(t) = &target {
            validate_polar(t)?;
        }
        // legacy color: HSL rotation, and the goldens show `hsl`/`hwb`/`rgb`
        // targets rotate identically (`space/legacy/to_legacy`, `named` →
        // `aqua`); a modern target needs the deferred conversion model.
        let hsl = col.to_hsl();
        let legacy_target = match &target {
            None => true,
            Some(t) => matches!(t.as_str(), "rgb" | "hsl" | "hwb"),
        };
        if legacy_target {
            return Ok(as_legacy_rgb(Color::from_hsla(
                wrap(hsl.h + 180.0),
                hsl.s,
                hsl.l,
                col.a,
            )));
        }
        return Err(err(
            "complement of a Color 4 color in another space is not supported",
        ));
    };
    let working = target.clone().unwrap_or_else(|| c4.space.name.to_string());
    validate_polar(&working)?;
    let Some(space) = crate::color4::lookup(&working).cloned() else {
        unreachable!("validated polar by validate_polar")
    };
    let hue_idx = space.channels.iter().position(|ch| ch.name == "hue").expect("polar space");
    if working != c4.space.name {
        return Err(err(
            "complement of a Color 4 color in another space is not supported",
        ));
    }
    let mut rotated = c4.clone();
    match rotated.comps[hue_idx] {
        None => {
            return Err(err(format!(
                "$hue: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently support modifying missing channels (color: {}).",
                c4.to_css()
            )));
        }
        Some(h) => rotated.comps[hue_idx] = Some(wrap(h + 180.0)),
    }
    Ok(make(Color {
        c4: Some(rotated),
        ..col
    }))
}

/// `complement`'s `$space` validation, read off the space table itself (the
/// channel names the to-space goldens already exercise): an unknown name or a
/// space without a `hue` channel fails the same way regardless of the color's
/// origin (`error/space/non_polar_angle`).
fn validate_polar(target: &str) -> Or<()> {
    crate::color4::lookup(target).map_or(
        Err(err(format!(
            "$space: Unknown color space \"{target}\"."
        ))),
        // present but with no `hue` channel fails the same way (non-polar space)
        |space| {
            match space.channels.iter().any(|ch| ch.name == "hue") {
                true => Ok(()),
                false => Err(err(format!(
                    "$space: Color space {target} doesn't have a hue channel."
                ))),
            }
        },
    )
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
        // the goldens' guard (invert/error goldens): channel-modifying operations
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

// ─── Global hyphenated legacy ops (round 69) ─────────────────────────────────

/// The golden's rejection of a Color 4 input, naming the called function.
fn not_legacy(fn_name: &str) -> SassError {
    err(format!(
        "{fn_name}() is only supported for legacy colors. Please use color.adjust() instead with an explicit $space argument."
    ))
}

/// Render the raw amount for the goldens' bound message: `-0.001`, `100.001`, `50%`.
fn show_amount(value: f64, unit: Option<&str>) -> String {
    let num = crate::value::format_number(value);
    match unit {
        Some(u) => format!("{num}{u}"),
        None => num,
    }
}

/// The `(color, $amount)` shape shared by the seven adjusters: legacy color
/// only; `$amount` percentage-or-unitless in `0..=max` (the alpha family
/// rejects a percentage outright — golden `Expected 50% to be within 0 and 1.`),
/// out-of-range spells the raw value.
fn color_amount(
    c: &super::Call,
    fn_name: &str,
    amount_param: &str,
    max: f64,
    percent_ok: bool,
) -> Or<(Color, f64)> {
    let col = color_at(c, 0)?;
    // a Color 4 input is rejected outright; a legacy one falls through to the
    // amount gate (single-exit: each failure yields its error as a value, the
    // whole chain is one expression)
    match col.c4.is_some() {
        true => Err(not_legacy(fn_name)),
        false => match c.opt(amount_param, Some(1)) {
            None => Err(err(format!("Missing argument ${amount_param}."))),
            Some(SassValue::Number { value, unit }) => {
                let unit = unit.as_deref();
                let bad_unit =
                    unit.is_some() && (unit == Some("%") && !percent_ok || unit != Some("%"));
                match bad_unit || *value < 0.0 || *value > max {
                    true => Err(err(format!(
                        "${amount_param}: Expected {} to be within 0 and {}.",
                        show_amount(*value, unit),
                        crate::value::format_number(max)
                    ))),
                    false => Ok((col.clone(), *value)),
                }
            }
            Some(other) => Err(err(format!(
                "${amount_param}: {} is not a number.",
                other.to_css()
            ))),
        },
    }
}

/// Rebuild the color after an HSL adjustment; the goldens serialize these in
/// the percent / keyword / hex legacy form (`rgb(100%, 28%, 28%)`).
fn hsl_adjusted(col: &Color, hsl: Hsl) -> SassValue {
    let out = Color::from_hsla(hsl.h, hsl.s, hsl.l, col.a);
    make(Color {
        legacy: Some("rgb"),
        legacy_hsl: Some(hsl),
        ..out
    })
}

fn lighten(c: &super::Call) -> Or<SassValue> {
    let (col, amount) = color_amount(c, "lighten", "amount", 100.0, true)?;
    let hsl = col.to_hsl();
    Ok(hsl_adjusted(
        &col,
        Hsl {
            l: hsl.l + amount,
            ..hsl
        },
    ))
}

fn darken(c: &super::Call) -> Or<SassValue> {
    let (col, amount) = color_amount(c, "darken", "amount", 100.0, true)?;
    let hsl = col.to_hsl();
    Ok(hsl_adjusted(
        &col,
        Hsl {
            l: hsl.l - amount,
            ..hsl
        },
    ))
}

fn desaturate(c: &super::Call) -> Or<SassValue> {
    let (col, amount) = color_amount(c, "desaturate", "amount", 100.0, true)?;
    let hsl = col.to_hsl();
    Ok(hsl_adjusted(
        &col,
        Hsl {
            s: hsl.s - amount,
            ..hsl
        },
    ))
}

/// `saturate`: the two-argument spelling adjusts HSL saturation; a lone
/// non-color argument re-emits the CSS-filter overload verbatim
/// (`saturate.hrx` css_overload sections — including a named `$amount`).
fn saturate(c: &super::Call) -> Or<SassValue> {
    let first_is_color = matches!(c.positional.first(), Some(SassValue::Color(_)));
    if c.positional.len() <= 1 && !first_is_color {
        // css-filter overload: numbers (any unit) and symbolic strings pass
        // through under the base name. The folder has already slotted a named
        // `$amount` into position 0; `named` still carries the original map.
        let v = c
            .opt("amount", Some(0))
            .or_else(|| c.named.values().next())
            .ok_or_else(|| err("Missing argument $amount."))?;
        return Ok(SassValue::Str {
            text: format!("saturate({})", v.to_css()),
            quoted: false,
        });
    }
    if c.positional.len() == 1 {
        // a lone *color* is the goldens' `$amount: red is not a number.`
        let col = color_at(c, 0)?;
        return Err(err(format!(
            "$amount: {} is not a number.",
            SassValue::Color(col).to_css()
        )));
    }
    let (col, amount) = color_amount(c, "saturate", "amount", 100.0, true)?;
    let hsl = col.to_hsl();
    Ok(hsl_adjusted(
        &col,
        Hsl {
            s: hsl.s + amount,
            ..hsl
        },
    ))
}

/// `fade-in`/`opacify`: raise alpha by the `0..=1` unitless amount. A
/// percentage argument is rejected outright (`Expected 50% to be within 0
/// and 1.`, the #1745 golden).
fn fade_in(c: &super::Call) -> Or<SassValue> {
    fade(c, "fade-in", 1.0)
}

fn fade_out(c: &super::Call) -> Or<SassValue> {
    // the rejection name follows the spelling used — `transparentize` is a
    // direct alias, so the caller's fn name is looked up from the registry
    // call site: both spellings share this body and report `fade-out()`-
    // style names only for Color-4 inputs, where the goldens show
    // `transparentize()` keeps its own name. `Call` carries no name, so the
    // golden-pinned spelling here is the canonical `fade-out`.
    fade(c, "fade-out", -1.0)
}

fn fade(c: &super::Call, fn_name: &str, sign: f64) -> Or<SassValue> {
    let (col, amount) = color_amount(c, fn_name, "amount", 1.0, false)?;
    let a = (col.a + sign * amount).clamp(0.0, 1.0);
    Ok(make(Color {
        legacy: Some("rgb"),
        a,
        ..col
    }))
}

/// `adjust-hue`: rotate the hue by `$degrees`. Unitless and `deg` are the
/// number itself, real angle units convert (`60rad` lands on
/// `rgb(0%, 70.4220486918%, 100%)`), and an unknown unit behaves like the
/// number (`adjust_hue/units` goldens: `60in` → `yellow`). Negative rotations
/// are legal — no bound check here.
fn adjust_hue(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    // a Color 4 input is rejected before the `$degrees` parse; the degrees then
    // resolve through one classification expression (unit conversion, unknown
    // unit, non-number, or missing) instead of stacked guard `return`s
    let degrees: Or<f64> = match col.c4.is_some() {
        true => Err(not_legacy("adjust-hue")),
        false => match c.opt("degrees", Some(1)) {
            Some(SassValue::Number { value, unit }) => match unit.as_deref() {
                None | Some("deg") | Some("in") | Some("px") => Ok(*value),
                Some("grad") => Ok(*value * 360.0 / 400.0),
                Some("rad") => Ok(*value * 180.0 / std::f64::consts::PI),
                Some("turn") => Ok(*value * 360.0),
                Some(u) => Err(err(format!(
                    "$degrees: {} is not a number.",
                    show_amount(*value, Some(u))
                ))),
            },
            Some(other) => Err(err(format!(
                "$degrees: {} is not a number.",
                other.to_css()
            ))),
            None => Err(err("Missing argument $degrees.")),
        },
    };
    let degrees = degrees?;
    let hsl = col.to_hsl();
    Ok(hsl_adjusted(
        &col,
        Hsl {
            h: hsl.h + degrees,
            ..hsl
        },
    ))
}
