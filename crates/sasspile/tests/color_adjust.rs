//! `color.adjust`, pinned row by row on `core_functions/color/adjust/{rgb,hsl,hwb,units,no_channels}.hrx`
//! and the namespaced twin `core_functions/modules/color/adjust.hrx`. Every input and expectation below is
//! copied from the fixture's own `input.scss` / `output.css`, case names included.
//!
//! The point of this file is that **`adjust` moves a channel instead of setting it**, and that the bounds it
//! moves against are not one rule: bytes stop at `0–255`, the hsl axes do not take the legacy clamp at all
//! (which is what separates this function from `saturate`), the hwb pair is normalised after the addition,
//! and a call that names nothing is a no-op. Each of those has its own rows, so they are written separately
//! rather than folded into one clamp a caller has to remember.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// A byte channel moves by its argument and cannot pass the ends: `red/above_max` is
/// `color.adjust(#abcdef, $red: 200)` → `#ffcdef` (171 + 200 = 371, clamped to `ff`, the other two
/// untouched), and `red/below_min` is the same call with `-200` → `#00cdef`. `all` and `named` are one call
/// on three channels — as positional arguments and with the colour labelled — and both give `#0c1830`, the
/// offsets `12/24/48` from black; `alpha_arg` adds `$alpha: -0.3` to that and prints `rgba(12, 24, 48, 0.7)`.
#[tokio::test]
async fn bytes_move_and_stop_at_the_ends() {
    for (input, want) in [
        ("a {b: color.adjust(black, $red: 255)}", "red"),
        ("a {b: color.adjust(red, $red: -100)}", "#9b0000"),
        ("a {b: color.adjust(#abcdef, $red: 200)}", "#ffcdef"),
        ("a {b: color.adjust(#abcdef, $red: -200)}", "#00cdef"),
        ("a {b: color.adjust(#abcdef, $green: 200)}", "#abffef"),
        ("a {b: color.adjust(#fedcba, $blue: -200)}", "#fedc00"),
        (
            "a {b: color.adjust(black, $red: 12, $green: 24, $blue: 48)}",
            "#0c1830",
        ),
        (
            "a {b: color.adjust($color: black, $red: 12, $green: 24, $blue: 48)}",
            "#0c1830",
        ),
        (
            "a {b: color.adjust(black, $red: 12, $green: 24, $blue: 48, $alpha: -0.3)}",
            "rgba(12, 24, 48, 0.7)",
        ),
        // The `@use "sass:color"` spelling reaches the same function (`modules/color/adjust.hrx::`).
        ("a {b: color.adjust(lime, $green: -255)}", "black"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **The hsl axes are not clamped**, which is what separates `color.adjust` from the legacy `saturate`:
/// `color.adjust(plum, $saturation: 53%)` keeps 100.2868217054% (plum's own saturation plus the argument)
/// and prints the hsl triple, while `saturate(plum, 53%)` prints `#ff7eff` — both are rows, so both are
/// entries ([`Color::transform_axes`](sasspile::color::Color) stays the legacy one).
#[tokio::test]
async fn the_hsl_axes_keep_what_they_add_up_to() {
    for (input, want) in [
        (
            "a {b: color.adjust(plum, $saturation: 53%)}",
            "hsl(300, 100.2868217054%, 74.7058823529%)",
        ),
        (
            "a {b: color.adjust(plum, $saturation: 100%)}",
            "hsl(300, 147.2868217054%, 74.7058823529%)",
        ),
        (
            "a {b: color.adjust(plum, $saturation: 200%)}",
            "hsl(300, 247.2868217054%, 74.7058823529%)",
        ),
        (
            "a {b: color.adjust(red, $lightness: -100%)}",
            "hsl(0, 100%, -50%)",
        ),
        (
            "a {b: color.adjust(red, $lightness: 100%)}",
            "hsl(0, 100%, 150%)",
        ),
        (
            "a {b: color.adjust(red, $lightness: 200%)}",
            "hsl(0, 100%, 250%)",
        ),
        (
            "a {b: color.adjust(plum, $lightness: -100%)}",
            "hsl(300, 47.2868217054%, -25.2941176471%)",
        ),
        (
            "a {b: color.adjust(plum, $lightness: -200%)}",
            "hsl(300, 47.2868217054%, -125.2941176471%)",
        ),
        // The legacy entry, pinned beside the row above it so the split cannot be merged away silently
        // (`saturate.hrx::two_args/max_remaining`).
        ("a {b: saturate(plum, 53%)}", "#ff7eff"),
        // One call naming all three axes at once (`adjust/hsl.hrx::all`): black moved by 12°, 24% and 48%
        // stays inside the gamut, so it prints the bytes that triple implies.
        (
            "a {b: color.adjust(black, $hue: 12, $saturation: 24%, $lightness: 48%)}",
            "rgb(59.52%, 41.088%, 36.48%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// One bound `adjust` *does* keep: a saturation pushed below zero becomes the grey a zero is.
/// `color.adjust(plum, $saturation: -100%)` and `color.adjust(plum, $saturation: -48%)` (47.2868217054% − 48
/// is negative) both print `rgb(74.7058823529%, …)` — the lightness of plum, unchanged, as a grey
/// (`adjust/hsl.hrx::saturation/{min,min_remaining}`). The lightness has no such clamp, which
/// [`the_hsl_axes_keep_what_they_add_up_to`] shows with `-50%`.
#[tokio::test]
async fn only_the_saturation_lower_bound_clamps() {
    let grey = "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)";
    for (input, want) in [
        ("a {b: color.adjust(plum, $saturation: -100%)}", grey),
        ("a {b: color.adjust(plum, $saturation: -48%)}", grey),
        ("a {b: color.adjust(plum, $saturation: -200%)}", grey),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The hue wraps, the one bound both entries share: `color.adjust(red, $hue: 540)` is `aqua` and
/// `color.adjust(red, $hue: -180)` is `aqua` too (`adjust/hsl.hrx::hue/{arg_above_max,negative}`), while a
/// move that stays inside prints the bytes it implies (`hue/max` is `red` moved 359°).
#[tokio::test]
async fn the_hue_wraps_and_otherwise_prints_bytes() {
    for (input, want) in [
        ("a {b: color.adjust(red, $hue: 540)}", "aqua"),
        ("a {b: color.adjust(red, $hue: -180)}", "aqua"),
        (
            "a {b: color.adjust(red, $hue: 359)}",
            "rgb(100%, 0%, 1.6666666667%)",
        ),
        ("a {b: color.adjust(blue, $hue: 0)}", "blue"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// hwb **adds first and normalises afterwards** — the opposite order to a clamp.
/// `color.adjust(#cc6666, $whiteness: 100%)` takes whiteness 60 → 160 against blackness 40, and the pair is
/// then scaled by its own sum: `160/200` and `40/200`, which is the `rgb(87.5%, …)` grey
/// (`adjust/hwb.hrx::whiteness/max`). A channel the call did not name keeps its value rather than being
/// moved twice: `color.adjust(#993333, $blackness: 40%)` is `#333333` (`…::blackness/high`), and a pair that
/// ends up outside the gamut prints the hsl triple, as `$whiteness: -100%` → `hsl(0, 700%, 10%)` shows
/// (`…::whiteness/min`).
#[tokio::test]
async fn hwb_moves_then_normalises_the_pair() {
    for (input, want) in [
        (
            "a {b: color.adjust(#cc6666, $whiteness: 100%)}",
            "rgb(87.5%, 87.5%, 87.5%)",
        ),
        (
            "a {b: color.adjust(#cc6666, $whiteness: 60%)}",
            "rgb(83.3333333333%, 83.3333333333%, 83.3333333333%)",
        ),
        ("a {b: color.adjust(#cc6666, $whiteness: 0%)}", "#cc6666"),
        (
            "a {b: color.adjust(#cc6666, $whiteness: -100%)}",
            "hsl(0, 700%, 10%)",
        ),
        ("a {b: color.adjust(#993333, $blackness: 40%)}", "#333333"),
        // `$hue:` alongside the pair is the same hwb write (`adjust/hwb.hrx::all`), and the result lands on
        // whole bytes, so it prints hex.
        (
            "a {b: color.adjust(red, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
            "#339966",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// Naming no channel is a no-op, and `$alpha:` alone still moves the alpha — clamped to `0–1`, where the
/// saturation and lightness above are not: `color.adjust(red)` and `color.adjust($color: red)` are both
/// `red`, `rgba(red, 0.5)` plus `0.5` or `1` or `2` is opaque again (`red`, printed by its keyword because
/// the colour was made from one), and minus `1` or `2` is `rgba(255, 0, 0, 0)`
/// (`adjust/no_channels.hrx::{positional,named}` and its `alpha/*` group). The magnitude is read whatever
/// unit it carries — `-0.3`, `-0.3%` and `-0.3px` all take `red` to `rgba(255, 0, 0, 0.7)`
/// (`adjust/units.hrx::alpha/*`; the two deprecated spellings carry warnings the harness does not compare).
#[tokio::test]
async fn naming_no_channel_is_a_no_op_and_the_alpha_still_moves() {
    for (input, want) in [
        ("a {b: color.adjust(red)}", "red"),
        ("a {b: color.adjust($color: red)}", "red"),
        ("a {b: color.adjust(rgba(red, 0.5), $alpha: 0.5)}", "red"),
        ("a {b: color.adjust(rgba(red, 0.5), $alpha: 1)}", "red"),
        ("a {b: color.adjust(rgba(red, 0.5), $alpha: 2)}", "red"),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: 0)}",
            "rgba(255, 0, 0, 0.5)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: 0.14)}",
            "rgba(255, 0, 0, 0.64)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: -0.14)}",
            "rgba(255, 0, 0, 0.36)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: -0.5)}",
            "rgba(255, 0, 0, 0)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: -1)}",
            "rgba(255, 0, 0, 0)",
        ),
        (
            "a {b: color.adjust(red, $alpha: -0.3)}",
            "rgba(255, 0, 0, 0.7)",
        ),
        (
            "a {b: color.adjust(red, $alpha: -0.3%)}",
            "rgba(255, 0, 0, 0.7)",
        ),
        (
            "a {b: color.adjust(red, $alpha: -0.3px)}",
            "rgba(255, 0, 0, 0.7)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

// Rows this file does **not** claim. The `adjust/hwb.hrx::alpha_input` and `…::alpha_arg` pair wants
// `rgba(20%, 60%, 40%, …)` where this compiler prints `rgba(51, 153, 102, …)` — the same two rows
// `tests/color_change.rs` leaves red for `color.change`, for the same unexplained reason (a third row,
// `rgb(#123, 0.5)`, disagrees with both), so they stay red rather than being fitted from a neighbours' law.
// `adjust/space.hrx` wants a conversion this compiler has not got, and the `adjust/{lab,lch,oklab,oklch,
// srgb,display_p3,rec2020,prophoto_rgb,a98_rgb,srgb_linear,xyz_d50,xyz_d65}.hrx` files want the same modern
// spaces; `adjust/global.hrx::legacy` wants the `adjust-color()` spelling, `adjust/no_channels.hrx` and
// `adjust/hsl.hrx::all` are covered above, and the `adjust/error/**` rows want the message text — tasks.md
// 1.5's P7 half rather than the arithmetic.
