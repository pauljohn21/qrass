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
//!
//! ## Layout
//!
//! This file is the **spine only**: the contract, the grouped registry, and dispatch. Each domain's bodies
//! live beside it (`math.rs` today; `color.rs` arrives with 1.5), because a single file holding every
//! domain would cross the project's 500-line cap the moment colour lands. The grouping is a list of
//! domains, so adding one adds a line here and a file there — nothing is reordered and no name changes:
//! the only external surface is [`signature`] and [`evaluate`], used by `calls`, `brackets` and `expr`.

use crate::value::Value;
use crate::{ErrorKind, SassError};

mod color;
mod math;

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

/// Every domain's registrations. Searched in order, and names are unique across domains — a duplicate
/// would shadow silently, so [`signature`]'s first match is the only rule that matters.
static BUILTINS: &[&[&Builtin]] = &[math::MATHS, color::COLORS];

/// The signature for a callee name, if we have one. Unknown names (`color.hwb`, `var`, `map.merge`) are
/// `None`, so `calls::known_call` leaves the run exactly as it is today.
///
/// Named `signature`, not `resolve`: `fn resolve(` is a `sasslint` ban recording the deleted pull-shaped
/// engine's recursive-descent resolver, and this lookup is nothing of the kind — weakening the gate to fit
/// a new name would be the wrong trade.
pub fn signature(name: &str) -> Option<&'static Builtin> {
    BUILTINS
        .iter()
        .flat_map(|domain| domain.iter())
        .copied()
        .find(|builtin| builtin.name == name)
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
pub(super) fn missing(param: String) -> SassError {
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
