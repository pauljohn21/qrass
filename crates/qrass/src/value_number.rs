//! Number formatting, CSS serialization, and unit conversion — the numeric layer
//! of the value model (split out of [`crate::value`] to keep that module focused
//! on the `SassValue` / `Color` type and its serialization). Re-exported from
//! `crate::value` so existing `crate::value::{format_number, number_to_css,
//! convert_number}` call sites resolve unchanged.

/// Serialize a number the way sass does: integral values print without a
/// fractional part; otherwise up to 10 significant digits with trailing zeros
/// trimmed. (Full precision behavior is golden-checked in task group 10.)
pub fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".into()
        } else if v > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if v == v.round() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let mut s = format!("{v:.10}");
    while s.contains('.') && (s.ends_with('0')) {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    // A channel like an hwb whiteness can carry a -0.0 / sub-resolution negative
    // residue from the RGB round trip; its trimmed text would read "-0", but
    // the goldens always emit a plain "0". Normalize the signed-zero artifact.
    if s == "-0" {
        return "0".into();
    }
    s
}

/// Serialize a Sass number to CSS. A non-finite value (`NaN`/`±Infinity`) is not
/// representable as a bare CSS number, so the goldens emit it inside `calc()`: a
/// unitless infinity reads `calc(infinity)`, and a unit-carrying one
/// `calc(infinity * 1<unit>)` (the golden multiplies the unit form explicitly).
pub fn number_to_css(value: f64, unit: Option<&str>) -> String {
    if value.is_nan() {
        return match unit {
            Some(u) => format!("calc(NaN * 1{u})"),
            None => "calc(NaN)".into(),
        };
    }
    if value.is_infinite() {
        let sign = if value > 0.0 { "infinity" } else { "-infinity" };
        return match unit {
            Some(u) => format!("calc({sign} * 1{u})"),
            None => format!("calc({sign})"),
        };
    }
    match unit {
        Some(u) => format!("{}{u}", format_number(value)),
        None => format_number(value),
    }
}

/// Unit conversion table (design D2): map a number in `from` to `to`, following
/// sass's absolute-unit groups. Returns the converted magnitude if convertible.
pub fn convert_number(value: f64, from: Option<&str>, to: Option<&str>) -> Option<f64> {
    if from == to {
        return Some(value);
    }
    let (Some(from), Some(to)) = (from, to) else {
        return None;
    };
    let base = |u: &str| -> Option<(u8, f64)> {
        // (group, factor to group's base unit)
        Some(match u {
            "in" => (0, 1.0),
            "px" => (0, 1.0 / 96.0),
            "pt" => (0, 1.0 / 72.0),
            "pc" => (0, 1.0 / 6.0),
            "cm" => (0, 1.0 / 2.54),
            "mm" => (0, 1.0 / 25.4),
            "q" => (0, 1.0 / 101.6),
            "deg" => (1, 1.0),
            "grad" => (1, 0.9),
            "rad" => (1, 180.0 / std::f64::consts::PI),
            "turn" => (1, 360.0),
            "s" => (2, 1.0),
            "ms" => (2, 0.001),
            "hz" => (3, 1.0),
            "khz" => (3, 1000.0),
            "dpi" => (4, 1.0),
            "dpcm" => (4, 2.54),
            "dppx" => (4, 96.0),
            _ => return None,
        })
    };
    let (gf, vf) = base(from)?;
    let (gt, vt) = base(to)?;
    if gf != gt {
        return None;
    }
    // value × factor_into_base ÷ factor_into_base_of_target
    Some(value * vf / vt)
}
