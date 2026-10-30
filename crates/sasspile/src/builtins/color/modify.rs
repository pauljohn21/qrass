//! `color.change`, `color.adjust` and `color.scale` — **rewriting the channels a call names**, leaving the
//! others as the colour had them.
//!
//! These are the bodies that need the registry's labels: `color.change(black, $red: 500)` and
//! `color.change(black, $green: -50)` each pass one number, and the corpus prints them as
//! `hsl(0, 5000%, 98.0392156863%)` and `hsl(120, 100%, -9.8039215686%)` respectively
//! (`change/rgb.hrx::{red,green}/out_of_range`). Without the parameter name the two are indistinguishable
//! by the time a body sees them, which is why [`Given`] exists.
//!
//! One [`route`] serves all three, split by [`Writing`] — set, move by an offset, or move toward a bound —
//! and by [`Space`] (bytes, hsl axes, hwb axes). Each of those crossings is where a corpus law lives, so the
//! code says which row states it, and an argument the family cannot use answers with the oracle's own
//! message rather than a silence (`error/{type,units,args,bounds,polar}.hrx` under
//! `core_functions/color/{change,adjust,scale}/`).

use super::amount::{self, axis, byte, share};
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::{Builtin, Given, missing};
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
pub(super) enum Writing {
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
pub(super) enum Space {
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
    let decline = Err(amount::decline());
    let colour = match Given::find(&given, "color") {
        // `color.change()` — the corpus names the parameter that is missing (`change/error/args.hrx::too_few`,
        // and the same row in `adjust`/`scale`).
        None => return Err(missing("color".to_string())),
        // `$color: 1 is not a color.` is said for a **number** (`change/error/type.hrx::color`). A colour in
        // a space this compiler has not got — `color(srgb …)`, `lab(…)`, `oklch(…)` — keeps declining: the
        // corpus gives no message for those calls, and reading them as sRGB is the wrong value
        // [`Color::of`] already refuses.
        Some(value @ Value::Number(_, _)) => {
            return Err(SassError::typed(
                ErrorKind::NotAColor,
                format!("$color: {} is not a color.", value.to_css()),
            ));
        }
        Some(value) => Color::of(value).ok_or_else(amount::decline)?,
    };
    // These three take the colour positionally and every channel by name: `color.change(red, 1)` is
    // `Error: Only one positional argument is allowed. All other arguments must be passed by name.`
    // (`change/error/args.hrx::too_many`, and the same row in `adjust`/`scale`).
    if Given::positional(&given) > 1 {
        return Err(SassError::typed(
            ErrorKind::TooManyArguments,
            "Only one positional argument is allowed. All other arguments must be passed by name.",
        ));
    }
    // `$space:` asks for a space this compiler has no conversion for, so it declines; the modern-space
    // `change`/`adjust`/`scale` files stay red for that reason — unless the argument is not a string at all,
    // which the corpus says outright: `color.scale(red, $space: 1)` → `Error: $space: 1 is not a string.`
    // (`scale/error/type.hrx::space`, and the same row in `adjust` and `change`).
    let beyond = match Given::find(&given, "space") {
        Some(Value::Raw(_)) => true,
        Some(value) => {
            return Err(SassError::typed(
                ErrorKind::NotAString,
                format!("$space: {} is not a string.", value.to_css()),
            ));
        }
        None => false,
    };
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
    // A hue has no bound to move toward, so a scale that names one is refused by name:
    // `color.scale(white, $hue: 10%)` → `Error: $hue: Channel isn't scalable.` (`scale/error/args.hrx::hue`,
    // `…/polar.hrx`), and `scale/*.hrx` never uses the parameter otherwise.
    if writing == Writing::Scale && hue {
        return Err(SassError::typed(
            ErrorKind::Channel,
            "$hue: Channel isn't scalable.",
        ));
    }
    // The channel names the call wrote, in signature order — which is the order the refusal below reports in.
    let channels = bytes
        .iter()
        .map(|(name, _)| *name)
        .chain(hue.then_some("hue"))
        .chain(hsl.iter().copied())
        .chain(hwb.iter().copied())
        .collect::<Vec<&str>>();
    // Two spaces in one call is refused **by name**, not kept as text: the first group says which space the
    // call is writing and the first foreign name is reported against it —
    // `color.change(red, $red: 1, $hue: 1)` → `Error: $hue: Color space rgb doesn't have a channel with this
    // name.`, `color.scale(red, $saturation: 1%, $blackness: 1%)` → `Error: $blackness: Color space hsl …`
    // (`{change,adjust,scale}/error/mixed_formats.hrx`).
    if let Some(foreign) = amount::mixed_channel(&channels) {
        return Err(foreign);
    }
    // A colour in its own colour space is refused by the **write itself**, further down: `Color::with_axes`
    // reads its axes through [`Color::axes_of`], which answers `None` for `Representation::InSpace`, and
    // `Color::with_hwb` asks [`Color::writes_bytes`], which answers `false` for it; either way the call declines
    // to its text. It is deliberately not a guard at this entry. An experiment with one placed before the
    // channel reads was measured and reverted, and the reason is stated by rows rather than by a count that no
    // recorded run can reproduce any more: `scale/error/polar.hrx::lch` writes
    // `color.scale(lch(50% 30% 180deg), $hue: 10%)` and its golden **is** `Error: $hue: Channel isn't
    // scalable.`, and `scale/error/units/oklch.hrx::wrong/lightness` writes
    // `color.scale(lch(30% 70% 200deg), $lightness: 1px)` expecting `Error: $lightness: Expected 1px to have
    // unit "%".` — a blanket refusal answers neither, because the message comes from reading the argument. The
    // cost of letting the reading run is recorded in `tasks.md` 9.9: two rows
    // (`change/unusual_numbers::{infinity,negative_infinity}/polar`) report
    // `$hue: calc(infinity) is not a number or unquoted "none"` where the corpus expects `lch(50% 150 0deg)`,
    // because `calc()` to a number and the Color-4 `$space` write are both unbuilt.
    // `$alpha:` is read **before** the "nothing else named" refusal below, because the corpus reports a bad
    // alpha even for a call that names nothing else: `color.change(red, $alpha: c)` →
    // `Error: $alpha: c is not a number or unquoted "none".` (`change/error/type.hrx::alpha`, and
    // `…::quoted_none` for the quoted `"none"`), while `color.adjust(red, $alpha: none)` →
    // `Error: $alpha: none is not a number.` says the plain wording for the two families that do not take it
    // (`adjust/error/type.hrx::none`, `scale/error/type.hrx::none`).
    let alpha = if Given::names(&given, "alpha") {
        Some(axis(&given, "alpha", writing)?)
    } else {
        None
    };
    // `change` writes an alpha only when it is already in range: every `change` row that names one passes an
    // in-gamut value (`change/hsl.hrx::alpha_arg` uses `0.7`), so what an out-of-range one does — clamp as
    // `rgb()` does, or report a type error — has no golden, and declining keeps the row red instead of
    // borrowing a law from a different function.
    if writing == Writing::Set && alpha.is_some_and(|alpha| !(0.0..=1.0).contains(&alpha)) {
        return decline;
    }
    // Which space the call is writing: bytes, the hsl axes, or the hwb ones. `$hue:` belongs to both axis
    // spaces (`change/hsl.hrx::hue/*` writes it as hsl, `change/hwb.hrx::all` alongside whiteness and
    // blackness), so it never decides on its own — and a call that names two spaces at once has already been
    // refused by name above, so at most one group is set here.
    let named = bytes.len() + hsl.len() + hwb.len() + usize::from(hue);
    if beyond || (named == 0 && writing == Writing::Set) {
        return decline;
    }
    // A named channel that cannot be read stops the call — with the corpus's message when the argument is
    // there but unusable (`$red: c is not a number.`), and with the refusal when the family has a rule this
    // slice has not got yet: `color.change(hsl(0deg 100% 50%), $hue: none)` wants
    // `hsl(none 100% 50%)`, a missing-channel law, not a type error.
    let reads = ["hue", "saturation", "lightness", "whiteness", "blackness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .map(|name| axis(&given, name, writing).map(|value| (name, value)))
        .collect::<Result<Vec<(&str, f64)>, SassError>>()?;
    // Which space to write. The guard above has already refused a call that names two of them, so exactly one
    // of these flags is set.
    let space = match (named == 0, !bytes.is_empty(), !hwb.is_empty()) {
        (true, _, _) => Space::Nothing,
        (false, true, _) => Space::Bytes,
        (false, false, true) => Space::Hwb,
        _ => Space::Axes,
    };
    let built = match (space, writing) {
        (Space::Nothing, _) => Ok(colour.clone()),
        (Space::Bytes, _) => bytes
            .into_iter()
            .try_fold(colour.clone(), |colour, (name, index)| {
                let argument = Given::find(&given, name).ok_or_else(amount::decline)?;
                let amount = match writing {
                    // The channel form the corpus writes: a unitless number of `0–255`, or a percentage
                    // of 255 (`adjust/rgb.hrx` uses both).
                    Writing::Set | Writing::Shift => byte(argument, name, writing)?,
                    // A scale's argument is a **share** of the distance still to the bound, not a channel:
                    // `$red: 100%` means the whole way to 255 (`color.scale(black, $red: 100%)` → `red`,
                    // `scale/rgb.hrx::red/max`), so it must carry `%` (`scale/error/units/rgb.hrx`).
                    Writing::Scale => share(name, argument)?,
                };
                // `adjust` moves the channel and cannot pass the ends: `color.adjust(#abcdef, $red: 200)`
                // is `#ffcdef`, the byte clamped to 255 (`adjust/rgb.hrx::red/above_max`). `change` keeps
                // the value it was given and lets the out-of-gamut rule print hsl instead. `scale` cannot
                // pass either end by construction (`scale/rgb.hrx::red/{max,min}`).
                let current = colour.byte(index).ok_or_else(amount::decline)?;
                let value = match writing {
                    Writing::Set => amount,
                    Writing::Shift => (current + amount).clamp(0.0, 255.0),
                    Writing::Scale => scale_toward(current, amount, 0.0, 255.0),
                };
                colour
                    .with_channel(index, value)
                    .ok_or_else(amount::decline)
            }),
        (Space::Hwb, _) => {
            // Whiteness and blackness the colour does not store must be implied from its bytes; an `hsl`
            // colour has no such reading and no row asks for one, so that call keeps its text.
            let current = colour.hwb_of().ok_or_else(amount::decline)?;
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
            // The hue sits at index 0 and is **not** folded here: a `scale` naming it is refused above, so its
            // share is zero and `scale_toward(hue, 0.0, …)` returns the current value; a `set` writes the value
            // `angle_of` already folded at the read; and a `shift` that passes 360 is wrapped by the conversion
            // it is handed to ([`crate::axes::axes_to_rgb`]) — one fold serving every caller rather than one
            // per writing.
            let applied = |index: usize, name: &str| match writing {
                Writing::Set => keep(&reads, name, current[index]),
                Writing::Shift => current[index] + step(&reads, name),
                Writing::Scale => scale_toward(current[index], step(&reads, name), 0.0, 100.0),
            };
            // What that leaves asymmetric is recorded here, not settled: the axes branch below folds a shifted
            // hue *before storing it*, so a past-a-turn sum reaches [`Color::with_hwb`] raw and
            // [`Color::with_axes`] folded, and only `authored` — the out-of-gamut hsl print — could show the
            // difference. No row can: none adds a nonzero hue to a colour whose own hue is nonzero
            // (`adjust/hsl.hrx`/`adjust/hwb.hrx` start at `red` or `black`, and `blue` is added `0`), and
            // `color::text`'s `out_of_gamut` matches the authored triple on saturation and lightness alone.
            colour
                .with_hwb(
                    applied(0, "hue"),
                    applied(1, "whiteness"),
                    applied(2, "blackness"),
                )
                .ok_or_else(amount::decline)
        }
        (Space::Axes, _) => colour
            .with_axes(|[hue, saturation, lightness]| match writing {
                // `change` writes the triple it was given, unclamped, and remembers it for the out-of-gamut
                // print (`change/hsl.hrx`).
                Writing::Set => [
                    keep(&reads, "hue", hue),
                    keep(&reads, "saturation", saturation),
                    keep(&reads, "lightness", lightness),
                ],
                // The legacy clamp does not belong here: `adjust/hsl.hrx::saturation/max_remaining` keeps the
                // 100.2868217054% that `color.adjust(plum, $saturation: 53%)` adds up to and prints it as hsl,
                // while `saturate.hrx::two_args/max_remaining` gives `#ff7eff` for the same move — two
                // entries, because two opposite laws are attested ([`Color::transform_axes`] is the legacy
                // one).
                //
                // Asymmetric bounds, both from rows: a saturation pushed below zero becomes the grey a zero
                // is (`…::saturation/{min,min_remaining}` → `rgb(74.7058823529%, …)`), a lightness pushed
                // below zero stays negative and prints hsl (`…::lightness/min` → `hsl(0, 100%, -50%)`). The
                // hue wraps, which is the one bound both entries share (`…::hue/arg_above_max` reads 540 as
                // 180).
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
            })
            .ok_or_else(amount::decline),
    }?;
    // `adjust` moves the alpha and `clamp_alpha` keeps it in `0–1`, which is what
    // `color.adjust(black, $red: 12, $green: 24, $blue: 48, $alpha: -0.3)` → `rgba(12, 24, 48, 0.7)` shows
    // (`adjust/rgb.hrx::alpha_arg`); `scale` moves it toward whichever bound its sign points at
    // (`color.scale(rgba(red, 0.5), $alpha: 14%)` → `0.57`, `scale/no_space.hrx::alpha/high`); `change`
    // writes the value as given, having refused an out-of-range one above where it was read.
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
