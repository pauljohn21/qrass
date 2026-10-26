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

use crate::value::Value;
use crate::{ErrorKind, SassError};

/// A builtin's contract. `params` are the labels a `$label:` argument binds to, in declaration order.
#[derive(Debug)]
pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [&'static str],
    pub apply: fn(Vec<Value>) -> Result<Value, SassError>,
}

/// `math.div(6, 3)` → `2`: numbers only, with the unit law the goldens state — same unit cancels
/// (`6px / 3px` → `2`), a unitless divisor keeps the numerator's unit (`6px / 3` → `2px`), and convertible
/// units convert first (`6in / 3px` → `192`, which also fixes `1in = 96px` as the only factor the corpus
/// evidences). Anything else declines to a verbatim fallback.
static MATH_DIV: Builtin = Builtin {
    name: "math.div",
    params: &["number1", "number2"],
    apply: |args| match args.as_slice() {
        [Value::Number(from, from_unit), Value::Number(by, by_unit)] => {
            if from_unit == by_unit {
                Ok(Value::Number(from / by, String::new()))
            } else if by_unit.is_empty() {
                Ok(Value::Number(from / by, from_unit.clone()))
            } else if let (Some(from_base), Some(by_base)) = (to_base(from_unit), to_base(by_unit))
            {
                Ok(Value::Number(
                    from * from_base / (by * by_base),
                    String::new(),
                ))
            } else {
                Err(SassError::new("units are not convertible"))
            }
        }
        _ => Err(SassError::new("not a number")),
    },
};

/// Convertible lengths, in px. Only factors the corpus proves are listed — an unlisted unit declines
/// rather than being converted by a remembered number.
fn to_base(unit: &str) -> Option<f64> {
    match unit {
        "px" => Some(1.0),
        "in" => Some(96.0),
        _ => None,
    }
}

static BUILTINS: &[&Builtin] = &[&MATH_DIV];

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
