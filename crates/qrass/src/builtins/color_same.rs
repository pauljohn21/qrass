//! `color.same` for the color builtins (round 76) — a sibling split of
//! `color_ops` for the 500-line budget. Equality rides the XYZ conversion
//! paths with missing channels normalized to 0 first, per the
//! `core_functions/color/same.hrx` goldens.

use super::Or;
use crate::value::{Color, SassValue};

/// `color.same($color1, $color2)`: compare colors in XYZ after normalizing
/// missing channels to `0` — the order the `false/different_none_channel`
/// golden spells out in its own comment ("none is converted to 0 *before*
/// conversion to XYZ"). Legacy colors join through their sRGB triple
/// (`plum` vs its `hsl(…)` spelling is `true`, `same.hrx`); channels compare
/// with a tolerance because the equality rides two independent conversion
/// paths, while every `false` golden differs by orders of magnitude.
pub(crate) fn same(c: &super::Call) -> Or<SassValue> {
    let col1 = super::color::color_at_named(c, 0, "color1")?;
    let col2 = super::color::color_at_named(c, 1, "color2")?;
    let (x1, a1) = xyz_of(&col1);
    let (x2, a2) = xyz_of(&col2);
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-7f64 * a.abs().max(b.abs()).max(1.0);
    Ok(SassValue::Bool(
        x1.iter().zip(&x2).all(|(a, b)| close(*a, *b)) && close(a1, a2),
    ))
}
fn xyz_of(col: &Color) -> ([f64; 3], f64) {
    let alpha = col.a;
    match &col.c4 {
        Some(c4) => {
            let mut zeroed = c4.clone();
            zeroed.comps = c4.comps.map(|slot| slot.or(Some(0.0)));
            (zeroed.to_xyz_d65(), alpha)
        }
        None => {
            let srgb = crate::color4::lookup("srgb").expect("srgb space registered");
            let c4 = crate::color4::Color4 {
                space: srgb,
                text: [None, None, None],
                comps: [Some(col.r / 255.0), Some(col.g / 255.0), Some(col.b / 255.0)],
                alpha: None,
                alpha_missing: false,
            };
            (c4.to_xyz_d65(), alpha)
        }
    }
}
