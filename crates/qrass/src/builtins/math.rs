//! `math.*` builtins (task 8.2): division with unit handling, rounding,
//! trigonometric-free utilities (`min`/`max`/`abs`/`percentage`/`pow`/`sqrt`/
//! `log`). Numbers are f64 (D2); serialization is delegated to `SassValue`.

use super::{Or, Registry};
use super::math_trig::{asin, atan, atan2, acos, clamp, cos, hypot, num_as, random, sin, tan, unitless_num};
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
    r.nsp(
        "math",
        "clamp",
        3,
        Some(3),
        &["min", "number", "max"],
        clamp,
    );
    r.nsp("math", "random", 0, Some(1), &["limit"], random);
    r.nsp("math", "unit", 1, Some(1), &["number"], unit);
    r.nsp("math", "is-unitless", 1, Some(1), &["number"], is_unitless);
    r.nsp(
        "math",
        "compatible",
        2,
        Some(2),
        &["number1", "number2"],
        compatible,
    );
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

/// A unitless number value.
pub(super) fn unitless(value: f64) -> SassValue {
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
    // The numerator's unit carries into the result, unless the two units are
    // equal (then they cancel to unitless), handled below.
    let unit = ua.clone();
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
    let ok = ua == ub || convert_number(1.0, ua.as_deref(), ub.as_deref()).is_some();
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
