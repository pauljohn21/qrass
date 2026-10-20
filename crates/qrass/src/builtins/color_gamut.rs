//! `color.is-in-gamut` / `color.to-gamut` — the CSS Color 4 gamut predicates
//! (task 11.3, gamut/output round W1/W2; to-gamut clip family in round 82).
//! Kept out of `color.rs` (which sits at
//! the 500-line budget) as a sibling module; reached through `super::color`'s
//! `pub(super)` `color_at`/`err` helpers.

use super::color::{color_at, err};
use super::Or;
use crate::color4::{self, Color4};
use crate::error::SassError;
use crate::value::{Color, SassValue};

/// Resolve a `$space` argument to a registered [`color4::Space`] name, mapping
/// the legacy alias `rgb` to `srgb`. `None` (no explicit space) is the caller's
/// responsibility to default.
fn resolve_space(name: &str) -> Option<&'static color4::Space> {
    color4::lookup(if name == "rgb" { "srgb" } else { name })
}

/// The channels of `col` expressed in `target` (converting when needed). A
/// legacy color carries sRGB display channels on the 0..255 scale; those are
/// normalised to 0..1 (or pushed through the hub for a non-sRGB target).
fn channels_in(col: &crate::value::Color, target: &'static color4::Space) -> [Option<f64>; 3] {
    match &col.c4 {
        Some(c4) => {
            if c4.space.name == target.name {
                c4.comps
            } else {
                c4.convert(target).comps
            }
        }
        None => {
            let srgb = [col.r / 255.0, col.g / 255.0, col.b / 255.0];
            if target.name == "srgb" {
                srgb.map(Some)
            } else {
                let base = Color4::new(
                    color4::lookup("srgb").expect("srgb registered"),
                    srgb,
                );
                base.convert(target).comps
            }
        }
    }
}

/// `color.is-in-gamut($color, $space: null)`: is every channel of `$color`
/// within the named space's channel ranges (default: the color's own space)?
/// A missing (`none`) channel is never out of gamut; alpha is not checked.
pub(crate) fn is_in_gamut(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let space = match c.named.get("space").or(c.positional.get(1)) {
        Some(v) => v.to_css(),
        None => match &col.c4 {
            Some(c4) => c4.space.name.to_string(),
            None => "srgb".to_string(),
        },
    };
    let target = resolve_space(&space).ok_or_else(|| err(format!("Unknown color space: {space}")))?;
    let comps = channels_in(&col, target);
    // Golden-verified slack: values on the boundary (and tiny float overshoot
    // from the round-trip) count as in gamut.
    const TOL: f64 = 1e-4;
    let ok = comps.iter().zip(&target.channels).all(|(v, ch)| match v {
        None => true,
        Some(x) => *x >= ch.lo - TOL && *x <= ch.hi + TOL,
    });
    Ok(SassValue::Bool(ok))
}

/// `color.to-gamut($color, $space, $method)`: clip the color's channels into
/// `$space`'s legal range and re-emit in the color's own space (every
/// `to_gamut/*/clip` golden round-trips through the source space; the `$space`
/// only names the gamut being clipped). Spellings are read off
/// `to_gamut/error.hrx`: unquoted-string requirements, `Unknown color space
/// "c".`, `Unknown gamut map method "c".`, and the mandatory-`$method`
/// forward-compatibility paragraph. The minde methods reduce to "unchanged"
/// for colors already in gamut (`local-minde/in_gamut` goldens); an
/// out-of-gamut minde mapping needs the CSS gamut-mapping algorithm and is
/// deferred honestly (a clear error, never a silently wrong color).
pub(crate) fn to_gamut(c: &super::Call) -> Or<SassValue> {
    let col = color_at(c, 0)?;
    let space_v = c.named.get("space").or(c.positional.get(1));
    let target = match space_v {
        // `Null` is the fold_named gap placeholder (`$method:`-only calls
        // push the named value to slot 2 and leave slot 1 filled with Null),
        // not an explicit null argument.
        None | Some(SassValue::Null) => col
            .c4
            .as_ref()
            .map(|c4| c4.space.name.to_string())
            .unwrap_or_else(|| "rgb".to_string()),
        Some(SassValue::Str { text, quoted: false }) => text.replace('_', "-"),
        Some(SassValue::Str { text, quoted: true }) => {
            return Err(err(format!(
                "$space: Expected \"{text}\" to be an unquoted string."
            )));
        }
        Some(other) => {
            return Err(err(format!(
                "$space: {} is not a string.",
                other.to_css()
            )));
        }
    };
    let is_legacy_target = target == "rgb";
    let sp = if is_legacy_target {
        color4::lookup("srgb").expect("srgb space registered")
    } else {
        resolve_space(&target).ok_or_else(|| {
            err(format!("Unknown color space \"{target}\"."))
        })?
    };
    let method_v = c.named.get("method").or(c.positional.get(2));
    let method = match method_v {
        None | Some(SassValue::Null) => {
            return Err(SassError::new(
                crate::error::ErrorKind::Builtins,
                "$method: color.to-gamut() requires a $method argument for forwards-compatibility with changes in the CSS spec. Suggestion:\n\n$method: local-minde",
            ));
        }
        Some(SassValue::Str { text, quoted: false })
            if matches!(text.as_str(), "clip" | "local-minde" | "oklch") =>
        {
            text.as_str()
        }
        Some(SassValue::Str { text, quoted: true }) => {
            return Err(err(format!(
                "$method: Expected \"{text}\" to be an unquoted string."
            )));
        }
        Some(SassValue::Str { text, .. }) => {
            return Err(err(format!("Unknown gamut map method \"{text}\".")));
        }
        Some(other) => {
            return Err(err(format!(
                "$method: {} is not a string.",
                other.to_css()
            )));
        }
    };
    // Work in the target space. A legacy color's 0..255 channels normalize to
    // the srgb 0..1 scale; everything else rides the existing conversion hub.
    let src_space = col
        .c4
        .as_ref()
        .map(|c4| c4.space.name)
        .unwrap_or("srgb");
    let src_c4 = match &col.c4 {
        Some(inner) => inner.as_ref().clone(),
        None => {
            let srgb = color4::lookup("srgb").expect("srgb space registered");
            Color4 {
                space: srgb,
                text: [None, None, None],
                comps: [
                    Some(col.r / 255.0),
                    Some(col.g / 255.0),
                    Some(col.b / 255.0),
                ],
                alpha: None,
                alpha_missing: false,
            }
        }
    };
    let mut in_target = src_c4.convert(sp);
    let out_of_gamut = in_target
        .comps
        .iter()
        .zip(sp.channels.iter())
        .any(|(v, ch)| v.is_some_and(|x| x < ch.lo - 1e-9 || x > ch.hi + 1e-9));
    if method != "clip" {
        if out_of_gamut {
            return Err(err(format!(
                "$method: {method} gamut mapping is not supported yet; only `clip` is implemented."
            )));
        }
        // In gamut every method returns the color unchanged.
        return Ok(SassValue::Color(col));
    }
    for (v, ch) in in_target.comps.iter_mut().zip(sp.channels.iter()) {
        if let Some(x) = v {
            *x = x.clamp(ch.lo, ch.hi);
        }
    }
    let back = in_target.convert(color4::lookup(src_space).expect("source space registered"));
    // The serialization follows the source color's kind: a Color 4 input
    // re-emits its own space (`to_gamut/space` `clip/out_of_gamut`: oklch in,
    // oklch out); a legacy input stays legacy (`clip/out_of_gamut/legacy`:
    // `rgb(255, 217, 128)`), with one exception — the `rgb(none 0 none)`
    // modern-rgb spelling keeps its bare `rgb(…)` form after clipping to the
    // rgb gamut (`clip/missing/rgb`).
    if col.c4.is_some() {
        let alpha = src_c4.alpha;
        return Ok(SassValue::Color(Color {
            c4: Some(Box::new(Color4 {
                alpha,
                ..back.clone()
            })),
            ..back_to_rgb(back.clone(), col.a, &col)
        }));
    }
    if is_legacy_target && col.name.is_none() && src_c4.comps.iter().any(|v| v.is_none()) {
        return Ok(SassValue::Color(Color {
            c4: Some(Box::new(Color4 {
                alpha: src_c4.alpha,
                ..back.clone()
            })),
            ..back_to_rgb(back.clone(), col.a, &col)
        }));
    }
    let rgb = back.to_rgb();
    Ok(SassValue::Color(Color {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
        a: col.a,
        hex: false,
        name: None,
        legacy: col.legacy,
        legacy_hsl: None,
        c4: None,
    }))
}

/// Numeric shell for a Color-4-carrying result (the r/g/b fields stay used by
/// legacy readers; serialization rides the `c4` payload).
fn back_to_rgb(
    back: Color4,
    a: f64,
    base: &Color,
) -> Color {
    let rgb = back.to_rgb();
    Color {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
        a,
        hex: false,
        name: None,
        legacy: base.legacy,
        legacy_hsl: None,
        c4: None,
    }
}
