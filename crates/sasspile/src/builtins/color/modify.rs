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

/// One `color.change` call: read the colour, replace each named `rgb` channel, and let the colour's own
/// printing law decide the output form.
fn changed(given: Vec<Given>) -> Result<Value, SassError> {
    let decline = Err(SassError::new(
        "a colour or channel this slice cannot change",
    ));
    let colour = match Given::find(&given, "color").and_then(Color::of) {
        Some(colour) => colour,
        None => return decline,
    };
    // Axes of another space, and any `$space:` at all: `change/hsl.hrx`, `change/hwb.hrx` and the
    // modern-space files are left red rather than answered from bytes. `$alpha:` is on the same list until
    // its rows are read — `with_alpha` already clamps, and whether `change` clamps is a fixture question.
    let beyond = [
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
        "space",
        "alpha",
    ]
    .into_iter()
    .any(|name| Given::names(&given, name));
    let named = [("red", 0), ("green", 1), ("blue", 2)]
        .into_iter()
        .filter(|(name, _)| Given::names(&given, name))
        .collect::<Vec<(&str, usize)>>();
    if beyond || named.is_empty() {
        return decline;
    }
    named
        .into_iter()
        .try_fold(colour.clone(), |colour, (name, index)| {
            let value = byte(Given::find(&given, name)?)?;
            colour.with_channel(index, value)
        })
        .map_or(decline, |built| Ok(Value::Color(built)))
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
