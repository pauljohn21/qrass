//! `color.is-in-gamut` / `color.to-gamut` — the CSS Color 4 gamut predicates
//! (task 11.3, gamut/output round W1/W2). Kept out of `color.rs` (which sits at
//! the 500-line budget) as a sibling module; reached through `super::color`'s
//! `pub(super)` `color_at`/`err` helpers.

use super::color::{color_at, err};
use super::Or;
use crate::color4::{self, Color4};
use crate::value::SassValue;

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
