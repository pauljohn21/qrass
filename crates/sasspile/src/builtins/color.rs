//! The `color` domain (`core_functions/color/**`): `rgb`/`rgba` for now, the value law in
//! [`crate::color`].
//!
//! A body that cannot honour its arguments returns a **non-fatal** error, so the run keeps its verbatim
//! text — which is exactly what several fixtures in this domain ask for. `rgb(var(--foo), 3, 0.4)` is the
//! clearest one: `var()` is substituted before parsing, so the argument list can hold anything, and the
//! corpus prints the call as written (`rgb/three_args::1_of_3`). `rgb(1px, 2px, 3px)` is another: channels
//! are numbers or percentages, never a `px` quantity, and the answer is an error message this project has
//! not built yet (P7), not a guess.

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Color;
use crate::value::Value;

/// This domain's registrations.
pub(super) static COLORS: &[&Builtin] = &[&RGB, &RGBA];

/// `rgb(18, 52, 86)` → `rgb(18, 52, 86)`, `rgb(256, 0, 0, 0.5)` → `rgba(255, 0, 0, 0.5)`, and
/// `rgb(#123, 1)` → `#112233` — each from its own fixture (`three_args/unitless::numbers`,
/// `four_args/clamped::red`, `two_args::opaque_to/opaque`).
///
/// **Declared variadic on purpose, and that has a price.** The corpus gives `rgb` three *or* four
/// arguments, which a fixed-arity signature would reject at the fourth, so the count is checked in the
/// body instead. The cost: a variadic signature binds no `$label:`, so
/// `rgb($red: 0, $green: 255, $blue: 127)` (`three_args/unitless::named`) stays verbatim until the
/// registry grows an optional-parameter concept — recorded rather than hidden.
static RGB: Builtin = Builtin {
    name: "rgb",
    params: &[],
    variadic: true,
    apply: channels,
};

/// `rgba(…)` takes the same shapes; the corpus's own rows use the four-argument form
/// (`core_functions/color/rgba.hrx`).
static RGBA: Builtin = Builtin {
    name: "rgba",
    params: &[],
    variadic: true,
    apply: channels,
};

/// Read a channel, an alpha, or a colour out of one argument.
fn number(value: &Value) -> Option<f64> {
    match value.number() {
        Some(Value::Number(number, unit)) if unit.is_unitless() => Some(number),
        _ => None,
    }
}

/// A colour argument: the hex text a declaration would have kept verbatim, or a colour already built.
fn colour(value: &Value) -> Option<Color> {
    match value {
        Value::Raw(text) => Color::from_hex(text.trim()),
        Value::Color(colour) => Some(colour.clone()),
        _ => None,
    }
}

/// One `rgb`/`rgba` call: three or four channel arguments, or a colour with an optional alpha. Anything
/// else declines, which is the caller's signal to keep the text.
fn channels(args: Vec<Value>) -> Result<Value, SassError> {
    /// The accepted argument shapes. Alpha is read as its own number so a bad one declines the call rather
    /// than producing a colour with a made-up opacity.
    fn build(args: &[Value]) -> Option<Color> {
        match args {
            [red, green, blue] => Color::new(red, green, blue, 1.0),
            [red, green, blue, alpha] => {
                Some(Color::new(red, green, blue, 1.0)?.with_alpha(number(alpha)?))
            }
            [source] => colour(source),
            [source, alpha] => Some(colour(source)?.with_alpha(number(alpha)?)),
            _ => None,
        }
    }
    build(&args)
        .map(Value::Color)
        .ok_or_else(|| SassError::new("not a colour this slice can build"))
}
