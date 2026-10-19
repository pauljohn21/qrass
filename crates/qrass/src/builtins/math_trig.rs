//! Trigonometric, angle, and unit machinery for `math.*` builtins (task 8.2, round
//! 62 budget split out of `builtins/math.rs`): the angle family
//! (`sin`/`cos`/`tan`/`asin`/`acos`/`atan`/`atan2`), `hypot`, `clamp`, `random`,
//! and the dart-shaped `$name:` type/unit-compatibility error helpers (`num_as`,
//! `unit_clash`, `unitless_num`, `angle_rad`) the main file shares.

use super::Or;
use super::math::unitless;
use crate::error::{ErrorKind, SassError};
use crate::value::{SassValue, convert_number};

/// Resolve an angle argument to radians: unitless means radians; `deg`/`grad`/
/// `rad`/`turn` are converted through the degree base. A non-angle unit reads
/// dart's `$number: Expected <value> to have an angle unit (deg, grad, rad, turn).`
pub(super) fn angle_rad(c: &super::Call, i: usize) -> Or<f64> {
    let (v, u) = num_as(c, i, "number")?;
    match u.as_deref() {
        None => Ok(v),
        Some(u @ ("deg" | "grad" | "rad" | "turn")) => {
            let deg = convert_number(v, Some(u), Some("deg")).expect("angle group");
            Ok(deg * std::f64::consts::PI / 180.0)
        }
        Some(_) => Err(SassError::new(
            ErrorKind::Builtins,
            format!(
                "$number: Expected {} to have an angle unit (deg, grad, rad, turn).",
                c.positional[i].to_css()
            ),
        )),
    }
}

/// A named argument that must be a unitless number: a non-number reads
/// `$<name>: <value> is not a number.` (via `num_as`) and a dimensioned one
/// `$<name>: Expected <value> to have no units.`.
pub(super) fn unitless_num(c: &super::Call, i: usize, name: &str) -> Or<f64> {
    let (v, u) = num_as(c, i, name)?;
    if u.is_some() {
        return Err(SassError::new(
            ErrorKind::Builtins,
            format!(
                "${name}: Expected {} to have no units.",
                c.positional[i].to_css()
            ),
        ));
    }
    Ok(v)
}

/// Wrap a radian result as the golden's `deg`-unit angle; `NaN` propagates so an
/// out-of-domain inverse reads `calc(NaN * 1deg)`.
pub(super) fn from_rad(rad: f64) -> SassValue {
    SassValue::Number {
        value: rad * 180.0 / std::f64::consts::PI,
        unit: Some("deg".into()),
    }
}

pub(super) fn sin(c: &super::Call) -> Or<SassValue> {
    Ok(unitless(angle_rad(c, 0)?.sin()))
}
pub(super) fn cos(c: &super::Call) -> Or<SassValue> {
    Ok(unitless(angle_rad(c, 0)?.cos()))
}
pub(super) fn tan(c: &super::Call) -> Or<SassValue> {
    Ok(unitless(angle_rad(c, 0)?.tan()))
}
pub(super) fn asin(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_num(c, 0, "number")?.asin()))
}
pub(super) fn acos(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_num(c, 0, "number")?.acos()))
}
pub(super) fn atan(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_num(c, 0, "number")?.atan()))
}
pub(super) fn atan2(c: &super::Call) -> Or<SassValue> {
    // `atan2($y, $x)`: both must be numbers of *compatible* units — dart reduces
    // `$x` into `$y`'s unit before taking the ratio (so `atan2(1cm, -10mm)` works),
    // and the result is a unitless angle regardless of the operands' unit. `$y` is
    // the reference: an incompatible `$x` names `$x: <val> and $y: <minval>`.
    let (y, uy) = num_as(c, 0, "y")?;
    let (x, ux) = num_as(c, 1, "x")?;
    if ux != uy && convert_number(1.0, ux.as_deref(), uy.as_deref()).is_none() {
        return Err(unit_clash(
            "x",
            &c.positional[1],
            &ux,
            "y",
            &c.positional[0],
            &uy,
        ));
    }
    let x = convert_number(x, ux.as_deref(), uy.as_deref()).unwrap_or(x);
    Ok(from_rad(y.atan2(x)))
}

/// `math.hypot($numbers...)`: square root of the sum of squares, normalizing
/// every argument into the first argument's unit. Errors mirror the goldens: a
/// non-number reads `<value> is not a number.`, and a unit clash reads
/// `$numbers[i]: <value> and $numbers[1]: <first> have incompatible units[. (one
/// has units and the other doesn't)]` — the reference is always the first
/// argument and the earliest clash (scanned left to right) is reported.
pub(super) fn hypot(c: &super::Call) -> Or<SassValue> {
    let mut vals: Vec<(f64, Option<String>)> = Vec::with_capacity(c.positional.len());
    if c.positional.is_empty() {
        return Err(SassError::new(
            ErrorKind::Builtins,
            "At least one argument must be passed.",
        ));
    }
    for (i, arg) in c.positional.iter().enumerate() {
        let SassValue::Number { value, unit } = arg else {
            return Err(SassError::new(
                ErrorKind::Builtins,
                format!("{} is not a number.", arg.to_css()),
            ));
        };
        if i > 0 {
            let (_, u0) = &vals[0];
            let compatible =
                unit == u0 || convert_number(1.0, unit.as_deref(), u0.as_deref()).is_some();
            if !compatible {
                let suffix = if unit.is_some() != u0.is_some() {
                    " (one has units and the other doesn't)"
                } else {
                    ""
                };
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$numbers[{}]: {} and $numbers[1]: {} have incompatible units{}.",
                        i + 1,
                        arg.to_css(),
                        c.positional[0].to_css(),
                        suffix
                    ),
                ));
            }
        }
        vals.push((*value, unit.clone()));
    }
    let u0 = vals[0].1.clone();
    let mut sum = 0.0f64;
    for (v, u) in &vals {
        let cv = convert_number(*v, u.as_deref(), u0.as_deref()).unwrap_or(*v);
        sum += cv * cv;
    }
    Ok(SassValue::Number {
        value: sum.sqrt(),
        unit: u0,
    })
}

/// `math.clamp($min, $number, $max)` — the golden's parameter order is (min, number,
/// max). It is equivalent to `math.max($min, math.min($number, $max))`: the
/// number is capped at `$max` (keeping the lower operand's unit) then raised to
/// `$min`, so a value sitting on a boundary serializes in that bound's unit.
pub(super) fn clamp(c: &super::Call) -> Or<SassValue> {
    // Validation first, in dart's order: $min, then $number, then $max each must
    // be a number (a non-number names the argument), and only then are the unit
    // compatibilities checked — $number vs $min before $max vs $min.
    let (mn, umn) = num_as(c, 0, "min")?;
    let (val, uv) = num_as(c, 1, "number")?;
    let (mx, umx) = num_as(c, 2, "max")?;
    if uv != umn && convert_number(1.0, uv.as_deref(), umn.as_deref()).is_none() {
        return Err(unit_clash(
            "number",
            &c.positional[1],
            &uv,
            "min",
            &c.positional[0],
            &umn,
        ));
    }
    if umx != umn && convert_number(1.0, umx.as_deref(), umn.as_deref()).is_none() {
        return Err(unit_clash(
            "max",
            &c.positional[2],
            &umx,
            "min",
            &c.positional[0],
            &umn,
        ));
    }
    let num_lt_max = convert_number(val, uv.as_deref(), umx.as_deref()).is_some_and(|x| x < mx);
    let (tv, tu) = if num_lt_max {
        (val, uv)
    } else {
        (mx, umx.clone())
    };
    let t_gt_min = convert_number(tv, tu.as_deref(), umn.as_deref()).is_some_and(|x| x > mn);
    let (rv, ru) = if t_gt_min { (tv, tu) } else { (mn, umn) };
    Ok(SassValue::Number {
        value: rv,
        unit: ru,
    })
}

/// Positional argument `i` as a `(value, unit)` pair, or a `$name: <value> is not
/// a number.` error naming the offending argument (a quoted string keeps its
/// quotes through `to_css`).
pub(super) fn num_as(c: &super::Call, i: usize, name: &str) -> Or<(f64, Option<String>)> {
    match &c.positional[i] {
        SassValue::Number { value, unit } => Ok((*value, unit.clone())),
        other => Err(SassError::new(
            ErrorKind::Builtins,
            format!("${name}: {} is not a number.", other.to_css()),
        )),
    }
}

/// The golden's incompatible-units message: `$<operand>: <value> and $<base>: <base>
/// have incompatible units`, with a `(one has units and the other doesn't)` suffix
/// when exactly one side is unitless. `base` is the operand dart reduces to — `$min`
/// for `clamp`, `$y` for `atan2`.
pub(super) fn unit_clash(
    operand: &str,
    value: &SassValue,
    unit: &Option<String>,
    base: &str,
    base_value: &SassValue,
    base_unit: &Option<String>,
) -> SassError {
    let suffix = if unit.is_some() != base_unit.is_some() {
        " (one has units and the other doesn't)"
    } else {
        ""
    };
    SassError::new(
        ErrorKind::Builtins,
        format!(
            "${operand}: {} and ${base}: {} have incompatible units{suffix}.",
            value.to_css(),
            base_value.to_css()
        ),
    )
}

/// `math.random($limit: null)`. dart's *seeded* PRNG is not reproduced here (the
/// exact random-value goldens are out of scope), but the `$limit` validation is
/// golden-accurate and runs before any value is produced: a non-number, a
/// non-integer (beyond float tolerance), or a limit that is not greater than zero
/// raise dart's messages. No argument / null yields a unitless fraction; a
/// positive integer limit yields an integer in `[1, $limit]` (so `random(1)`
/// deterministically returns `1`).
pub(super) fn random(c: &super::Call) -> Or<SassValue> {
    if c.positional.is_empty() || matches!(&c.positional[0], SassValue::Null) {
        return Ok(unitless(rand_u64() as f64 / u64::MAX as f64));
    }
    let (v, _u) = num_as(c, 0, "limit")?;
    let r = v.round();
    if (v - r).abs() > 1e-9 {
        return Err(SassError::new(
            ErrorKind::Builtins,
            format!("$limit: {} is not an int.", c.positional[0].to_css()),
        ));
    }
    if r < 1.0 {
        return Err(SassError::new(
            ErrorKind::Builtins,
            format!(
                "$limit: Must be greater than 0, was {}.",
                c.positional[0].to_css()
            ),
        ));
    }
    let n = r as u64;
    Ok(unitless(1.0 + (rand_u64() % n) as f64))
}

/// A time-seeded xorshift64* step — genuinely non-deterministic (as `math.random`
/// should be), with no shared mutable state.
pub(super) fn rand_u64() -> u64 {
    let mut x = seed_nanos() ^ 0x2545_F491_4F6C_DD1D;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Wall-clock nanos everywhere the stock std has a clock: native targets and
/// `wasm32-wasip1` (`clock_time_get`).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(super) fn seed_nanos() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

/// `wasm32-unknown-unknown`: stock-std `SystemTime::now()` **panics** there, so
/// the seed degrades to a monotonic per-process counter — successive
/// `math.random` calls still differ, they are just reproducible.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(super) fn seed_nanos() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TICKS: AtomicU64 = AtomicU64::new(0);
    TICKS
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
}
