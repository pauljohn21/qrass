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

use crate::SassError;
use crate::builtins::{Builtin, Given};
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
        // The names the Color-4 spaces use for their own channels: `$x/$y/$z` (`change/xyz.hrx`), `$a/$b`
        // (`change/lab.hrx`, `change/oklab.hrx`) and `$chroma` (`change/lch.hrx`, `change/oklch.hrx`). A label
        // that is not declared here never reaches the body at all — the call declines as an unknown label —
        // which is why all 15 of `change/xyz.hrx` were red until `color.change` learned to write a carried
        // space, even though the write itself was already correct for `$red`.
        "x",
        "y",
        "z",
        "a",
        "b",
        "chroma",
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

use super::write::route;
