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

/// The positional reading these builtins deliberately do not have.
fn needs_labels(args: Vec<Value>) -> Result<Value, SassError> {
    let _ = args;
    Err(SassError::new(
        "this colour call reads its arguments by parameter",
    ))
}

/// `color.adjust(black, $red: 255)` → `red` and `color.adjust(#abcdef, $red: 200)` → `#ffcdef`
/// (`core_functions/color/adjust/rgb.hrx::{red/max,red/above_max}`): the same routing as [`COLOR_CHANGE`],
/// with each named channel **moved** by its argument instead of set to it. Bytes cannot pass the ends —
/// where `color.change` keeps an out-of-range channel and prints hsl, `adjust` clamps to `0–255`.
///
/// The axes follow *different* laws per space, each of them stated by rows:
/// * hwb adds first and normalises an overflowing pair afterwards: `$whiteness: 100%` on `#cc6666` →
///   `rgb(87.5%, 87.5%, 87.5%)` (`adjust/hwb.hrx::whiteness/max`), and `$whiteness: -100%` leaves
///   `hsl(0, 700%, 10%)` — out of gamut, printed from the written axes (`…::whiteness/min`);
/// * hsl does **not** take the legacy clamp: `color.adjust(plum, $saturation: 53%)` keeps 100.2868217054%
///   and prints hsl (`adjust/hsl.hrx::saturation/max_remaining`), while the legacy `saturate(plum, 53%)`
///   clamps at 100 and prints `#ff7eff` (`saturate.hrx::two_args/max_remaining`) — so
///   [`Color::transform_axes`] remains the legacy entry and is not used here. Only the saturation's **lower**
///   bound clamps (`…::saturation/{min,min_remaining}` want the grey a zero saturation gives), while
///   `$lightness: -100%` stays negative and prints `hsl(0, 100%, -50%)` (`…::lightness/min`);
/// * a call that names no channel is a no-op, and `$alpha:` alone still moves the alpha — clamped to `0–1`
///   (`adjust/no_channels.hrx::{positional,named}` and its `alpha/*` group, where `rgba(red, 0.5)` plus `1`
///   prints `red` again). No `change` row states either, so `change` keeps declining there.
pub(super) static COLOR_ADJUST: Builtin = Builtin {
    name: "color.adjust",
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
    apply: needs_labels,
    labelled: Some(adjusted),
};

/// Whether a call **replaces** a channel (`color.change`), **moves** it by an offset (`color.adjust`), or
/// **moves it toward its bound** by that share of the distance still left (`color.scale`).
///
/// One type rather than three flags, and one routing function rather than three copies of it: the three
/// differ only in the arithmetic on the value and in where clamping applies, and a law written down twice is
/// how `fractional()` and the gamut test each came to disagree with the corpus this week.
#[derive(Clone, Copy, PartialEq)]
enum Writing {
    Set,
    Shift,
    Scale,
}

/// One `color.change` call.
fn changed(given: Vec<Given>) -> Result<Value, SassError> {
    route(given, Writing::Set)
}

/// One `color.adjust` call.
fn adjusted(given: Vec<Given>) -> Result<Value, SassError> {
    route(given, Writing::Shift)
}

/// One `color.scale` call.
fn scaled(given: Vec<Given>) -> Result<Value, SassError> {
    route(given, Writing::Scale)
}

/// `color.scale(black, $red: 100%)` → `red` and `color.scale(lightcoral, $red: -33%)` →
/// `rgb(63.0588235294%, …)` (`core_functions/color/scale/rgb.hrx::{red/max,red/low}`): a channel moves
/// **toward whichever bound the sign points at**, by that share of the distance still between it and that
/// bound — 240 − (240 − 0) × 0.33 = 160.8, and `turquoise`'s red 64 + (255 − 64) × 0.86 = 228.26 =
/// `89.5137254902%` (`…::red/high`). So a scale can never leave the gamut, which is why this file's rows
/// have no `above_max`: `100%` lands **on** the bound (`scale/hsl.hrx::saturation/max` → `#ff7eff`).
///
/// The same formula runs on the hsl axes (`scale/hsl.hrx`: plum's own saturation 47.2868217054% scaled by
/// 67% gives `rgb(95.6%, 53.8117647059%, 95.6%)`), on whiteness and blackness — where an overflowing pair is
/// normalised afterwards exactly as in `adjust`, `color.scale(#66cc99, $whiteness: 100%)` →
/// `rgb(83.3333333333%, …)` (`scale/hwb.hrx::whiteness/max`) — and on the alpha, whose bounds are `0–1`:
/// `rgba(red, 0.5)` with `$alpha: 14%` → `0.57` and `0.3` with `-36%` → `0.192`
/// (`scale/no_space.hrx::alpha/{high,low}`).
///
/// `$hue:` **declines**: no `scale/*.hrx` row names it, and an angle has no bound to scale toward —
/// borrowing `adjust`'s wrap would be inventing that law. A call naming no channel is a no-op
/// (`scale/no_space.hrx::{positional,named}` → `red`), as `adjust` already shows.
pub(super) static COLOR_SCALE: Builtin = Builtin {
    name: "color.scale",
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
    apply: needs_labels,
    labelled: Some(scaled),
};

/// The legacy global spelling of the same law. `scale/global.hrx::legacy` is
/// `scale-color(pink, $blue: 20%)` → `rgb(100%, 75.2941176471%, 83.6862745098%)` — pink's blue 203 moved
/// 20% of the 52 left below 255. That file's `non_legacy` row names `$chroma:` with `$space: oklch`, which
/// this slice has no space for, so it keeps its text.
pub(super) static SCALE_COLOR: Builtin = Builtin {
    name: "scale-color",
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
    apply: needs_labels,
    labelled: Some(scaled),
};

/// The value a scale writes: `amount` is read as a percentage of the distance still to the bound its sign
/// points at, so the result is always between `floor` and `ceiling` (`scale_toward(240, -33.0, 0.0, 255.0)`
/// is `160.8`, `scale_toward(64, 86.0, 0.0, 255.0)` is `228.26` — both from `scale/rgb.hrx`).
fn scale_toward(current: f64, amount: f64, floor: f64, ceiling: f64) -> f64 {
    let share = amount / 100.0;
    if share >= 0.0 {
        current + (ceiling - current) * share
    } else {
        current - (current - floor) * -share
    }
}

/// Which of the colour's own spaces a call writes. Decided once, from what the call named, so the arithmetic
/// below matches on `(space, writing)` instead of stacking one guard per function — three functions and three
/// spaces is nine arms, and a chain of `if`s would have to be re-read every time a fourth writing arrives.
enum Space {
    /// Nothing but `$alpha:`, or no channel at all — the colour carries through (`adjust/no_channels.hrx`).
    Nothing,
    /// `rgb` channels, which the colour stores as bytes.
    Bytes,
    /// Whiteness and blackness, which it may store directly or imply from bytes.
    Hwb,
    /// The hsl axes.
    Axes,
}

/// One `color.change`/`color.adjust`/`color.scale` call: read the colour, apply the channels it names — as
/// bytes, as hsl axes, or as hwb axes — then let the colour's own printing law decide the output form.
fn route(given: Vec<Given>, writing: Writing) -> Result<Value, SassError> {
    let decline = Err(SassError::new(
        "a colour or channel this slice cannot change",
    ));
    let colour = match Given::find(&given, "color").and_then(Color::of) {
        Some(colour) => colour,
        None => return decline,
    };
    // `$space:` asks for a space this compiler has no conversion for, so it declines; the modern-space
    // `change/*.hrx` files (a98-rgb, display-p3, lab, lch, oklab, oklch, xyz, …) stay red for that reason.
    let beyond = Given::names(&given, "space");
    let bytes = [("red", 0), ("green", 1), ("blue", 2)]
        .into_iter()
        .filter(|(name, _)| Given::names(&given, name))
        .collect::<Vec<(&str, usize)>>();
    let hsl = ["saturation", "lightness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .collect::<Vec<&str>>();
    let hwb = ["whiteness", "blackness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .collect::<Vec<&str>>();
    let hue = Given::names(&given, "hue");
    // Which space the call is writing: bytes, the hsl axes, or the hwb ones. `$hue:` belongs to both axis
    // spaces (`change/hsl.hrx::hue/*` writes it as hsl, `change/hwb.hrx::all` alongside whiteness and
    // blackness), so it never decides on its own — but naming hsl *and* hwb axes together, or bytes together
    // with axes, would need an order no fixture states, and those decline.
    let named = bytes.len() + hsl.len() + hwb.len() + usize::from(hue);
    if beyond
        || (named == 0 && writing == Writing::Set)
        // No `scale/*.hrx` row names `$hue:`, and an angle has no bound to scale toward; `adjust` wraps it
        // because its rows say so (`adjust/hsl.hrx::hue/arg_above_max`), and that is not evidence here.
        || (writing == Writing::Scale && hue)
        || (!hsl.is_empty() && !hwb.is_empty())
        || (!bytes.is_empty() && named > bytes.len())
    {
        return decline;
    }
    // A named channel that cannot be read stops the call rather than keeping its current value:
    // `color.change(hsl(0deg 100% 50%), $hue: none)` wants `hsl(none 100% 50%)`, and answering it with the
    // unchanged hue would print a colour the call did not ask for.
    let reads = ["hue", "saturation", "lightness", "whiteness", "blackness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .map(|name| Some((name, axis(&given, name)?)))
        .collect::<Option<Vec<(&str, f64)>>>();
    let reads = match reads {
        Some(reads) => reads,
        None => return decline,
    };
    // Which space to write. The guard above has already refused a call that names two of them, so exactly one
    // of these flags is set.
    let space = match (named == 0, !bytes.is_empty(), !hwb.is_empty()) {
        (true, _, _) => Space::Nothing,
        (false, true, _) => Space::Bytes,
        (false, false, true) => Space::Hwb,
        _ => Space::Axes,
    };
    let built = match (space, writing) {
        (Space::Nothing, _) => Some(colour.clone()),
        (Space::Bytes, _) => bytes
            .into_iter()
            .try_fold(colour.clone(), |colour, (name, index)| {
                let argument = Given::find(&given, name)?;
                let amount = match writing {
                    // The channel form the corpus writes: a unitless number of `0–255`, or a percentage
                    // of 255.
                    Writing::Set | Writing::Shift => byte(argument),
                    // A scale's argument is a **share** of the distance still to the bound, not a channel:
                    // `$red: 100%` means the whole way to 255 (`color.scale(black, $red: 100%)` → `red`,
                    // `scale/rgb.hrx::red/max`). It takes the magnitude the way the axes do — reading it
                    // through `byte()` was my first answer here and turned 100% into 255, sending every
                    // `scale/rgb` row out of the gamut.
                    Writing::Scale => number(argument),
                }?;
                // `adjust` moves the channel and cannot pass the ends: `color.adjust(#abcdef, $red: 200)`
                // is `#ffcdef`, the byte clamped to 255 (`adjust/rgb.hrx::red/above_max`). `change` keeps
                // the value it was given and lets the out-of-gamut rule print hsl instead. `scale` cannot
                // pass either end by construction (`scale/rgb.hrx::red/{max,min}`).
                let value = match writing {
                    Writing::Set => amount,
                    Writing::Shift => (colour.byte(index)? + amount).clamp(0.0, 255.0),
                    Writing::Scale => scale_toward(colour.byte(index)?, amount, 0.0, 255.0),
                };
                colour.with_channel(index, value)
            }),
        (Space::Hwb, _) => match colour.hwb_of() {
            Some(current) => {
                // No clamp here for any writing: the pair is normalised by its sum afterwards, which is what
                // `color.adjust(#cc6666, $whiteness: 100%)` → `rgb(87.5%, …)` shows
                // (`adjust/hwb.hrx::whiteness/max`) and `color.scale(#66cc99, $whiteness: 100%)` →
                // `rgb(83.3333333333%, …)` repeats (`scale/hwb.hrx::whiteness/max`).
                //
                // A channel the call did **not** name keeps its current value — which reads differently per
                // writing: `change` needs the value itself (nothing is written), `adjust` needs no offset and
                // `scale` no share. Feeding the current value through the same `keep(..)` path once doubled it
                // (`#993333` with `$blackness: 40%` came out `#555555` instead of `#333333`,
                // `…::blackness/high`), because the shift then added what was already there.
                //
                // The hue sits at index 0 and is left alone: under `scale` a call naming it already declined,
                // so its share is zero and `scale_toward(hue, 0.0, …)` returns it unchanged.
                let applied = |index: usize, name: &str| match writing {
                    Writing::Set => keep(&reads, name, current[index]),
                    Writing::Shift => current[index] + step(&reads, name),
                    Writing::Scale => scale_toward(current[index], step(&reads, name), 0.0, 100.0),
                };
                colour.with_hwb(
                    applied(0, "hue"),
                    applied(1, "whiteness"),
                    applied(2, "blackness"),
                )
            }
            None => return decline,
        },
        (Space::Axes, _) => colour.with_axes(|[hue, saturation, lightness]| match writing {
            // `change` writes the triple it was given, unclamped, and remembers it for the out-of-gamut
            // print (`change/hsl.hrx`).
            Writing::Set => [
                keep(&reads, "hue", hue),
                keep(&reads, "saturation", saturation),
                keep(&reads, "lightness", lightness),
            ],
            // The legacy clamp does not belong here: `adjust/hsl.hrx::saturation/max_remaining` keeps the
            // 100.2868217054% that `color.adjust(plum, $saturation: 53%)` adds up to and prints it as hsl,
            // while `saturate.hrx::two_args/max_remaining` gives `#ff7eff` for the same move — two entries,
            // because two opposite laws are attested ([`Color::transform_axes`] is the legacy one).
            //
            // Asymmetric bounds, both from rows: a saturation pushed below zero becomes the grey a zero is
            // (`…::saturation/{min,min_remaining}` → `rgb(74.7058823529%, …)`), a lightness pushed below zero
            // stays negative and prints hsl (`…::lightness/min` → `hsl(0, 100%, -50%)`). The hue wraps, which
            // is the one bound both entries share (`…::hue/arg_above_max` reads 540 as 180).
            Writing::Shift => [
                (hue + step(&reads, "hue")).rem_euclid(360.0),
                (saturation + step(&reads, "saturation")).max(0.0),
                lightness + step(&reads, "lightness"),
            ],
            // Toward 0 or 100 by the named share, which cannot leave the gamut — so no clamp question
            // arises, and no `scale` row prints hsl (`scale/hsl.hrx::saturation/max` → `#ff7eff`).
            Writing::Scale => [
                hue,
                scale_toward(saturation, step(&reads, "saturation"), 0.0, 100.0),
                scale_toward(lightness, step(&reads, "lightness"), 0.0, 100.0),
            ],
        }),
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
            // `adjust` moves the alpha and `clamp_alpha` keeps it in `0–1`, which is what
            // `color.adjust(black, $red: 12, $green: 24, $blue: 48, $alpha: -0.3)` → `rgba(12, 24, 48, 0.7)`
            // shows (`adjust/rgb.hrx::alpha_arg`); `scale` moves it toward whichever bound its sign points at
            // (`color.scale(rgba(red, 0.5), $alpha: 14%)` → `0.57`, `scale/no_space.hrx::alpha/high`);
            // `change` accepts only an in-range value, since no `change` row states what clamping means there.
            Some(alpha) => match writing {
                Writing::Set if (0.0..=1.0).contains(&alpha) => Some(alpha),
                Writing::Shift | Writing::Scale => Some(alpha),
                Writing::Set => return decline,
            },
            _ => return decline,
        },
        None => None,
    };
    Ok(Value::Color(match (alpha, writing) {
        (Some(alpha), Writing::Set) => built.with_alpha(alpha),
        (Some(alpha), Writing::Shift) => built.with_alpha(built.alpha() + alpha),
        (Some(alpha), Writing::Scale) => {
            built.with_alpha(scale_toward(built.alpha(), alpha, 0.0, 1.0))
        }
        (None, _) => built,
    }))
}

/// The offset a call named for one axis, or zero when it did not — `adjust` adds these, `change` never does.
fn step(reads: &[(&str, f64)], name: &str) -> f64 {
    reads
        .iter()
        .find(|(read, _)| *read == name)
        .map_or(0.0, |(_, value)| *value)
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
