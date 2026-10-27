//! The `math` domain (`core_functions/math/**`). Bodies only: the contract, the registry entry and the
//! arity diagnostics live in [`crate::builtins`], because the corpus states those identically for every
//! function and duplicating them per domain is how they drift.

use crate::builtins::{Builtin, missing};
use crate::unit::Unit;
use crate::value::Value;
use crate::{ErrorKind, SassError};

/// This domain's registrations, in the order the file defines them.
pub(super) static MATHS: &[&Builtin] = &[
    &MATH_DIV,
    &MATH_ROUND,
    &MATH_ABS,
    &MATH_CEIL,
    &MATH_FLOOR,
    &MATH_MIN,
    &MATH_MAX,
    &MATH_UNIT,
    &MATH_IS_UNITLESS,
    &MATH_PERCENTAGE,
    &MATH_COMPATIBLE,
];

/// `math.div(6, 3)` → `2`: numbers only, with the unit law the goldens state — same unit cancels
/// (`6px / 3px` → `2`), a unitless divisor keeps the numerator's unit (`6px / 3` → `2px`), convertible
/// units convert first (`6in / 3px` → `192`, which also fixes `1in = 96px` as the only factor the corpus
/// evidences), and anything else becomes a **rational unit** rather than a refusal:
/// `math.div(6, 3px)` is `2/px`, which `* 1px` cancels back to `2` (`div.hrx::unit/denominator`),
/// `math.div(6in, 3s)` is `2in/s` (`…::unit/incompatible`) and `math.div(6c, 3d)` is `2c/d` — unknown
/// units combine without being converted (`…::unit/unknown`).
static MATH_DIV: Builtin = Builtin {
    name: "math.div",
    params: &["number1", "number2"],
    variadic: false,
    apply: |args| match args.as_slice() {
        [from, by] => {
            let (
                Some(Value::Number(from_value, from_unit)),
                Some(Value::Number(by_value, by_unit)),
            ) = (from.number(), by.number())
            else {
                return Err(SassError::new("not a number"));
            };
            // A calculation argument is a number when its content folded: `math.div(calc(2px * 3px), 4px)`
            // is `1.5px` (`values/calculation/calc/operator/units::multiplication`).
            let (left, right) = (
                Value::Number(from_value, from_unit.clone()),
                Value::Number(by_value, by_unit.clone()),
            );
            if from_unit == by_unit {
                Ok(Value::Number(from_value / by_value, Unit::unitless()))
            } else if by_unit.is_unitless() {
                Ok(Value::Number(from_value / by_value, from_unit.clone()))
            } else {
                left.divided_by(&right)
            }
        }
        _ => Err(SassError::new("not a number")),
    },
};

/// Convertible lengths, in px, with the factors the oracle states (`libsass/units/conversion/size.hrx`).
/// An unlisted unit declines rather than being converted by a remembered number.
fn to_base(unit: &str) -> Option<f64> {
    match unit {
        "px" => Some(1.0),
        "in" => Some(96.0),
        "cm" => Some(37.7952755906),
        "mm" => Some(3.7795275591),
        "pc" => Some(16.0),
        "pt" => Some(1.3333333333),
        _ => None,
    }
}

/// `math.unit(1px)` → `"px"`, `math.unit(1)` → `""`, and the compound forms the corpus states
/// (`"px*em*rad"`, `"px/em"`, `"px^-1"`, `"px*em/(rad*s)"`) — all of them come out of
/// [`Unit::to_css`], which is the single place a unit becomes text.
static MATH_UNIT: Builtin = Builtin {
    name: "math.unit",
    params: &["number"],
    variadic: false,
    apply: |args| {
        number_arg("number", &args).map(|(_, unit)| Value::Raw(format!("\"{}\"", unit.to_css())))
    },
};

/// `math.is-unitless(1px)` → `false` — the file is `unitless.hrx`, but the function the corpus calls is
/// `math.is-unitless`; naming it after the file would have implemented nothing.
static MATH_IS_UNITLESS: Builtin = Builtin {
    name: "math.is-unitless",
    params: &["number"],
    variadic: false,
    apply: |args| number_arg("number", &args).map(|(_, unit)| Value::Bool(unit.is_unitless())),
};

/// `math.percentage(0.246)` → `24.6%`. A unit on the argument is the corpus's own message
/// (`$number: Expected 1% to have no units.` — plural “units” even for one unit, as written), and the
/// result only fits Sass's ten-significant-digit output for a computed number.
static MATH_PERCENTAGE: Builtin = Builtin {
    name: "math.percentage",
    params: &["number"],
    variadic: false,
    apply: |args| {
        let (value, unit) = number_arg("number", &args)?;
        if unit.is_unitless() {
            Ok(Value::Number(value * 100.0, Unit::named("%")))
        } else {
            Err(SassError::typed(
                ErrorKind::WrongUnits,
                format!(
                    "$number: Expected {} to have no units.",
                    Value::Number(value, unit).to_css()
                ),
            ))
        }
    },
};

/// `math.compatible(1px, 2in)` → `true`: two numbers are comparable when they carry the same unit, either
/// is unitless, or both units belong to a dimension the corpus evidences. An unknown unit **declines**
/// rather than answering `false`, because `px` vs `q` is not something the fixtures state. Each parameter
/// is validated under its own name, as `comparable.hrx::error/type/*` demands
/// (`$number1: c is not a number.`, `$number2: c is not a number.`).
static MATH_COMPATIBLE: Builtin = Builtin {
    name: "math.compatible",
    params: &["number1", "number2"],
    variadic: false,
    apply: |args| {
        let (Some(first), Some(second)) = (args.first(), args.get(1)) else {
            return Err(missing("number2".to_string()));
        };
        let (_, first_unit) = number_arg("number1", std::slice::from_ref(first))?;
        let (_, second_unit) = number_arg("number2", std::slice::from_ref(second))?;
        if first_unit == second_unit || first_unit.is_unitless() || second_unit.is_unitless() {
            return Ok(Value::Bool(true));
        }
        // Comparison is on the **dimension signature** — the multiset of dimensions on each side of the
        // fraction — so an inverse unit answers rather than declining: `math.compatible(1px, 1/1px)` is
        // `false` (`comparable.hrx::unit/to_inverse`), while `1px` vs `2em` stays `false`
        // (`…::unit/to_different`) and any unit outside the proven table still declines.
        match (dimension(&first_unit), dimension(&second_unit)) {
            (Some(left), Some(right)) => Ok(Value::Bool(left == right)),
            _ => Err(SassError::new("unit dimension not proven")),
        }
    },
};

/// `math.max(1px, 1in, 1cm)` → `1in`: the arguments are compared under the unit law and the **original**
/// value wins (it is not rewritten into a common unit).
///
/// The length factors come from the oracle itself (`libsass/units/conversion/size.hrx`: `1in` → `96px`,
/// `1cm` → `37.7952755906px`, `1mm` → `3.7795275591px`, `1pc` → `16px`, `1pt` → `1.3333333333px`), and
/// `max.hrx::error/incompatible_units` states that `px` and `s` are different dimensions.
/// Two **known** different dimensions produce the corpus's message; an unlisted unit declines to verbatim
/// instead of inventing that error, because `1px and 2q have incompatible units.` is not something the
/// fixtures assert and an unknown unit is not evidence of incompatibility.
static MATH_MIN: Builtin = Builtin {
    name: "math.min",
    params: &[],
    variadic: true,
    apply: |args| extremum(Ordering::Less, args),
};

static MATH_MAX: Builtin = Builtin {
    name: "math.max",
    params: &[],
    variadic: true,
    apply: |args| extremum(Ordering::Greater, args),
};

/// Which way an extremum picks. A named choice keeps one body for `min` and `max` rather than two copies
/// that can drift apart.
#[derive(Clone, Copy, PartialEq)]
enum Ordering {
    Less,
    Greater,
}

/// Fold the arguments down to the smallest or largest, comparing them pairwise under the unit law.
fn extremum(wants: Ordering, args: Vec<Value>) -> Result<Value, SassError> {
    let numbers = args
        .into_iter()
        .map(|value| match value {
            Value::Number(number, unit) => Ok((number, unit)),
            other => Err(SassError::typed(
                ErrorKind::NotANumber,
                format!("{} is not a number.", other.to_css()),
            )),
        })
        .collect::<Result<Vec<(f64, Unit)>, SassError>>()?;
    let (head, tail) = numbers
        .split_first()
        .expect("variadic arity is checked by `evaluate`");
    tail.iter()
        .try_fold(head.clone(), |best, candidate| {
            comparable(&best, candidate).map(|(left, right)| {
                match (wants, left.partial_cmp(&right)) {
                    (Ordering::Less, Some(core::cmp::Ordering::Greater)) => candidate.clone(),
                    (Ordering::Greater, Some(core::cmp::Ordering::Less)) => candidate.clone(),
                    _ => best,
                }
            })
        })
        .map(|(number, unit)| Value::Number(number, unit))
}

/// The two magnitudes to compare, in a common scale: equal units (or either side unitless) compare as
/// written, known convertible lengths are scaled to px, and known different dimensions are the corpus's
/// `1px and 2s have incompatible units.` — anything else declines.
fn comparable(a: &(f64, Unit), b: &(f64, Unit)) -> Result<(f64, f64), SassError> {
    if a.1 == b.1 || a.1.is_unitless() || b.1.is_unitless() {
        return Ok((a.0, b.0));
    }
    match (
        a.1.single().and_then(to_base),
        b.1.single().and_then(to_base),
    ) {
        (Some(from_a), Some(from_b)) => Ok((a.0 * from_a, b.0 * from_b)),
        _ if dimension(&a.1).is_some() && dimension(&b.1).is_some() => Err(SassError::typed(
            ErrorKind::IncompatibleUnits,
            format!(
                "{} and {} have incompatible units.",
                Value::Number(a.0, a.1.clone()).to_css(),
                Value::Number(b.0, b.1.clone()).to_css()
            ),
        )),
        // At least one unit is outside the proven table: decline rather than claim incompatibility.
        _ => Err(SassError::new("unit comparison not modelled yet")),
    }
}

/// The dimension signature of a unit: the group each name belongs to, on both sides of the fraction, in
/// the order written — only for the groups the corpus evidences (the lengths that carry a px factor and
/// time, from `max.hrx::error/incompatible_units`, plus `em` as its own group because
/// `comparable.hrx::unit/to_different` states `math.compatible(1px, 2em)` → `false`). `ex`/`ch`/`rem`/`deg`…
/// are deliberately absent: no fixture contrasts them, so a unit outside the table makes the whole
/// signature unproven and every function that would judge it declines.
fn dimension(unit: &Unit) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    let group = |name: &str| match name {
        "px" | "in" | "cm" | "mm" | "pc" | "pt" => Some("length"),
        "s" | "ms" => Some("time"),
        "em" => Some("em"),
        _ => None,
    };
    let (numerator, denominator) = unit.names();
    Some((
        numerator
            .iter()
            .map(|name| group(name))
            .collect::<Option<Vec<&'static str>>>()?,
        denominator
            .iter()
            .map(|name| group(name))
            .collect::<Option<Vec<&'static str>>>()?,
    ))
}

/// The rounding family shares one parameter name and one type contract, both stated by the corpus:
/// `Missing argument $number.` and `$number: c is not a number.` (`core_functions/math/{round,abs,ceil,
/// floor}.hrx::error/*`). Their units are preserved (`7px / 4em` cases need compound units and are out of
/// reach), and every unitless golden here is matched exactly: `round(2.2)` → `2`, `round(-5.6)` → `-6`,
/// `round(0.2)` → `0`, `abs(-17)` → `17`, `ceil(-1.2)` → `-1`, `floor(-1.2)` → `-2`.
static MATH_ROUND: Builtin = Builtin {
    name: "math.round",
    params: &["number"],
    variadic: false,
    apply: |args| {
        number_arg("number", &args).map(|(value, unit)| Value::Number(value.round(), unit))
    },
};

static MATH_ABS: Builtin = Builtin {
    name: "math.abs",
    params: &["number"],
    variadic: false,
    apply: |args| number_arg("number", &args).map(|(value, unit)| Value::Number(value.abs(), unit)),
};

static MATH_CEIL: Builtin = Builtin {
    name: "math.ceil",
    params: &["number"],
    variadic: false,
    apply: |args| {
        number_arg("number", &args).map(|(value, unit)| Value::Number(value.ceil(), unit))
    },
};

static MATH_FLOOR: Builtin = Builtin {
    name: "math.floor",
    params: &["number"],
    variadic: false,
    apply: |args| {
        number_arg("number", &args).map(|(value, unit)| Value::Number(value.floor(), unit))
    },
};

/// A parameter that must be a number. The message is the oracle's own, and the value renders in CSS form
/// so a quoted string keeps its quotes (`$number: "0" is not a number.`) while an unquoted identifier does
/// not (`$number: c is not a number.`).
///
/// Not every builtin uses this: `math.div` with a non-number emits a deprecation **warning** and still
/// produces a value (`div.hrx::non_numeric/numerator`), so its body declines instead of failing — the
/// difference lives in the body, because that is where the corpus differs.
fn number_arg(param: &str, bound: &[Value]) -> Result<(f64, Unit), SassError> {
    // The two failures are different messages, so they stay two branches: an absent argument is
    // `Missing argument $number.` (`core_functions/math/round.hrx::error/no_args`), a present one that is
    // not a number is `$number: c is not a number.` (`…::error/type`). A calculation whose content folded
    // counts as the number it reached (`Value::number`).
    let Some(bound) = bound.first() else {
        return Err(SassError::typed(
            ErrorKind::MissingArgument,
            format!("Missing argument ${param}."),
        ));
    };
    match bound.number() {
        Some(Value::Number(value, unit)) => Ok((value, unit)),
        _ => Err(SassError::typed(
            ErrorKind::NotANumber,
            format!("${param}: {} is not a number.", bound.to_css()),
        )),
    }
}
