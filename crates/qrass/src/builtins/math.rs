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
    r.ns("math", "min", 0, None, min);
    r.ns("math", "max", 0, None, max);
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
    let (v, _) = num_as(c, 0, "number")?;
    Ok(unitless((v + 0.5).floor()))
}

fn ceil(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num_as(c, 0, "number")?;
    Ok(unitless(v.ceil()))
}

fn floor(c: &super::Call) -> Or<SassValue> {
    let (v, _) = num_as(c, 0, "number")?;
    Ok(unitless(v.floor()))
}

fn abs(c: &super::Call) -> Or<SassValue> {
    let (v, unit) = num_as(c, 0, "number")?;
    Ok(SassValue::Number {
        value: v.abs(),
        unit,
    })
}

fn percentage(c: &super::Call) -> Or<SassValue> {
    let v = unitless_num(c, 0, "number")?;
    Ok(SassValue::Number {
        value: v * 100.0,
        unit: Some("%".into()),
    })
}

fn pow(c: &super::Call) -> Or<SassValue> {
    let a = unitless_num(c, 0, "base")?;
    let b = unitless_num(c, 1, "exponent")?;
    Ok(unitless(a.powf(b)))
}

fn sqrt(c: &super::Call) -> Or<SassValue> {
    let v = unitless_num(c, 0, "number")?;
    Ok(unitless(v.sqrt()))
}

fn log(c: &super::Call) -> Or<SassValue> {
    let v = unitless_num(c, 0, "number")?;
    let value = match c.positional.get(1) {
        // A missing or `null` base means the natural log (dart treats `null` as
        // "no base"), so it never reaches the `$base` unit check.
        None | Some(SassValue::Null) => v.ln(),
        Some(_) => {
            let base = unitless_num(c, 1, "base")?;
            v.log(base)
        }
    };
    Ok(unitless(value))
}

/// Resolve an angle argument to radians: unitless means radians; `deg`/`grad`/
/// `rad`/`turn` are converted through the degree base. A non-angle unit reads
/// dart's `$number: Expected <value> to have an angle unit (deg, grad, rad, turn).`
fn angle_rad(c: &super::Call, i: usize) -> Or<f64> {
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
fn unitless_num(c: &super::Call, i: usize, name: &str) -> Or<f64> {
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
    Ok(from_rad(unitless_num(c, 0, "number")?.asin()))
}
fn acos(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_num(c, 0, "number")?.acos()))
}
fn atan(c: &super::Call) -> Or<SassValue> {
    Ok(from_rad(unitless_num(c, 0, "number")?.atan()))
}
fn atan2(c: &super::Call) -> Or<SassValue> {
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
    let (tv, tu) = if num_lt_max { (val, uv) } else { (mx, umx.clone()) };
    let t_gt_min = convert_number(tv, tu.as_deref(), umn.as_deref()).is_some_and(|x| x > mn);
    let (rv, ru) = if t_gt_min { (tv, tu) } else { (mn, umn) };
    Ok(SassValue::Number { value: rv, unit: ru })
}

/// Positional argument `i` as a `(value, unit)` pair, or a `$name: <value> is not
/// a number.` error naming the offending argument (a quoted string keeps its
/// quotes through `to_css`).
fn num_as(c: &super::Call, i: usize, name: &str) -> Or<(f64, Option<String>)> {
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
fn unit_clash(
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

fn unit(c: &super::Call) -> Or<SassValue> {
    let (_, u) = num_as(c, 0, "number")?;
    Ok(SassValue::Str {
        text: u.unwrap_or_default(),
        quoted: true,
    })
}

fn is_unitless(c: &super::Call) -> Or<SassValue> {
    let (_, u) = num_as(c, 0, "number")?;
    Ok(SassValue::Bool(u.is_none()))
}

fn compatible(c: &super::Call) -> Or<SassValue> {
    let (_, ua) = num_as(c, 0, "number1")?;
    let (_, ub) = num_as(c, 1, "number2")?;
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
    // `min/max($numbers…)`: dart-shaped errors — a non-number reads the bare
    // `<value> is not a number.`, no arguments reads "At least one argument must
    // be passed." (arity min lowered to 0 so this fires, as for `hypot`), and two
    // dimensioned operands of different, non-convertible units read
    // `<a> and <b> have incompatible units.`.
    if c.positional.is_empty() {
        return Err(SassError::new(
            ErrorKind::Builtins,
            "At least one argument must be passed.",
        ));
    }
    let mut nums: Vec<(f64, Option<String>)> = Vec::with_capacity(c.positional.len());
    for arg in c.positional.iter() {
        let SassValue::Number { value, unit } = arg else {
            return Err(SassError::new(
                ErrorKind::Builtins,
                format!("{} is not a number.", arg.to_css()),
            ));
        };
        nums.push((*value, unit.clone()));
    }
    // Reduce by comparing each candidate expressed in the current best's unit.
    let mut best = 0usize;
    for i in 1..nums.len() {
        let (bv, bu) = &nums[best];
        let (cv, cu) = &nums[i];
        // Equal units, both unitless, or exactly one unitless: compare raw
        // magnitudes (dart's min/max keeps the unit-bearing value as the larger).
        // Two different dimensioned units must be convertible or it is an error.
        let cand_eff = match (bu, cu) {
            (x, y) if x != y && x.is_some() && y.is_some() => {
                convert_number(*cv, cu.as_deref(), bu.as_deref()).ok_or_else(|| {
                    SassError::new(
                        ErrorKind::Builtins,
                        format!(
                            "{} and {} have incompatible units.",
                            c.positional[best].to_css(),
                            c.positional[i].to_css()
                        ),
                    )
                })?
            }
            _ => *cv,
        };
        let better = (want_min && cand_eff < *bv) || (!want_min && cand_eff > *bv);
        if better {
            best = i;
        }
    }
    Ok(c.positional[best].clone())
}
