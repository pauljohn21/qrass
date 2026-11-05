//! The **write side** of `color.change` / `color.adjust` / `color.scale`: which space a call is writing, and the arithmetic each of the
//! four representations gets for it. Split out of [`super::modify`], which keeps the signatures — the labels
//! a family declares and the error wording of an unreadable argument — because the two halves answer
//! different questions: "what did the call name" is a signature law, "what does it write" is an arithmetic
//! one, and the file had passed this repository's line budget with both in it.

use super::amount::{self, axis, byte, keep, scale_toward, share, step};
use super::modify::Writing;
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::{Given, missing};
use crate::color::Color;
use crate::value::Value;

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
pub(super) fn route(given: Vec<Given>, writing: Writing) -> Result<Value, SassError> {
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
    // **A colour that carries its own space is written through that space's channels**, named by that space's
    // own vocabulary: `color.change(color(srgb 0.2 0.5 0.7), $red: 0.5)` → `color(srgb 0.5 0.5 0.7)`
    // (`change/srgb.hrx::red/unitless`), while `change/xyz.hrx` writes the same position through `$x` and
    // `change/lch.hrx` through `$chroma` — the names come from the space's table in `color::channels`, not
    // from a list here. Only `change` is wired: `adjust`/`scale` on a carried space keep declining exactly as
    // they do now, and `tasks.md` 9.25 names the rows that still wait on them.
    let owned = match (colour.carried_space(), writing) {
        (Some(space), Writing::Set) => space
            .channel_names()
            .into_iter()
            .filter(|name| Given::names(&given, name))
            .filter_map(|name| space.channel(name).map(|channel| (name, channel)))
            .collect::<Vec<(&'static str, usize)>>(),
        _ => Vec::new(),
    };
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
    let named = bytes.len() + hsl.len() + hwb.len() + usize::from(hue) + owned.len();
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
    let built = match colour.carried_space() {
        // The space's own names were collected above, so an empty `owned` here means the call named something
        // this space has no channel for — which keeps its text, as it did before this arm existed.
        Some(space) if !owned.is_empty() => amount::write_carried(&colour, &given, space, &owned),
        _ => match (space, writing) {
            (Space::Nothing, _) => Ok(colour.clone()),
            (Space::Bytes, _) => {
                bytes
                    .into_iter()
                    .try_fold(colour.clone(), |colour, (name, index)| {
                        let argument = Given::find(&given, name).ok_or_else(amount::decline)?;
                        // The argument is read **before** the colour's current byte, and that order is load-bearing: the
                        // corpus's message comes from this read. `scale/error/units/srgb.hrx::wrong/red` expects
                        // `$red: Expected 1px to have unit "%".` on a call whose byte read would fail too, and reading
                        // the two the other way round answers it with the call's text instead — measured, when I made
                        // exactly that swap: 42 `scale/error/units/*` rows went red.
                        //
                        // The channel form the corpus writes is the same for `change` and a shift: a unitless number of
                        // `0–255`, or a percentage of 255 (`adjust/rgb.hrx` uses both). A scale's argument is a **share**
                        // of the distance still to the bound, not a channel: `$red: 100%` means the whole way to 255
                        // (`color.scale(black, $red: 100%)` → `red`, `scale/rgb.hrx::red/max`), so it must carry `%`
                        // (`scale/error/units/rgb.hrx`). `change` alone may write **absence** — `$red: none` is the
                        // missing-channel answer (`change/rgb.hrx::red/none` → `rgb(none 0 0)`) — which is why its reader
                        // is [`amount::written_byte`], the only one that allows a `none`.
                        let amount: Option<f64> = match writing {
                            Writing::Set => amount::written_byte(argument, name)?,
                            Writing::Shift => Some(byte(argument, name, writing)?),
                            Writing::Scale => Some(share(name, argument)?),
                        };
                        // Then the current byte, for the writings that move it. `change` reads it too, and that read is
                        // what makes `Color::with_channel` decline a numeric write over a channel the colour never wrote
                        // (`adjust/error/missing.hrx::legacy`) — `change` cannot bypass it.
                        let current = colour.byte(index).ok_or_else(amount::decline)?;
                        let written: Option<f64> = match (writing, amount) {
                            // `change` keeps the value it was given and lets the out-of-gamut rule print hsl instead;
                            // nothing is clamped here (`change/rgb.hrx::red/out_of_range` keeps 500).
                            (Writing::Set, written) => written,
                            // `adjust` moves the channel and cannot pass the ends: `color.adjust(#abcdef, $red: 200)` is
                            // `#ffcdef`, the byte clamped to 255 (`adjust/rgb.hrx::red/above_max`).
                            (Writing::Shift, Some(delta)) => {
                                Some((current + delta).clamp(0.0, 255.0))
                            }
                            // A scale cannot pass either end by construction (`scale/rgb.hrx::red/{max,min}`), so it
                            // raises no clamp question.
                            (Writing::Scale, Some(part)) => {
                                Some(scale_toward(current, part, 0.0, 255.0))
                            }
                            // No reader other than `written_byte` answers `None`, and that one runs only under `Set`;
                            // declining here instead of substituting a `0` keeps an unreachable from becoming an
                            // invented value.
                            (_, None) => return Err(amount::decline()),
                        };
                        colour
                            .with_channel(index, written)
                            .ok_or_else(amount::decline)
                    })
            }
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
        },
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
