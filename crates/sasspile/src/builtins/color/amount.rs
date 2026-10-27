//! Reading the **channel arguments** of `color.change` / `color.adjust` / `color.scale`, and the message the
//! corpus gives when an argument cannot be used. Split out of [`super::modify`] because that file is about
//! *which* channel a call names and what arithmetic it does, while this one is about *whether an argument may
//! be read at all* — the half where P7's diagnostics live.
//!
//! Every wording here is a row, and the three families do not share one wording:
//! * `change` says `c is not a number or unquoted "none".` because an unquoted `none` **means something**
//!   there — a missing channel — and that row's message is another rule this slice has not got
//!   (`core_functions/color/change/error/type.hrx::{red,quoted_none}`; `…/change/hwb.hrx::whiteness/none`
//!   is the exempt value);
//! * `adjust` and `scale` say `c is not a number.` even for it
//!   (`core_functions/color/adjust/error/type.hrx::none` is `color.adjust(red, $alpha: none)` →
//!   `Error: $alpha: none is not a number.`, and `scale/error/type.hrx` has the same for `$alpha`);
//! * a scale's amount must carry `%` (`scale/error/units/rgb.hrx::none/red` is `$red: 1` →
//!   `Error: $red: Expected 1 to have unit "%".`), and so do `adjust`'s and `change`'s whiteness and
//!   blackness (`adjust/error/units/hwb.hrx::none/whiteness`) — while every other legacy channel amount takes
//!   the magnitude of whatever unit it carries (`change/hsl.hrx::units/*`: `60deg`, `60` and `60in` all mean
//!   60; those rows carry a deprecation warning the harness does not compare, so matching their output is not
//!   deprecation support).

use super::modify::Writing;
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::Given;
use crate::value::Value;

/// The refusal this slice already makes for reasons the corpus leaves unanswered — an unreadable colour, a
/// `$space:` it cannot convert, two spaces named at once. Not fatal: the call keeps its own text.
pub(super) fn decline() -> SassError {
    SassError::new("a colour or channel this slice cannot change")
}

/// One axis (or alpha) argument. The hue converts an angle and folds
/// (`color.change(red, $hue: 60rad)` → `rgb(0%, 70.4220486918%, 100%)`, `change/hsl.hrx::units/hue/angle`;
/// `$hue: 540` → `aqua`), and every other axis is the magnitude of whatever unit it carries — except where a
/// `%` is demanded, which is [`share`] for a scale and the whiteness/blackness branch below.
///
/// A `%` on the hue is attested only as `Expected 20% to have an angle unit (deg, grad, rad, turn).` for
/// `lch` and `oklch` colours (`adjust/error/units/lch.hrx`, `…/oklch.hrx`); this slice reads neither space,
/// so those calls still decline rather than claiming the message.
pub(super) fn axis(given: &[Given], name: &str, writing: Writing) -> Result<f64, SassError> {
    let value = Given::find(given, name).ok_or_else(decline)?;
    if name == "hue" {
        // An unquoted `none` is the missing-channel case again, not a type error:
        // `color.change(hsl(0deg 100% 50%), $hue: none)` expects the output `hsl(none 100% 50%)`, so
        // refusing it loudly would turn a wrong-value row into an unexpected-error row.
        return crate::color::angle_of(value).ok_or_else(|| {
            if is_none(value) {
                decline()
            } else {
                not_a_number(name, value, writing)
            }
        });
    }
    if writing == Writing::Scale {
        return share(name, value);
    }
    let magnitude = number(name, value, writing)?;
    // Whiteness and blackness are percentages by the corpus's own statement, for `adjust` and `change` too:
    // `color.adjust(white, $whiteness: 1)` → `Error: $whiteness: Expected 1 to have unit "%".`
    // (`adjust/error/units/hwb.hrx::none/whiteness`, and the same file in `change`).
    if matches!(name, "whiteness" | "blackness") && !is_percent(value) {
        return Err(expected_percent(name, value));
    }
    Ok(magnitude)
}

/// A scale's amount: a **share** of the distance still to the bound, so it must carry `%` and stay within
/// `-100%` and `100%` (`scale/error/units/rgb.hrx::{none,wrong}/*` for the unit, and
/// `color.scale(red, $saturation: -100.001%)` →
/// `Error: $saturation: Expected -100.001% to be within -100% and 100%.` for the range
/// — `scale/error/bounds.hrx::legacy/too_low`, whose sibling `too_high` is `$whiteness: 100.001%`).
pub(super) fn share(name: &str, value: &Value) -> Result<f64, SassError> {
    let magnitude = number(name, value, Writing::Scale)?;
    if !is_percent(value) {
        return Err(expected_percent(name, value));
    }
    if !(-100.0..=100.0).contains(&magnitude) {
        return Err(SassError::typed(
            ErrorKind::Channel,
            format!(
                "${name}: Expected {} to be within -100% and 100%.",
                value.to_css()
            ),
        ));
    }
    Ok(magnitude)
}

/// An `rgb` channel argument as `change` and `adjust` write one: a unitless number of `0–255`
/// (`color.adjust(#abcdef, $red: 200)`, `adjust/rgb.hrx::red/above_max`) or a percentage of 255
/// (`adjust/units.hrx` uses `10%` for a byte channel).
pub(super) fn byte(value: &Value, name: &str, writing: Writing) -> Result<f64, SassError> {
    match value {
        Value::Number(number, unit) if unit.single() == Some("%") => Ok(number * 255.0 / 100.0),
        Value::Number(number, _) => Ok(*number),
        // Anything else — a `px` quantity, the keyword `none` — has no byte to write. `none` in `change`
        // wants the missing-channel answer, so it keeps its text; `c` is the corpus's type message.
        _ if is_none(value) && writing == Writing::Set => Err(decline()),
        _ => Err(not_a_number(name, value, writing)),
    }
}

/// The magnitude of a numeric argument, or the family's own message about the value that is not one.
fn number(name: &str, value: &Value, writing: Writing) -> Result<f64, SassError> {
    match value {
        Value::Number(number, _) => Ok(*number),
        _ if is_none(value) && writing == Writing::Set => Err(decline()),
        _ => Err(not_a_number(name, value, writing)),
    }
}

fn is_percent(value: &Value) -> bool {
    matches!(value, Value::Number(_, unit) if unit.single() == Some("%"))
}

/// An **unquoted** `none` — the value `change` reads as a missing channel (`change/hwb.hrx::whiteness/none`),
/// which is why that family's type message says `or unquoted "none"` and why the value itself is refused
/// quietly instead of with that message.
fn is_none(value: &Value) -> bool {
    matches!(value, Value::Raw(text) if text == "none")
}

/// `${parameter}: Expected ${value} to have unit "%".`, with the value rendered in CSS form so `1` and `1px`
/// read as the corpus writes them (`scale/error/units/rgb.hrx`, `…/hsl.hrx`, `…/hwb.hrx`).
fn expected_percent(name: &str, value: &Value) -> SassError {
    SassError::typed(
        ErrorKind::WrongUnits,
        format!("${name}: Expected {} to have unit \"%\".", value.to_css()),
    )
}

/// The type message, in the wording the family that made the call uses: `change` names the `none` it also
/// accepts, and keeps the quotes of a quoted `"none"` (`change/error/type.hrx::quoted_none` →
/// `Error: $alpha: "none" is not a number or unquoted "none".`).
fn not_a_number(name: &str, value: &Value, writing: Writing) -> SassError {
    let message = match writing {
        Writing::Set => format!(
            "${name}: {} is not a number or unquoted \"none\".",
            value.to_css()
        ),
        Writing::Shift | Writing::Scale => {
            format!("${name}: {} is not a number.", value.to_css())
        }
    };
    SassError::typed(ErrorKind::NotANumber, message)
}
