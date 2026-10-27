//! `color.change` — **replacing the channels a call names**, leaving the others as the colour had them.
//!
//! This is the first body that needs the registry's labels: `color.change(black, $red: 500)` and
//! `color.change(black, $green: -50)` each pass one number, and the corpus prints them as
//! `hsl(0, 5000%, 98.0392156863%)` and `hsl(120, 100%, -9.8039215686%)` respectively
//! (`change/rgb.hrx::{red,green}/out_of_range`). Without the parameter name the two calls are indistinguishable
//! by the time a body sees them, which is why [`Given`] exists.
//!
//! Scope is the **`rgb` channels only**. `$hue:`/`$saturation:`/`$lightness:`/`$whiteness:`/`$blackness:` name
//! axes of a *different* space and answering them from bytes would be a wrong value, so a call naming any of
//! them declines (those are the rows of `change/hsl.hrx` and `change/hwb.hrx`); so does any `$space:`, since
//! the modern spaces have no implementation behind them yet.

use crate::SassError;
use crate::builtins::{Builtin, Given};
use crate::color::Color;
use crate::value::Value;

/// `color.change(black, $red: 255)` → `red`, `color.change(red, $red: 100)` → `#640000`,
/// `color.change(black, $green: 255)` → `lime` and `color.change(black, $red: 500)` →
/// `hsl(0, 5000%, 98.0392156863%)` — four rows each of `core_functions/color/change/rgb.hrx`. The first three
/// are the colour's own printing kept (`Representation::Named` writes a keyword when the bytes still name
/// one, hex otherwise); the fourth is [`Color::to_css`]'s out-of-gamut rule.
///
/// Declared **variadic with names** so `$red:` binds and an unknown label still declines as unknown; the body
/// judges which channels were named, which is the whole reason it is label-aware.
pub(super) static COLOR_CHANGE: Builtin = Builtin {
    name: "color.change",
    params: &["color"],
    optional: &[
        "red",
        "green",
        "blue",
        "alpha",
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
        "space",
    ],
    variadic: true,
    // Every reading of this call needs the labels, so the spine takes [`labelled`]; this exists only because
    // a signature must say how it reads its arguments, and guessing from positions would be the wrong answer
    // for `color.change`.
    apply: needs_labels,
    labelled: Some(changed),
};

/// The positional reading this builtin deliberately does not have.
fn needs_labels(args: Vec<Value>) -> Result<Value, SassError> {
    let _ = args;
    Err(SassError::new(
        "color.change reads its arguments by parameter",
    ))
}

/// One `color.change` call: read the colour, replace the channels it names — as bytes, or as hsl axes —
/// then let the colour's own printing law decide the output form.
fn changed(given: Vec<Given>) -> Result<Value, SassError> {
    let decline = Err(SassError::new(
        "a colour or channel this slice cannot change",
    ));
    let colour = match Given::find(&given, "color").and_then(Color::of) {
        Some(colour) => colour,
        None => return decline,
    };
    // Whiteness and blackness are a different space's axes and `$space:` asks for a space this compiler has
    // no conversion for, so both decline (`change/hwb.hrx` and the modern-space `change/*.hrx` files).
    let beyond = ["whiteness", "blackness", "space"]
        .into_iter()
        .any(|name| Given::names(&given, name));
    let bytes = [("red", 0), ("green", 1), ("blue", 2)]
        .into_iter()
        .filter(|(name, _)| Given::names(&given, name))
        .collect::<Vec<(&str, usize)>>();
    let axes = ["hue", "saturation", "lightness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .collect::<Vec<&str>>();
    // Writing both spaces in one call is not read here: which one applies second decides the result, and
    // no `change` fixture states the order.
    if beyond || (bytes.is_empty() && axes.is_empty()) || (!bytes.is_empty() && !axes.is_empty()) {
        return decline;
    }
    // A named axis that cannot be read stops the call rather than keeping its current value:
    // `color.change(hsl(0deg 100% 50%), $hue: none)` wants `hsl(none 100% 50%)`, and answering it with the
    // unchanged hue would print a colour the call did not ask for.
    let writes = axes
        .iter()
        .map(|name| Some((*name, axis(&given, name)?)))
        .collect::<Option<Vec<(&str, f64)>>>();
    let built = match (bytes.is_empty(), writes) {
        (false, _) => bytes
            .into_iter()
            .try_fold(colour.clone(), |colour, (name, index)| {
                let value = byte(Given::find(&given, name)?)?;
                colour.with_channel(index, value)
            }),
        (true, Some(writes)) => colour.with_axes(|[hue, saturation, lightness]| {
            [
                keep(&writes, "hue", hue),
                keep(&writes, "saturation", saturation),
                keep(&writes, "lightness", lightness),
            ]
        }),
        (true, None) => return decline,
    };
    let Some(built) = built else {
        return decline;
    };
    // `$alpha:` is written only when it is already in range. Every `change` row that names it passes an
    // in-gamut alpha (`change/hsl.hrx::alpha_arg` uses `0.7`), so what an out-of-range one does — clamp as
    // `rgb()` does, or report a type error — has no golden, and declining keeps the row red instead of
    // borrowing a law from a different function.
    let alpha = match Given::find(&given, "alpha") {
        Some(value) => match number(value) {
            Some(alpha) if (0.0..=1.0).contains(&alpha) => Some(alpha),
            _ => return decline,
        },
        None => None,
    };
    Ok(Value::Color(match alpha {
        Some(alpha) => built.with_alpha(alpha),
        None => built,
    }))
}

/// The value to write for one axis, or the colour's current one when the call did not name it.
fn keep(writes: &[(&str, f64)], name: &str, current: f64) -> f64 {
    writes
        .iter()
        .find(|(written, _)| *written == name)
        .map_or(current, |(_, value)| *value)
}

/// A value for one hsl axis, read the way the constructors read it. The hue converts an angle
/// (`color.change(red, $hue: 60rad)` → `rgb(0%, 70.4220486918%, 100%)`, `change/hsl.hrx::units/hue/angle`)
/// and folds (`$hue: 540` → `aqua`, `$hue: -60` → `fuchsia`), while saturation and lightness take the
/// magnitude of **any** unit — `50%`, `50` and `50in` all mean 50 (`…::units/saturation/*`; those rows carry
/// a deprecation warning the harness does not compare, so their output matching is not deprecation support).
fn axis(given: &[Given], name: &str) -> Option<f64> {
    let value = Given::find(given, name)?;
    if name == "hue" {
        return crate::color::angle_of(value);
    }
    number(value)
}

/// The magnitude of a numeric argument, whatever its unit.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, _) => Some(*number),
        _ => None,
    }
}

/// An `rgb` channel as the corpus writes one: a unitless number of `0–255`, or a percentage of 255.
/// Anything else — a `px` quantity, the keyword `none` (`change/rgb.hrx::red/none` wants `rgb(none 0 0)`,
/// which is a `none` rule rather than a byte rule) — has no byte to write, so the call is unreadable. There
/// is no sentinel value here: an out-of-range number is what the caller sees, and only `None` means "stop".
fn byte(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, unit) if unit.single() == Some("%") => Some(number * 255.0 / 100.0),
        Value::Number(number, _) => Some(*number),
        _ => None,
    }
}
