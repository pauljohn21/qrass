//! The colour **constructors** (`rgb`/`rgba`/`hsl`/`hsla`) — the calls that build a [`Color`] rather than
//! read or move one. Split out of the domain root because the corpus treats construction as its own law:
//! what decides the output form here (`Representation`) is not what the axis operations decide later.

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Color;
use crate::value::Value;

/// `rgb(18, 52, 86)` → `rgb(18, 52, 86)`, `rgb(256, 0, 0, 0.5)` → `rgba(255, 0, 0, 0.5)`, and
/// `rgb(#123, 1)` → `#112233` — one fixture each (`three_args/unitless::numbers`,
/// `four_args/clamped::red`, `two_args::opaque_to/opaque`).
///
/// Declared **variadic, with names**: the call is overloaded — one or two arguments are a colour plus an
/// optional alpha, three or four are channels — so the body judges the count, while `params`/`optional`
/// still give the labels something to bind to (`rgb($red: 0, $green: 255, $blue: 127)` →
/// `rgb(0, 255, 127)`, `three_args/unitless::named`). `channels` is a name too, because CSS Color 4's
/// one-argument spelling is written that way (`rgb($channels: 0 255 127)`, `rgb/one_arg/no_alpha.hrx::unitless/named`).
pub(super) static RGB: Builtin = Builtin {
    name: "rgb",
    params: &["red", "green", "blue"],
    optional: &["alpha", "channels"],
    variadic: true,
    apply: channels,
};

/// `rgba(…)`, the four-argument spelling of the same law (`core_functions/color/rgba.hrx`).
pub(super) static RGBA: Builtin = Builtin {
    name: "rgba",
    params: &["red", "green", "blue"],
    optional: &["alpha", "channels"],
    variadic: true,
    apply: channels,
};

/// `hsl(60rad, 100%, 50%)` → `hsl(197.7467707849, 100%, 50%)`, `hsl(0, 50, 50%)` → `hsl(0, 50%, 50%)`,
/// and `hsl(0, 500%, 50%)` stays out of range — each from `hsl/three_args/{units,bounds}.hrx`. The labels
/// bind as they do for `rgb` (`hsl/three_args/named.hrx`).
pub(super) static HSL: Builtin = Builtin {
    name: "hsl",
    params: &["hue", "saturation", "lightness"],
    optional: &["alpha", "channels"],
    variadic: true,
    apply: hsl_channels,
};

/// `hsla(…)`, the four-argument spelling (`hsl/four_args/alpha.hrx`).
pub(super) static HSLA: Builtin = Builtin {
    name: "hsla",
    params: &["hue", "saturation", "lightness"],
    optional: &["alpha", "channels"],
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

/// One `rgb`/`rgba` call: three channels with an optional alpha, a colour with an optional alpha, or CSS
/// Color 4's **one-argument space-separated** spelling. A colour argument is text a declaration would have
/// kept verbatim (hex **or** a CSS keyword — `rgba(red, 0.42)` must read `red`, `alpha.hrx::color/middle`) or
/// a colour already built (`Color::of`).
fn channels(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        // Channel kinds and their units are the colour's own law (`Color::new` refuses anything but a
        // number or a percentage), so the body only has to shape the arguments.
        [red, green, blue] => Color::new(red, green, blue, 1.0),
        [red, green, blue, given] => {
            alpha(given).and_then(|alpha| Color::new(red, green, blue, alpha))
        }
        // `rgb(18 52 86)` → `rgb(18, 52, 86)` (`rgb/one_arg/no_alpha.hrx::unitless/numbers`), and a label
        // binds the whole list (`…::unitless/named` is `rgb($channels: 0 255 127)`). Exactly three
        // components: the modern spelling takes its alpha after a `/`, which is a separate law with its own
        // fixtures (`rgb/one_arg/alpha.hrx`) and is not guessed here.
        [Value::Space(parts)] => match parts.as_slice() {
            [red, green, blue] => Color::new(red, green, blue, 1.0),
            _ => None,
        },
        // `rgb(18 52 86 / 0.5)` → `rgba(18, 52, 86, 0.5)` (`rgb/one_arg/alpha.hrx::in_gamut/partial`): the
        // slash list's second element is the alpha, read under the same law as the comma form's fourth
        // argument — so a percentage alpha converts (`…::in_gamut/percent` gives `50%` → `0.5`) and an
        // alpha of one returns the three-argument spelling (`…::in_gamut/opaque`).
        [Value::Slash(list)] => match list.as_slice() {
            [Value::Space(parts), given] => match parts.as_slice() {
                [red, green, blue] => {
                    alpha(given).and_then(|alpha| Color::new(red, green, blue, alpha))
                }
                _ => None,
            },
            _ => None,
        },
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
        // `hsl(0 100% 50%)` → `hsl(0, 100%, 50%)` (`hsl/one_arg/no_alpha.hrx::in_gamut/red`): the modern
        // spelling's axes follow the same laws as the comma form — the hue folds to degrees
        // (`…::units/hue/deg` is `hsl(0deg 100% 50%)`), saturation and lightness gain their `%`
        // (`…::units/saturation/unitless` → `hsl(0, 50%, 50%)`), and neither is clamped
        // (`…::out_of_gamut/saturation/above` keeps `500%`) — which is why this arm only has to re-shape
        // the arguments, not re-read them.
        [Value::Space(parts)] => match parts.as_slice() {
            [hue, saturation, lightness] => Color::hsl(hue, saturation, lightness, 1.0),
            _ => None,
        },
        // The alpha-after-a-slash spelling, same law as `rgb`: `hsl(0 100% 50% / 1.1)` clamps the alpha to
        // one and so prints the three-argument form, while `/ -0.1` clamps to `0` and keeps `hsla(…)`
        // (`hsl/one_arg/alpha.hrx::clamped/alpha/{above,below}`). Axes keep their own rules unchanged
        // (`…::clamped/saturation` gives `0%`, `…::clamped/lightness` keeps `9999%`).
        [Value::Slash(list)] => match list.as_slice() {
            [Value::Space(parts), given] => match parts.as_slice() {
                [hue, saturation, lightness] => {
                    alpha(given).and_then(|alpha| Color::hsl(hue, saturation, lightness, alpha))
                }
                _ => None,
            },
            _ => None,
        },
        _ => None,
    };
    finished(built)
}

fn finished(built: Option<Color>) -> Result<Value, SassError> {
    built
        .map(Value::Color)
        .ok_or_else(|| SassError::new("not a colour this slice can build"))
}
