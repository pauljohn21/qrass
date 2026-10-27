//! The trigonometric half of the `math` domain —
//! `core_functions/math/{sin,cos,tan,asin,acos,atan}.hrx` and `…/atan2/*`.
//!
//! Split out of the parent because these seven share two contracts no other `math` body reads — an **angle**
//! in, or a **ratio** in and a **degree** out — and because the parent file is the general number/unit law,
//! not a table of functions.
//!
//! What the fixtures decide:
//! * a unitless argument to `sin`/`cos`/`tan` **is radians**: `math.cos(1)` and `math.cos(1rad)` both give
//!   `0.5403023059` (`cos.hrx::{unitless,rad}`), while `1deg` → `0.9998476952` and `1grad` →
//!   `0.9998766325` — the angle factors are the ones `crate::unit` already carries for colour hues;
//! * any other unit is refused by name: `$number: Expected 1px to have an angle unit (deg, grad, rad,
//!   turn).` (`cos.hrx::error/unit`, the same row in `sin`/`tan`);
//! * the inverse three take **no unit at all** (`$number: Expected 1px to have no units.`,
//!   `asin.hrx::error/units`) and answer in degrees (`math.atan(1)` → `45deg`, `math.asin(-0.5)` → `-30deg`,
//!   `math.acos(1)` → `0deg`);
//! * `math.atan2($y, $x)` needs its two arguments to share a dimension — `math.atan2(1cm, -10mm)` → `135deg`
//!   converts first (`atan2/arguments.hrx::compatible_units`) — and answers degrees too.

use super::{comparable, number_arg, unitless};
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::Builtin;
use crate::unit::Unit;
use crate::value::Value;

/// The angle of a `sin`/`cos`/`tan` call, in radians. A unitless argument already is one, an angle unit is
/// converted through the table the hue accessors use (`1grad` = `0.9deg`), and anything else is the corpus's
/// own refusal — which is a **fatal** message, so `math.cos(1px)` does not print:
/// `Error: $number: Expected 1px to have an angle unit (deg, grad, rad, turn).`
fn radians(args: &[Value]) -> Result<f64, SassError> {
    let (value, unit) = number_arg("number", args)?;
    if unit.is_unitless() {
        return Ok(value);
    }
    let angle = unit
        .single()
        .filter(|name| crate::unit::measure(name).0 == "angle")
        .map(|name| (value * crate::unit::measure(name).1).to_radians());
    angle.ok_or_else(|| {
        SassError::typed(
            ErrorKind::WrongUnits,
            format!(
                "$number: Expected {} to have an angle unit (deg, grad, rad, turn).",
                Value::Number(value, unit).to_css()
            ),
        )
    })
}

/// A unitless ratio for the inverse functions: they answer in degrees, so an angle in makes no question the
/// corpus asks (`asin.hrx::error/units` is `$number: Expected 1px to have no units.`).
fn ratio(args: &[Value]) -> Result<f64, SassError> {
    let (value, unit) = number_arg("number", args)?;
    unitless("number", value, unit)
}

/// One direct function: the angle in, a plain number out (`math.sin(1deg)` → `0.0174524064` — Sass's ten
/// significant digits, which [`crate::value::format_number`] already yields for a computed value).
fn direct(value: f64, take: fn(f64) -> f64) -> Result<Value, SassError> {
    Ok(Value::Number(take(value), Unit::unitless()))
}

/// One inverse function: the ratio in, degrees out (`math.atan(-1)` → `-45deg`). An out-of-domain ratio
/// (`math.asin(2)`, and even `math.asin(1.000000000001)`) is `NaN` — the corpus prints it as
/// `calc(NaN * 1deg)`, whose wrapper is the non-finite printing law this step does not reach; the value here
/// is the one the domain answer asks for.
fn inverse(value: f64, take: fn(f64) -> f64) -> Result<Value, SassError> {
    Ok(Value::Number(take(value).to_degrees(), Unit::named("deg")))
}

pub(super) static MATH_SIN: Builtin = Builtin {
    name: "math.sin",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| radians(&args).and_then(|angle| direct(angle, f64::sin)),
};

pub(super) static MATH_COS: Builtin = Builtin {
    name: "math.cos",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| radians(&args).and_then(|angle| direct(angle, f64::cos)),
};

pub(super) static MATH_TAN: Builtin = Builtin {
    name: "math.tan",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| radians(&args).and_then(|angle| direct(angle, f64::tan)),
};

pub(super) static MATH_ASIN: Builtin = Builtin {
    name: "math.asin",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| ratio(&args).and_then(|value| inverse(value, f64::asin)),
};

pub(super) static MATH_ACOS: Builtin = Builtin {
    name: "math.acos",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| ratio(&args).and_then(|value| inverse(value, f64::acos)),
};

pub(super) static MATH_ATAN: Builtin = Builtin {
    name: "math.atan",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| ratio(&args).and_then(|value| inverse(value, f64::atan)),
};

/// `math.atan2($y, $x)`: both arguments must belong to one dimension — they are **lengths** in
/// `atan2/arguments.hrx::compatible_units` (`1cm` against `-10mm` → `135deg`, so the scale cancels and only
/// the ratio matters), either may be unitless, and the two refusals differ:
/// `$x: 1deg and $y: 1px have incompatible units.` for two dimensions, and
/// `$x: 1 and $y: 1px have incompatible units (one has units and the other doesn't).` when only one carries
/// a unit. `x` is named first in both, whatever the call order (`…::arguments.hrx::error/*`).
pub(super) static MATH_ATAN2: Builtin = Builtin {
    name: "math.atan2",
    params: &["y", "x"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (Some(y), Some(x)) = (args.first(), args.get(1)) else {
            return Err(crate::builtins::missing("y".to_string()));
        };
        let (y, y_unit) = number_arg("y", std::slice::from_ref(y))?;
        let (x, x_unit) = number_arg("x", std::slice::from_ref(x))?;
        let (y, x) = comparable(&(y, y_unit.clone()), &(x, x_unit.clone())).map_err(|why| {
            // The generic comparison message names the values bare (`1px and 2s have incompatible units.`,
            // `max.hrx`); `atan2` states them by parameter, `x` first (`atan2(1px, 1deg)` is
            // `$x: 1deg and $y: 1px have incompatible units.`).
            match why.kind {
                ErrorKind::IncompatibleUnits => SassError::typed(
                    ErrorKind::IncompatibleUnits,
                    format!(
                        "$x: {} and $y: {} have incompatible units.",
                        Value::Number(x, x_unit.clone()).to_css(),
                        Value::Number(y, y_unit.clone()).to_css()
                    ),
                ),
                _ => why,
            }
        })?;
        if y_unit.is_unitless() != x_unit.is_unitless() {
            return Err(SassError::typed(
                ErrorKind::IncompatibleUnits,
                format!(
                    "$x: {} and $y: {} have incompatible units (one has units and the other doesn't).",
                    Value::Number(x, x_unit).to_css(),
                    Value::Number(y, y_unit).to_css()
                ),
            ));
        }
        Ok(Value::Number(y.atan2(x).to_degrees(), Unit::named("deg")))
    },
};
