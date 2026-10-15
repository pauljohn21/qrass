//! `math.*` builtins (task 8.2): division with unit handling, rounding,
//! trigonometric-free utilities (`min`/`max`/`abs`/`percentage`/`pow`/`sqrt`/
//! `log`). Numbers are f64 (D2); serialization is delegated to `SassValue`.

use super::{Or, Registry};
use crate::error::{ErrorKind, SassError};
use crate::value::{SassValue, convert_number};

pub(crate) fn register(r: &mut Registry) {
    r.ns("math", "div", 2, Some(2), div);
    r.ns("math", "round", 1, Some(1), round);
    r.ns("math", "ceil", 1, Some(1), ceil);
    r.ns("math", "floor", 1, Some(1), floor);
    r.ns("math", "abs", 1, Some(1), abs);
    r.ns("math", "percentage", 1, Some(1), percentage);
    r.ns("math", "pow", 2, Some(2), pow);
    r.ns("math", "sqrt", 1, Some(1), sqrt);
    r.ns("math", "log", 1, Some(2), log);
    r.ns("math", "min", 1, None, min);
    r.ns("math", "max", 1, None, max);
    // Legacy global spellings used by older stylesheets.
    r.def("round", 1, Some(1), round);
    r.def("ceil", 1, Some(1), ceil);
    r.def("floor", 1, Some(1), floor);
    r.def("abs", 1, Some(1), abs);
    // Bare `min(…)`/`max(…)` (and `clamp(…)`) are *plain-CSS* math functions: a
    // literal call like `min(23px, 100%)` must stay verbatim (dart-sass only
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
    if b == 0.0 {
        return Err(SassError::new(ErrorKind::Builtins, "division by zero"));
    }
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
