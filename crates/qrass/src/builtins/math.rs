//! `math.*` builtins (task 8.2): division with unit handling, rounding,
//! trigonometric-free utilities (`min`/`max`/`abs`/`percentage`/`pow`/`sqrt`/
//! `log`). Numbers are f64 (D2); serialization is delegated to `SassValue`.

use super::{Or, Registry};
use crate::error::{ErrorKind, SassError};
use crate::value::{SassValue, convert_number};

pub(crate) fn register(r: &mut Registry) {
    // `nsp` declares the golden-declared parameter names so named-argument calls
    // (`math.abs($number: 1)`, `math.clamp($min: 0, $number: 1, $max: 2)`) fold
    // into the right positional slots in `builtins::call`. Variadic `min`/`max`/
    // `hypot` keep `ns`: the goldens forbid naming a `…` parameter, so there is nothing
    // to fold.
    r.nsp("math", "div", 2, Some(2), &["number1", "number2"], div);
    r.nsp("math", "round", 1, Some(1), &["number"], round);
    r.nsp("math", "ceil", 1, Some(1), &["number"], ceil);
    r.nsp("math", "floor", 1, Some(1), &["number"], floor);
    r.nsp("math", "abs", 1, Some(1), &["number"], abs);
    r.nsp("math", "percentage", 1, Some(1), &["number"], percentage);
    r.nsp("math", "pow", 2, Some(2), &["base", "exponent"], pow);
    r.nsp("math", "sqrt", 1, Some(1), &["number"], sqrt);
    r.nsp("math", "log", 1, Some(2), &["number", "base"], log);
    r.ns("math", "min", 1, None, min);
    r.ns("math", "max", 1, None, max);
    // Trig: `sin`/`cos`/`tan` take an angle (unitless = radians, or a
    // deg/grad/turn unit) and return a unitless ratio; the inverse family takes a
    // unitless ratio and returns an angle in `deg` (the golden's convention). The rest
    // are unit helpers plus `clamp`/`hypot`.
    r.nsp("math", "sin", 1, Some(1), &["number"], sin);
    r.nsp("math", "cos", 1, Some(1), &["number"], cos);
    r.nsp("math", "tan", 1, Some(1), &["number"], tan);
    r.nsp("math", "asin", 1, Some(1), &["number"], asin);
    r.nsp("math", "acos", 1, Some(1), &["number"], acos);
    r.nsp("math", "atan", 1, Some(1), &["number"], atan);
    r.nsp("math", "atan2", 2, Some(2), &["y", "x"], atan2);
    r.ns("math", "hypot", 0, None, hypot);
    r.nsp("math", "clamp", 3, Some(3), &["min", "number", "max"], clamp);
    r.nsp("math", "unit", 1, Some(1), &["number"], unit);
    r.nsp("math", "is-unitless", 1, Some(1), &["number"], is_unitless);
    r.nsp("math", "compatible", 2, Some(2), &["number1", "number2"], compatible);
    // Legacy global spellings used by older stylesheets.
    r.def("round", 1, Some(1), round);
    r.def("ceil", 1, Some(1), ceil);
    r.def("floor", 1, Some(1), floor);
    r.def("abs", 1, Some(1), abs);
    // Bare `min(…)`/`max(…)` (and `clamp(…)`) are *plain-CSS* math functions: a
    // literal call like `min(23px, 100%)` must stay verbatim (the goldens only
    // computes Sass math via the namespaced `math.min`), so they are NOT
    // registered globally — the evaluator routes them through CSS passthrough.
    r.def("percentage", 1, Some(1), percentage);
}

/// Coerce a positional argument to `(value, unit)`.
fn num(c: &super::Call, i: usize) -> Or<(f64, Option<String>)> {
    match c.positional.get(i) {
        Some(SassValue::Number { value, unit }) => Ok((*value, unit.clone())),
        Some(other) => Err(SassError::new(
            ErrorKind::Builtins,
            format!("expected a number, given {}", other.type_name()),
        )),
        None => Err(SassError::new(ErrorKind::Builtins, "expected a number")),
    }
}

fn unitless(value: f64) -> SassValue {
    SassValue::Number { value, unit: None }
}

fn div(c: &super::Call) -> Or<SassValue> {
    let (a, ua) = num(c, 0)?;
    let (b, ub) = num(c, 1)?;
    // the goldens follow IEEE-754 here: `math.div(1, 0)` is `infinity` (rendered
    // `calc(infinity)`), `math.div(0, 0)` is `NaN` — it does NOT raise on a zero
    // divisor, and the spec builds infinity via `math.div(1, 0)`.
    let b2 = match convert_number(b, ub.as_deref(), ua.as_deref()) {
        Some(v) => v,
        None => b,
    };
    let unit = if ua.is_none() || ub.is_none() || ua == ub {
        ua.clone()
    } else {
        ua.clone()
    };
    // Dividing equal units cancels; keep the numerator unit otherwise.
    let unit = if ua == ub { None } else { unit };
    Ok(SassValue::Number {
        value: a / b2,
        unit,
    })
}

fn round(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num(c, 0)?;
    Ok(unitless((v + 0.5).floor()))
}

fn ceil(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num(c, 0)?;
    Ok(unitless(v.ceil()))
}

fn floor(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num(c, 0)?;
    Ok(unitless(v.floor()))
}

fn abs(c: &super::Call) -> Or<SassValue> {
    let (v, unit) = num(c, 0)?;
    Ok(SassValue::Number {
        value: v.abs(),
        unit,
    })
}

fn percentage(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num(c, 0)?;
    Ok(SassValue::Number {
        value: v * 100.0,
        unit: Some("%".into()),
    })
}

fn pow(c: &super::Call) -> Or<SassValue> {
    let (a, _) = num(c, 0)?;
    let (b, _) = num(c, 1)?;
    Ok(unitless(a.powf(b)))
}

fn sqrt(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num(c, 0)?;
    Ok(unitless(v.sqrt()))
}

fn log(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num(c, 0)?;
    let value = match num(c, 1) {
        Ok((base, _)) => v.log(base),
        Err(_) => v.ln(),
    };
    Ok(unitless(value))
}

/// Resolve an angle argument to radians: unitless means radians; `deg`/`grad`/
/// `rad`/`turn` are converted through the degree base.
fn angle_rad(c: &super::Call, i: usize) -> Or<f64> {
    let (v, u) = num(c, i)?;
    match u.as_deref() {
        None => Ok(v),
        Some(u @ ("deg" | "grad" | "rad" | "turn")) => {
            let deg = convert_number(v, Some(u), Some("deg")).expect("angle group");
            Ok(deg * std::f64::consts::PI / 180.0)
        }
        Some(_) => Err(SassError::new(
            ErrorKind::Builtins,
            "$number: expected an angle unit (deg, grad, rad, turn) or no units".to_string(),
        )),
    }
}

/// A ratio argument that must carry no units (inverse trig / atan2).
fn unitless_arg(c: &super::Call, i: usize) -> Or<f64> {
    let (v, u) = num(c, i)?;
    if u.is_some() {
        return Err(SassError::new(
            ErrorKind::Builtins,
            "$number: expected a unitless number".to_string(),
        ));
    }
    Ok(v)
}

/// Wrap a radian result as the golden's `deg`-unit angle; `NaN` propagates so an
/// out-of-domain inverse reads `calc(NaN * 1deg)`.
fn from_rad(rad: f64) -> SassValue {
    SassValue::Number {
        value: rad * 180.0 / std::f64::consts::PI,
        unit: Some("deg".into()),
    }
}

fn sin(c: &super::Call) -> Or<SassValue> {
    Ok(unitless(angle_rad(c, 0)?.sin()))
}
fn cos(c: &super::Call) -> Or<SassValue> {
    Ok(unitless(angle_rad(c, 0)?.cos()))
}
fn tan(c: &super::Call) -> Or<SassValue> {
    Ok(unitless(angle_rad(c, 0)?.tan()))
}
fn asin(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_arg(c, 0)?.asin()))
}
fn acos(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_arg(c, 0)?.acos()))
}
fn atan(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_arg(c, 0)?.atan()))
}
fn atan2(c: &super::Call) -> Or<SassValue> {
    let y = unitless_arg(c, 0)?;
    let x = unitless_arg(c, 1)?;
    Ok(from_rad(y.atan2(x)))
}

/// `math.hypot($numbers...)`: square root of the sum of squares, normalizing
/// every argument into the first argument's unit. Errors mirror the goldens: a
/// non-number reads `<value> is not a number.`, and a unit clash reads
/// `$numbers[i]: <value> and $numbers[1]: <first> have incompatible units[. (one
/// has units and the other doesn't)]` — the reference is always the first
/// argument and the earliest clash (scanned left to right) is reported.
fn hypot(c: &super::Call) -> Or<SassValue> {
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
            let compatible = unit == u0
                || convert_number(1.0, unit.as_deref(), u0.as_deref()).is_some();
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
fn clamp(c: &super::Call) -> Or<SassValue> {
    let (mn, umn) = num(c, 0)?;
    let (val, uv) = num(c, 1)?;
    let (mx, umx) = num(c, 2)?;
    let num_lt_max = convert_number(val, uv.as_deref(), umx.as_deref()).is_some_and(|x| x < mx);
    let (tv, tu) = if num_lt_max { (val, uv) } else { (mx, umx.clone()) };
    let t_gt_min = convert_number(tv, tu.as_deref(), umn.as_deref()).is_some_and(|x| x > mn);
    let (rv, ru) = if t_gt_min { (tv, tu) } else { (mn, umn) };
    Ok(SassValue::Number { value: rv, unit: ru })
}

fn unit(c: &super::Call) -> Or<SassValue> {
    let (_, u) = num(c, 0)?;
    Ok(SassValue::Str {
        text: u.unwrap_or_default(),
        quoted: true,
    })
}

fn is_unitless(c: &super::Call) -> Or<SassValue> {
    let (_, u) = num(c, 0)?;
    Ok(SassValue::Bool(u.is_none()))
}

fn compatible(c: &super::Call) -> Or<SassValue> {
    let (_, ua) = num(c, 0)?;
    let (_, ub) = num(c, 1)?;
    let ok = ua == ub
        || convert_number(1.0, ua.as_deref(), ub.as_deref()).is_some();
    Ok(SassValue::Bool(ok))
}

fn min(c: &super::Call) -> Or<SassValue> {
    extremum(c, true)
}

fn max(c: &super::Call) -> Or<SassValue> {
    extremum(c, false)
}

fn extremum(c: &super::Call, want_min: bool) -> Or<SassValue> {
    let mut best: Option<(f64, SassValue)> = None;
    for i in 0..c.positional.len() {
        let (v, _) = num(c, i)?;
        let better = match &best {
            None => true,
            Some((b, _)) => (want_min && v < *b) || (!want_min && v > *b),
        };
        if better {
            best = Some((v, c.positional[i].clone()));
        }
    }
    best.map(|(_, v)| v)
        .ok_or_else(|| SassError::new(ErrorKind::Builtins, "expected at least one number"))
}
