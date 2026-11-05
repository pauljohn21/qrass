//! Each colour space's **own channel names** and the **percentage base of each channel** — the two tables that
//! say what a call's `$red:` / `$x:` / `$chroma:` names in *this* space, and what `40%` of it means.
//!
//! Split out of [`super::space`] for the same reason [`super::primaries`] was: these are tables with a
//! provenance per row, and every row here is a fixture, not a recollection. Nothing in this file converts
//! anything; [`super::space`] asks it two questions.
//!
//! The naming half is what `color.change` writes by: a colour that carries its own space is written **in that
//! space's names**, which is how the corpus spells the same three numbers — `change/srgb.hrx` uses
//! `$red/$green/$blue`, `change/xyz.hrx` `$x/$y/$z`, `change/lab.hrx` `$lightness/$a/$b`, and
//! `change/lch.hrx` `$lightness/$chroma/$hue`.
//!
//! The percentage half is one law read in two directions, and it is **per channel**, not per space: lab's
//! lightness and its `a` do not share a base. The rows that say so are quoted on each arm below.

use super::Space;

/// The three channel names this space answers, in index order.
///
/// Exhaustive over [`Space`] on purpose: a space added later has to say its names here rather than inherit a
/// default that would answer `color.change` for a call nobody had asked it about.
pub(super) fn names(space: Space) -> [&'static str; 3] {
    match space {
        // The seven RGB-shaped `color()` spaces, in both their encoded and linear forms:
        // `change/{srgb,srgb_linear,display_p3,display_p3_linear,a98_rgb,prophoto_rgb,rec2020}.hrx`.
        Space::Srgb
        | Space::SrgbLinear
        | Space::DisplayP3
        | Space::DisplayP3Linear
        | Space::A98Rgb
        | Space::ProphotoRgb
        | Space::Rec2020 => ["red", "green", "blue"],
        // `change/{xyz,xyz_d50}.hrx`.
        Space::Xyz | Space::XyzD50 => ["x", "y", "z"],
        // `change/{lab,oklab}.hrx`.
        Space::Lab | Space::Oklab => ["lightness", "a", "b"],
        // `change/{lch,oklch}.hrx`.
        Space::Lch | Space::Oklch => ["lightness", "chroma", "hue"],
    }
}

/// The index of `name` in this space, or `None` when the space has no channel by that name — which is the
/// question behind `$whiteness:` on a `color(srgb …)`; `change/error/incompatible_channel.hrx::legacy_channel`
/// states that case as an error this slice does not raise yet, so the call keeps its text.
///
/// The compare is case-insensitive as a **stated choice with no exercising path today**: no fixture writes a
/// channel label in another case (`grep -rln '\$RED\|\$Lightness\|\$CHROMA' sass-spec/spec/core_functions/color/`
/// is empty), and the only caller passes the canonical lowercase names out of [`names`], so a label reaching
/// here is already normalized by whatever the spine does with `$X:` — which is *not* asserted here.
pub(super) fn index(space: Space, name: &str) -> Option<usize> {
    names(space)
        .iter()
        .position(|own| own.eq_ignore_ascii_case(name))
}

/// Whether one channel of this space is its **hue** — the channel an argument reaches it as an angle, folded
/// into one turn (`change/lch.hrx::hue/{negative,above_max}` read `-20deg` as `340deg` and `400deg` as
/// `40deg`), rather than as a plain number. Derived from [`names`] so the word “hue” is not spelled out in a
/// second place.
pub(super) fn is_hue(space: Space, channel: usize) -> bool {
    names(space).get(channel) == Some(&"hue")
}

/// How one channel **prints**: a derived space's lightness carries `%`, its hue carries `deg`, and every other
/// channel — the rectangular and XYZ families in full, and `lab`/`oklab`'s axes and `lch`/`oklch`'s chroma —
/// is a plain number. Asked of [`names`] rather than matched per space, so the printer cannot grow a suffix
/// the table does not say (`to_space/srgb/lab.hrx::middle` prints `lab(44.1210142293% …)` while
/// `…/lch.hrx::middle` prints `lch(44.1210142293% 60.0926108873 280.5032243387deg)`).
pub(super) fn suffix(space: Space, channel: usize) -> &'static str {
    if space.is_functional() {
        return "";
    }
    match names(space).get(channel).copied() {
        Some("lightness") => "%",
        Some("hue") => "deg",
        // The axes and chroma of the derived families, and any channel index that is not one of the three.
        _ => "",
    }
}

/// The divisor a percentage argument on one channel reads against. `None` means the archive states no
/// percentage for that channel — the two hues (`change/{lch,oklch}.hrx` have no `hue/percent` case; their
/// `hue/unitless`, `hue/negative` and `hue/above_max` cases all spell the argument as an angle, and the reader
/// in `builtins/color/amount.rs` answers a `%` there with the corpus's angle error —
/// `change/error/units/lch.hrx::hue` — before it ever asks this question). An index past the three channels is
/// not a channel either.
///
/// **Every space answers this for itself** — no `_ =>` on the space, for the reason [`names`] states: a space
/// added later must say what its channels' bases are rather than inherit `100` (or `None`) quietly. The
/// lightness column is [`full`] rather than a repeated number, so the print direction and the read direction
/// cannot drift apart.
pub(super) fn percent_divisor(space: Space, channel: usize) -> Option<f64> {
    let lightness = full(space);
    let bases: [Option<f64>; 3] = match space {
        // `lab`'s axes span `±125`, so `-40%` of `a` or `b` is `-50` (`change/lab.hrx::{a,b}/percent`).
        Space::Lab => [Some(lightness), Some(0.8), Some(0.8)],
        // `lch`'s chroma spans to 150, so `40%` of it is `60` (`change/lch.hrx::chroma/percent`).
        Space::Lch => [Some(lightness), Some(2.0 / 3.0), None],
        // The oklab family's axes span `±0.4`: `-40%` of `a` or `b` is `-0.16`
        // (`change/oklab.hrx::{a,b}/percent`) and `40%` of `oklch`'s chroma is `0.16`
        // (`change/oklch.hrx::chroma/percent`).
        Space::Oklab => [Some(lightness), Some(250.0), Some(250.0)],
        Space::Oklch => [Some(lightness), Some(250.0), None],
        // The rectangular and XYZ families are fractions of one across all three channels: `40%` → `0.4` in
        // `color(srgb …)` (`change/srgb.hrx::green/percent`), in `color(display-p3-linear …)`
        // (`change/display_p3_linear.hrx::green/percent`) and in `color(xyz …)` (`change/xyz.hrx::y/percent`).
        Space::Srgb
        | Space::SrgbLinear
        | Space::DisplayP3
        | Space::DisplayP3Linear
        | Space::A98Rgb
        | Space::ProphotoRgb
        | Space::Rec2020
        | Space::Xyz
        | Space::XyzD50 => [Some(lightness); 3],
    };
    bases.get(channel).copied().flatten()
}

/// The number a **full-scale percentage** names in this space's lightness: `lab`/`lch` state lightness on a
/// `0–100` scale, so their `50%` *is* `50` (`to_space/srgb/lab.hrx::middle` reads `lab(44.1210142293% …)`
/// and prints the same number it holds), while every other space is a fraction of one, so `100%` is `1`
/// (`to_space/oklab/srgb.hrx::white` reads `oklab(100% 0 0)` and wants `color(srgb 1 1 1)`;
/// `to_space/srgb/oklab.hrx::white` wants `oklab(100% 0 0)` back for white).
///
/// This is **one** law with both directions read from it: the argument parser divides by it (in
/// `builtins/color/constructors.rs`) and [`super::text::print`] multiplies by it before writing the `%`. It
/// was written three times before — the print suffix, this divisor, and the matrix's `0–1` domain — and the
/// three disagreed for `oklab`/`oklch`, turning `oklab(100% 0 0)` into `L = 100` and a conversion into numbers
/// of order `1e5`.
///
/// It is the `channel = 0` column of [`percent_divisor`]; the other channels are named there because the
/// archive gives them different bases (`lab`'s `a` is not on the `0–100` scale its lightness is on).
pub(crate) fn full(space: Space) -> f64 {
    // Named in full rather than `| _ => 100.0`: eleven of the thirteen spaces would otherwise be answered by a
    // default, which is the shape this repository has already rejected twice (`unwrap_or(&BRADFORD)`,
    // `Space::white`'s `_ =>` arm) for exactly the reason given — a space added later would be answered here
    // without ever being asked.
    match space {
        Space::Lab | Space::Lch => 1.0,
        Space::Srgb
        | Space::SrgbLinear
        | Space::DisplayP3
        | Space::DisplayP3Linear
        | Space::A98Rgb
        | Space::ProphotoRgb
        | Space::Rec2020
        | Space::Xyz
        | Space::XyzD50
        | Space::Oklab
        | Space::Oklch => 100.0,
    }
}
