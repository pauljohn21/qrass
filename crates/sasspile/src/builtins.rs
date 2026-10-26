//! The builtin registry (tasks 1.7a). One `Builtin` per function: its call name exactly as written, its
//! parameter names, and its body. Argument binding and the two arity diagnostics come from the signature
//! rather than from each function, because the corpus states them identically:
//! `Missing argument $number2.` / `Only 2 arguments allowed, but 3 were passed.`
//! (`core_functions/math/div.hrx::error/*`).
//!
//! A body that cannot honour the arguments returns a **non-fatal** error, and the caller falls back to the
//! run's verbatim text. That is deliberate: `math.div(6in, 3s)` produces a compound unit (`2in/s`) which
//! this compiler does not model yet, and guessing a scalar would print a wrong number where Sass would
//! divide on.

use crate::unit::Unit;
use crate::value::Value;
use crate::{ErrorKind, SassError};

/// A builtin's contract. `params` are the labels a `$label:` argument binds to, in declaration order.
///
/// `variadic` functions (`math.min`/`math.max`) take any number of positional arguments instead:
/// `params` is empty and the corpus's own messages replace both fixed-arity ones —
/// `At least one argument must be passed.` and, for a type failure, `c is not a number.` **without** a
/// `$parameter:` prefix, because there is no parameter to name (`core_functions/math/{min,max}.hrx::error/*`).
#[derive(Debug)]
pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [&'static str],
    pub variadic: bool,
    pub apply: fn(Vec<Value>) -> Result<Value, SassError>,
}

/// `math.div(6, 3)` → `2`: numbers only, with the unit law the goldens state — same unit cancels
/// (`6px / 3px` → `2`), a unitless divisor keeps the numerator's unit (`6px / 3` → `2px`), and convertible
/// units convert first (`6in / 3px` → `192`, which also fixes `1in = 96px` as the only factor the corpus
/// evidences). Anything else declines to a verbatim fallback.
static MATH_DIV: Builtin = Builtin {
    name: "math.div",
    params: &["number1", "number2"],
    variadic: false,
    apply: |args| match args.as_slice() {
        [Value::Number(from, from_unit), Value::Number(by, by_unit)] => {
            if from_unit == by_unit {
                Ok(Value::Number(from / by, Unit::unitless()))
            } else if by_unit.is_unitless() {
                Ok(Value::Number(from / by, from_unit.clone()))
            } else if let (Some(from_base), Some(by_base)) = (
                from_unit.single().and_then(to_base),
                by_unit.single().and_then(to_base),
            ) {
                Ok(Value::Number(
                    from * from_base / (by * by_base),
                    Unit::unitless(),
                ))
            } else {
                Err(SassError::new("units are not convertible"))
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

static BUILTINS: &[&Builtin] = &[
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

/// The physical dimension of a unit, only for the groups the corpus evidences: the lengths that carry a
/// px factor and time (`max.hrx::error/incompatible_units` contrasts `1px` with `2s`), plus `em` as its own
/// dimension because `comparable.hrx::unit/to_different` states `math.compatible(1px, 2em)` → `false`.
/// `ex`/`ch`/`rem`/`deg`… are deliberately absent: no fixture contrasts them, so they stay unproven and
/// every function that would need to judge them declines.
fn dimension(unit: &Unit) -> Option<&'static str> {
    match unit.single() {
        Some("px" | "in" | "cm" | "mm" | "pc" | "pt") => Some("length"),
        Some("s" | "ms") => Some("time"),
        Some("em") => Some("em"),
        _ => None,
    }
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
    match bound.first() {
        Some(Value::Number(value, unit)) => Ok((*value, unit.clone())),
        Some(other) => Err(SassError::typed(
            ErrorKind::NotANumber,
            format!("${param}: {} is not a number.", other.to_css()),
        )),
        None => Err(SassError::typed(
            ErrorKind::MissingArgument,
            format!("Missing argument ${param}."),
        )),
    }
}

/// The signature for a callee name, if we have one. Unknown names (`color.hwb`, `var`, `map.merge`) are
/// `None`, so `calls::known_call` leaves the run exactly as it is today.
///
/// Named `signature`, not `resolve`: `fn resolve(` is a `sasslint` ban recording the deleted pull-shaped
/// engine's recursive-descent resolver, and this lookup is nothing of the kind — weakening the gate to fit
/// a new name would be the wrong trade.
pub fn signature(name: &str) -> Option<&'static Builtin> {
    BUILTINS
        .iter()
        .find(|builtin| builtin.name == name)
        .copied()
}

/// Bind given arguments to the signature, then apply. Positional arguments fill parameters in order; a
/// `$label:` argument fills its named parameter wherever it was written
/// (`math.div($number2: 3, $number1: 6)` → `2`).
pub fn evaluate(name: &str, given: Vec<(Option<String>, Value)>) -> Result<Value, SassError> {
    let builtin =
        signature(name).ok_or_else(|| SassError::new(format!("unknown builtin {name}")))?;
    // A variadic signature takes every positional argument and has no parameter names for a `$label:` to
    // bind to, so a labelled run declines (the 1.2b rule) rather than guessing which parameter was meant.
    // Its one arity rule is the corpus's own: no arguments at all is an error.
    if builtin.variadic {
        if given.is_empty() {
            return Err(SassError::typed(
                ErrorKind::MissingArgument,
                "At least one argument must be passed.",
            ));
        }
        return given
            .into_iter()
            .map(|(label, value)| match label {
                Some(_) => Err(SassError::new(
                    "named arguments are not bound for a variadic builtin",
                )),
                None => Ok(value),
            })
            .collect::<Result<Vec<Value>, SassError>>()
            .and_then(|bound| (builtin.apply)(bound));
    }
    let positional = given
        .iter()
        .filter(|(label, _)| label.is_none())
        .map(|(_, value)| value.clone())
        .collect::<Vec<Value>>();
    let named = given
        .iter()
        .filter_map(|(label, value)| label.as_ref().map(|name| (name.as_str(), value)))
        .collect::<Vec<(&str, &Value)>>();
    if given.len() > builtin.params.len() {
        return Err(too_many(builtin.params.len(), given.len()));
    }
    builtin
        .params
        .iter()
        .enumerate()
        .map(
            |(index, param)| match named.iter().find(|(label, _)| label == param) {
                Some((_, value)) => Ok((*value).clone()),
                None => positional
                    .get(index)
                    .cloned()
                    .ok_or_else(|| missing((*param).to_string())),
            },
        )
        .collect::<Result<Vec<Value>, SassError>>()
        .and_then(|bound| (builtin.apply)(bound))
}

/// `Error: Missing argument $number2.` — the first parameter left without a value.
fn missing(param: String) -> SassError {
    SassError::typed(
        ErrorKind::MissingArgument,
        format!("Missing argument ${param}."),
    )
}

/// `Error: Only 2 arguments allowed, but 3 were passed.` Both counts decline the English plural
/// independently, as the corpus shows (`Only 1 argument allowed, but 2 were passed.` and
/// `Only 0 arguments allowed, but 1 was passed.`).
fn too_many(allowed: usize, passed: usize) -> SassError {
    SassError::typed(
        ErrorKind::TooManyArguments,
        format!(
            "Only {} argument{} allowed, but {} {} passed.",
            allowed,
            if allowed == 1 { "" } else { "s" },
            passed,
            if passed == 1 { "was" } else { "were" }
        ),
    )
}
