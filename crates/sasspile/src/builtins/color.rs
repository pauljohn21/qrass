//! The `color` domain (`core_functions/color/**`): `rgb`/`rgba`/`hsl`/`hsla` for now, with the value law
//! in [`crate::color`].
//!
//! A body that cannot honour its arguments returns a **non-fatal** error, so the run keeps its verbatim
//! text — which is what several fixtures in this domain ask for. `rgb(var(--foo), 3, 0.4)` is the clearest:
//! `var()` is substituted before parsing, so the argument list can hold anything, and the corpus prints the
//! call as written (`rgb/three_args::1_of_3`). `rgb(1px, 2px, 3px)` is another: channels are numbers or
//! percentages, never a `px` quantity, and the answer is an error message this project has not built (P7),
//! not a guess.

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Color;
use crate::value::Value;

/// This domain's registrations.
pub(super) static COLORS: &[&Builtin] = &[
    &RGB,
    &RGBA,
    &HSL,
    &HSLA,
    &COLOR_RED,
    &COLOR_GREEN,
    &COLOR_BLUE,
    &COLOR_ALPHA,
];

/// `color.red(…)`, the first channel; likewise green and blue.
///
/// Declared **variadic with one name**, deliberately: `core_functions/color/alpha.hrx::css_overloads/alpha/multi_arg`
/// writes `color.alpha(c=d, e=f, g=h)` — a Microsoft filter that must print verbatim — and a fixed arity
/// raises its `Only 1 argument allowed…` **before** any body runs, which turned that row into an unexpected
/// error. The `ledger failures <run> "unexpected error" --count` reading is what caught it: `net=26` was
/// real, but the invalid-error count had moved 53 → 55. The body now judges the count, so an unmodellable
/// call declines instead of erroring; the arity-message rows stay red and are P7's, not lost to a guess.
static COLOR_RED: Builtin = Builtin {
    name: "color.red",
    params: &["color"],
    optional: &[],
    variadic: true,
    apply: |args| channel(args, 0),
};

/// `color.green(…)`, the second channel.
static COLOR_GREEN: Builtin = Builtin {
    name: "color.green",
    params: &["color"],
    optional: &[],
    variadic: true,
    apply: |args| channel(args, 1),
};

/// `color.blue(…)`, the third channel.
static COLOR_BLUE: Builtin = Builtin {
    name: "color.blue",
    params: &["color"],
    optional: &[],
    variadic: true,
    apply: |args| channel(args, 2),
};

/// `color.alpha(red)` → `1`, `color.alpha(rgba(red, 0.42))` → `0.42`, and `color.alpha($color: …)` binds
/// by name (`core_functions/color/alpha.hrx::color/{max,middle,min,named}`).
static COLOR_ALPHA: Builtin = Builtin {
    name: "color.alpha",
    params: &["color"],
    optional: &[],
    variadic: true,
    apply: alpha_of,
};

/// `rgb(18, 52, 86)` → `rgb(18, 52, 86)`, `rgb(256, 0, 0, 0.5)` → `rgba(255, 0, 0, 0.5)`, and
/// `rgb(#123, 1)` → `#112233` — one fixture each (`three_args/unitless::numbers`,
/// `four_args/clamped::red`, `two_args::opaque_to/opaque`).
///
/// Declared **variadic, with names**: the call is overloaded — one or two arguments are a colour plus an
/// optional alpha, three or four are channels — so the body judges the count, while `params`/`optional`
/// still give the labels something to bind to (`rgb($red: 0, $green: 255, $blue: 127)` →
/// `rgb(0, 255, 127)`, `three_args/unitless::named`).
static RGB: Builtin = Builtin {
    name: "rgb",
    params: &["red", "green", "blue"],
    optional: &["alpha"],
    variadic: true,
    apply: channels,
};

/// `rgba(…)`, the four-argument spelling of the same law (`core_functions/color/rgba.hrx`).
static RGBA: Builtin = Builtin {
    name: "rgba",
    params: &["red", "green", "blue"],
    optional: &["alpha"],
    variadic: true,
    apply: channels,
};

/// `hsl(60rad, 100%, 50%)` → `hsl(197.7467707849, 100%, 50%)`, `hsl(0, 50, 50%)` → `hsl(0, 50%, 50%)`,
/// and `hsl(0, 500%, 50%)` stays out of range — each from `hsl/three_args/{units,bounds}.hrx`. The labels
/// bind as they do for `rgb` (`hsl/three_args/named.hrx`).
static HSL: Builtin = Builtin {
    name: "hsl",
    params: &["hue", "saturation", "lightness"],
    optional: &["alpha"],
    variadic: true,
    apply: hsl_channels,
};

/// `hsla(…)`, the four-argument spelling (`hsl/four_args/alpha.hrx`).
static HSLA: Builtin = Builtin {
    name: "hsla",
    params: &["hue", "saturation", "lightness"],
    optional: &["alpha"],
    variadic: true,
    apply: hsl_channels,
};

/// An alpha argument: a number, or a percentage — `hsl(0, 0%, 0%, 45.6%)` is `hsla(0, 0%, 0%, 0.456)`
/// (`hsl/four_args/alpha.hrx`, and `rgb/four_args/alpha.hrx` states the same for `rgb`).
fn alpha(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, unit) if unit.is_unitless() => Some(*number),
        Value::Number(number, unit) if unit.single() == Some("%") => Some(*number / 100.0),
        _ => None,
    }
}

/// A colour argument: text a declaration would have kept verbatim (hex **or** a CSS keyword —
/// `rgba(red, 0.42)` must read `red`, `alpha.hrx::color/middle`), or a colour already built.
/// One `rgb`/`rgba` call: three channels with an optional alpha, or a colour with an optional alpha. A
/// colour argument is text a declaration would have kept verbatim (hex **or** a CSS keyword —
/// `rgba(red, 0.42)` must read `red`, `alpha.hrx::color/middle`) or a colour already built (`Color::of`).
fn channels(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        // Channel kinds and their units are the colour's own law (`Color::new` refuses anything but a
        // number or a percentage), so the body only has to shape the arguments.
        [red, green, blue] => Color::new(red, green, blue, 1.0),
        [red, green, blue, given] => {
            alpha(given).and_then(|alpha| Color::new(red, green, blue, alpha))
        }
        [source] => Color::of(source),
        [source, given] => match (Color::of(source), alpha(given)) {
            (Some(color), Some(alpha)) => Some(color.with_alpha(alpha)),
            _ => None,
        },
        _ => None,
    };
    finished(built)
}

/// One `hsl`/`hsla` call. The alpha is read apart because the colour stores it normalised to `0–1`.
fn hsl_channels(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        [hue, saturation, lightness] => Color::hsl(hue, saturation, lightness, 1.0),
        [hue, saturation, lightness, given] => {
            alpha(given).and_then(|alpha| Color::hsl(hue, saturation, lightness, alpha))
        }
        _ => None,
    };
    finished(built)
}

fn finished(built: Option<Color>) -> Result<Value, SassError> {
    built
        .map(Value::Color)
        .ok_or_else(|| SassError::new("not a colour this slice can build"))
}

/// One channel of a colour argument, as a unitless number.
fn channel(args: Vec<Value>, index: usize) -> Result<Value, SassError> {
    let read = match access(&args) {
        Access::Colour(color) => color.byte(index),
        // A colour held in another space (an `hsl` value) has no `0–255` channel to hand out, so it
        // declines rather than reporting a number read off the wrong axes.
        Access::NotAColour => None,
        Access::TooFew => return Err(super::missing("color".to_string())),
        Access::TooMany(passed) => return Err(super::too_many(1, passed)),
    };
    match read {
        Some(number) => Ok(Value::Number(number, crate::unit::Unit::unitless())),
        None => Err(SassError::new(
            "not a colour whose channels this slice can read",
        )),
    }
}

/// `color.alpha`: the colour's own alpha, which every representation carries.
fn alpha_of(args: Vec<Value>) -> Result<Value, SassError> {
    match access(&args) {
        Access::Colour(color) => Ok(Value::Number(color.alpha(), crate::unit::Unit::unitless())),
        Access::NotAColour => Err(SassError::new("not a colour this slice can read")),
        Access::TooFew => Err(super::missing("color".to_string())),
        Access::TooMany(passed) => Err(super::too_many(1, passed)),
    }
}

/// What an accessor's argument list is: a colour, a call this compiler must leave alone, or a colour call
/// with the wrong number of arguments.
///
/// The third case cannot be left to the signature: `alpha.hrx::css_overloads/alpha/multi_arg` writes
/// `color.alpha(c=d, e=f, g=h)` (a Microsoft filter, to be printed verbatim) with the **same** shape as
/// `color.red(red, green)` (a real misuse, to be reported). What tells them apart is the arguments
/// themselves — every argument reading as a colour means a colour call — so that is the test, and an
/// unmodellable run declines to text instead of erroring.
///
/// **What this does not claim**: the two arity rows (`red::error/too_few_args`, `…/too_many_args`) are
/// still red — 0 arguments hits the spine's own variadic message before any body runs, and the golden text
/// differs. So these branches exist to keep the filter overload as text, not because the arity messages are
/// satisfied; those rows, like `error/non_legacy` and `error/type`, are P7's message work.
enum Access {
    Colour(Color),
    NotAColour,
    TooFew,
    TooMany(usize),
}

fn access(args: &[Value]) -> Access {
    if args.is_empty() {
        return Access::TooFew;
    }
    match args.iter().map(Color::of).collect::<Option<Vec<Color>>>() {
        Some(colours) if colours.len() == 1 => Access::Colour(colours[0].clone()),
        Some(colours) => Access::TooMany(colours.len()),
        None => Access::NotAColour,
    }
}
