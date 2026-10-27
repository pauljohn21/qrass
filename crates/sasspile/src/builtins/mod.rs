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

/// One argument the spine hands a **label-aware** body: the **parameter it bound to** (`""` for a
/// positional argument of a signature that declares no names) and the value.
///
/// The name is here because a body that modifies a channel has to be told *which* one: `color.change(black,
/// $red: 500)` and `color.change(black, $green: -50)` pass one number each and print `hsl(0, 5000%,
/// 98.0392156863%)` versus `hsl(120, 100%, -9.8039215686%)` (`change/rgb.hrx::red|green/out_of_range`).
/// Dropping the name is what made those two calls indistinguishable at the body.
#[derive(Debug, Clone)]
pub struct Given {
    pub param: &'static str,
    pub value: Value,
    /// Whether the call **wrote a `$label:`** for this parameter, as opposed to filling it by position.
    /// `color.change`/`color.adjust`/`color.scale` take exactly one positional argument — the colour — and
    /// refuse the rest by name (`Only one positional argument is allowed. All other arguments must be passed
    /// by name.`, `change/error/args.hrx::too_many`), and that distinction is not recoverable from the
    /// bound value: `color.change(red, 1)` and `color.change($color: red, $red: 1)` land on the same slots.
    pub label: bool,
}

impl Given {
    /// The values alone, for a body whose reading does not depend on which parameter an argument filled.
    pub fn values(given: Vec<Self>) -> Vec<Value> {
        given.into_iter().map(|argument| argument.value).collect()
    }

    /// How many arguments were written **positionally** — the count the colour calls' name-or-none rule
    /// judges, and the reason [`Given::label`] exists.
    pub fn positional(given: &[Self]) -> usize {
        given.iter().filter(|argument| !argument.label).count()
    }

    /// The value passed for one parameter, if the call named it.
    pub fn find<'a>(given: &'a [Self], param: &str) -> Option<&'a Value> {
        given
            .iter()
            .find(|argument| argument.param == param)
            .map(|argument| &argument.value)
    }

    /// Whether the call named this parameter at all — a `change` body's question for every channel, since
    /// only the named ones move.
    pub fn names(given: &[Self], param: &str) -> bool {
        given.iter().any(|argument| argument.param == param)
    }
}

/// A body that reads its arguments **by parameter** rather than by position.
///
/// Named because `Option<fn(Vec<Given>) -> Result<Value, SassError>>` is the kind of type clippy rightly
/// asks to be factored out, and because the two entry points differ in more than their shape: [`Given`] is
/// what makes `color.change` able to tell `$red:` from `$hue:`.
type LabelledBody = fn(Vec<Given>) -> Result<Value, SassError>;

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
    /// A body that must know **which parameter** each argument bound to. `None` for every body written so far,
    /// which reads its arguments positionally; `color.change` is the first call whose question cannot be
    /// answered without the label (`$red: 500` and `$green: -50` are one number each and print completely
    /// different colours), and it sets this instead of `apply`.
    ///
    /// The field is optional rather than a change to `apply`'s argument type on purpose: rewriting 29 bodies
    /// to pass through a newtype to unlock one function is how this session already produced two wrong
    /// turns, and a purely additive field leaves every existing body — and every existing fixture reading —
    /// untouched, which `ledger diff` can then prove.
    pub labelled: Option<LabelledBody>,
}

/// Every domain's registrations. Searched in order, and names are unique across domains — a duplicate
/// would shadow silently, so [`signature`]'s first match is the only rule that matters.
static BUILTINS: &[&[&Builtin]] = &[math::MATHS, color::COLORS];

/// The signature for a callee name, if we have one. Unknown names (`color.contrast`, `var`, `map.merge`) are
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
        // A variadic that declares **no** names takes every argument positionally, so an empty call has no
        // parameter to name in its message: `At least one argument must be passed.`
        // (`core_functions/math/{hypot,max,min}.hrx::error/too_few_args`). One that does declare names lets
        // the empty call through to its body, because the body knows which parameter is missing —
        // `color.change()` reports `Missing argument $color.` (`change/error/args.hrx::too_few`) and
        // `saturate()` reports `Missing argument $amount.` (`saturate.hrx::error/too_few_args`), neither of
        // which this generic message could say.
        if given.is_empty() && builtin.params.is_empty() && builtin.optional.is_empty() {
            return Err(SassError::typed(
                ErrorKind::MissingArgument,
                "At least one argument must be passed.",
            ));
        }
        if builtin.params.is_empty() && builtin.optional.is_empty() {
            return given
                .into_iter()
                .map(|(label, value)| match label {
                    // Same meaning as the check below: a label with no parameter to bind to is a refusal,
                    // and the kind is what stops the caller from resolving `$number1` as a variable.
                    Some(_) => Err(SassError::typed(
                        ErrorKind::UnknownParameter,
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
    // A `$label:` that names **no parameter of this signature** is not this builtin's call to answer.
    // dart-sass reports `No $x parameter.`; until P7 builds that message the run declines and keeps its
    // text. Doing this here rather than in each body is the point: without it a labelled argument is simply
    // dropped by the binding below, so `color.mix(#a, #b, $method: oklab)` reached the legacy sRGB mixer and
    // answered a request for another colour space — a wrong value that the pass count cannot see, because
    // those rows (`mix/explicit_method`, `named::polar_space`, `hue_interpolation`) were already failing.
    if named
        .iter()
        .any(|(label, _)| !builtin.params.contains(label) && !builtin.optional.contains(label))
    {
        return Err(SassError::typed(
            ErrorKind::UnknownParameter,
            "this signature has no parameter with that name",
        ));
    }
    // One slot per name, filled by label first and by position second, so an optional parameter written
    // out of order (`rgb(1, 2, 3, $alpha: 0.5)`) still binds to the alpha. The parameter's name travels
    // with it, because a label-aware body asks which channel the call named.
    let slots = builtin
        .params
        .iter()
        .chain(builtin.optional)
        .enumerate()
        .map(|(index, param)| {
            let label = named.iter().any(|(name, _)| name == param);
            let value = named
                .iter()
                .find(|(label, _)| label == param)
                .map(|(_, value)| (*value).clone())
                .or_else(|| positional.get(index).cloned());
            (*param, value, label)
        })
        .collect::<Vec<(&'static str, Option<Value>, bool)>>();
    // Every declared parameter up to the first absent one is handed to a **fixed** body, so it never sees a
    // hole, and a required parameter left absent is the corpus's own message
    // (`math/div.hrx::error/missing_argument` → `Missing argument $number2.`).
    let taken = slots
        .iter()
        .position(|(_, value, _)| value.is_none())
        .unwrap_or(slots.len());
    if taken < builtin.params.len() && !builtin.variadic {
        return Err(missing(builtin.params[taken].to_string()));
    }
    // A **variadic** signature lets its body judge the count, so the body sees every parameter that got a
    // value, in declaration order. Trimming at the first hole (the fixed rule) is what silently discarded a
    // label bound to anything but the leading slots: `rgb($channels: 0 255 127)`
    // (`rgb/one_arg/no_alpha.hrx::unitless/named`, which wants `rgb(0, 255, 127)`) arrived as no arguments
    // at all. Positional calls are unaffected — with no holes, both rules give the same list.
    let kept = if builtin.variadic {
        slots
    } else {
        slots.into_iter().take(taken).collect()
    };
    let given = kept
        .into_iter()
        .filter_map(|(param, value, label)| {
            Some(Given {
                param,
                label,
                value: value?,
            })
        })
        .collect::<Vec<Given>>();
    match builtin.labelled {
        Some(labelled) => labelled(given),
        // The positional view every existing body reads.
        None => (builtin.apply)(Given::values(given)),
    }
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
pub(super) fn too_many(allowed: usize, passed: usize) -> SassError {
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
