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
    /// Parameters a call may leave out, in declaration order after `params`. `rgb` needs this because the
    /// corpus accepts **both** `rgb(18, 52, 86)` and `rgb(0, 0, 0, 0.5)` (`rgb/four_args/alpha.hrx`), which
    /// a fixed arity would reject at the fourth argument and a `variadic` signature would lose the
    /// `$red:`/`$green:`/`$blue:` labels for (`three_args/unitless::named`).
    pub optional: &'static [&'static str],
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
    // A variadic signature takes every positional argument. Its one arity rule is the corpus's own: no
    // arguments at all is an error. When it also declares names — `rgb` does, so that `$red:`/`$green:`/
    // `$blue:` can bind (`three_args/unitless::named`) — the run falls through to the shared binding
    // below, which skips the required-parameter check; the body decides how many arguments it accepts,
    // because `rgb` is genuinely overloaded (`rgb(#123, 1)` is two arguments, `rgb(1, 2, 3)` is three).
    if builtin.variadic {
        if given.is_empty() {
            return Err(SassError::typed(
                ErrorKind::MissingArgument,
                "At least one argument must be passed.",
            ));
        }
        if builtin.params.is_empty() && builtin.optional.is_empty() {
            return given
                .into_iter()
                .map(|(label, value)| match label {
                    Some(_) => Err(SassError::new(
                        "named arguments have no parameter to bind to here",
                    )),
                    None => Ok(value),
                })
                .collect::<Result<Vec<Value>, SassError>>()
                .and_then(|bound| (builtin.apply)(bound));
        }
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
    let allowed = builtin.params.len() + builtin.optional.len();
    if !builtin.variadic && given.len() > allowed {
        return Err(too_many(allowed, given.len()));
    }
    // One slot per name, filled by label first and by position second, so an optional parameter written
    // out of order (`rgb(1, 2, 3, $alpha: 0.5)`) still binds to the alpha.
    let slots = builtin
        .params
        .iter()
        .chain(builtin.optional)
        .enumerate()
        .map(|(index, param)| {
            named
                .iter()
                .find(|(label, _)| label == param)
                .map(|(_, value)| (*value).clone())
                .or_else(|| positional.get(index).cloned())
        })
        .collect::<Vec<Option<Value>>>();
    // Every declared parameter up to the first absent one is handed to the body, so a body never sees a
    // hole. A required parameter left absent is the corpus's own message — but only for a fixed signature;
    // a variadic one lets its body judge the count.
    let taken = slots
        .iter()
        .position(Option::is_none)
        .unwrap_or(slots.len());
    if taken < builtin.params.len() && !builtin.variadic {
        return Err(missing(builtin.params[taken].to_string()));
    }
    (builtin.apply)(
        slots
            .into_iter()
            .take(taken)
            .flatten()
            .collect::<Vec<Value>>(),
    )
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
