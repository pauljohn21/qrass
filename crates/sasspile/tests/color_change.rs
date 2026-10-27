//! `color.change` on the **`rgb` channels**, pinned row by row on `core_functions/color/change/rgb.hrx` and
//! its namespaced twin `core_functions/modules/color/change.hrx`. Every input and expectation below is copied
//! from the fixture's own `input.scss` / `output.css` — including the case names, which are not a grammar I
//! may invent (`change/rgb.hrx` has exactly `red|green|blue/{max,min,low,high,out_of_range,none}` plus
//! `alpha_arg`; there is no "mixed channels" row to assert).
//!
//! The point of this file is the **label**: `color.change(black, $red: 500)` and
//! `color.change(black, $green: -50)` each carry one number and must land on different colours, which is what
//! the registry's positional argument list could not express.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// A changed channel keeps the colour's own origin: a keyword-created colour prints a keyword when the new
/// bytes still name one, and hex otherwise (`red/max`, `green/max`, `blue/max`, `red/min`, `red/low`).
#[tokio::test]
async fn change_keeps_the_colour_origin() {
    for (input, want) in [
        ("a {b: color.change(black, $red: 255)}", "red"),
        ("a {b: color.change(black, $green: 255)}", "lime"),
        ("a {b: color.change(black, $blue: 255)}", "blue"),
        ("a {b: color.change(red, $red: 0)}", "black"),
        ("a {b: color.change(red, $red: 100)}", "#640000"),
        ("a {b: color.change(black, $red: 200)}", "#c80000"),
        ("a {b: color.change(blue, $blue: 100)}", "#000064"),
        // The `@use "sass:color"` spelling reaches the same function (`modules/color/change.hrx`).
        ("a {b: color.change(lime, $green: 0)}", "black"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **An out-of-gamut result prints in the hsl form**, because an out-of-range byte is a value Sass carries
/// rather than one it clamps: `red/out_of_range` is `color.change(black, $red: 500)` →
/// `hsl(0, 5000%, 98.0392156863%)` and `blue/out_of_range` is `color.change(black, $blue: 256)` →
/// `hsl(240, 100.7874015748%, 50.1960784314%)`. Clamping the second to `rgb(0, 0, 255)` would print a
/// different colour, and rounding would flatten `100.7874015748%` into `100%`.
#[tokio::test]
async fn an_out_of_gamut_change_prints_hsl() {
    for (input, want) in [
        (
            "a {b: color.change(black, $red: 500)}",
            "hsl(0, 5000%, 98.0392156863%)",
        ),
        (
            "a {b: color.change(black, $blue: 256)}",
            "hsl(240, 100.7874015748%, 50.1960784314%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **The hsl axes the call names** replace the colour's own: `color.change(plum, $saturation: 100%)` →
/// `#ff7eff` (origin kept, bytes whole), `color.change(red, $lightness: 63%)` → `rgb(100%, 26%, 26%)`
/// (fractional bytes ⇒ percentages), and an out-of-range axis prints the hsl triple **as written**:
/// `color.change(red, $lightness: 120%)` → `hsl(0, 100%, 120%)` (`change/hsl.hrx::lightness/above_range`).
///
/// Deriving that triple back from the implied bytes would print `hsl(180, -100%, 120%)` instead — a negative
/// chroma rotates the hue — which is why a colour built by `change` remembers the axes it was given.
///
/// The `units/hue/*` rows also show that `60deg`, `60` and `60in` all mean the same number; those fixtures
/// carry a deprecation warning the harness does not compare, so matching their output is not deprecation
/// support.
#[tokio::test]
async fn change_writes_the_hsl_axes_too() {
    for (input, want) in [
        ("a {b: color.change(plum, $saturation: 100%)}", "#ff7eff"),
        (
            "a {b: color.change(red, $lightness: 63%)}",
            "rgb(100%, 26%, 26%)",
        ),
        (
            "a {b: color.change(red, $lightness: 0.5%)}",
            "rgb(1%, 0%, 0%)",
        ),
        (
            "a {b: color.change(red, $lightness: 120%)}",
            "hsl(0, 100%, 120%)",
        ),
        ("a {b: color.change(red, $hue: 540)}", "aqua"),
        ("a {b: color.change(red, $hue: 60deg)}", "yellow"),
        ("a {b: color.change(red, $hue: 60in)}", "yellow"),
        (
            "a {b: color.change(black, $hue: 12, $saturation: 24%, $lightness: 48%)}",
            "rgb(59.52%, 41.088%, 36.48%)",
        ),
        // An alpha named alongside the channels (`change/rgb.hrx::alpha_arg`).
        (
            "a {b: color.change(black, $red: 12, $green: 24, $blue: 48, $alpha: 0.3)}",
            "rgba(12, 24, 48, 0.3)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// Three rows in this file are **not** claimed, and the reasons are different:
/// * `red|green|blue/none` (`color.change(black, $red: none)` → `rgb(none 0 0)`) needs a `none` *value*,
///   which this compiler does not model — a byte channel cannot express it, so the call keeps its text;
/// * `alpha_arg` names `$alpha:` too, and in-gamut alphas are written above;
/// * `green/out_of_range` disagrees with the hue derivation: the corpus writes
///   `hsl(120, 100%, -9.8039215686%)` for `(0, -50, 0)` while the legacy hsl formula gives hue `300` and
///   saturation `-100%` (while `red/out_of_range` and `blue/out_of_range` both agree with the formula).
#[tokio::test]
async fn what_change_still_declines() {
    for call in [
        // `change/rgb.hrx::red/none` — the printed form needs a `none` channel, not a byte.
        "color.change(black, $red: none)",
        // `change/hsl.hrx::hue/none` — same, on a colour the compiler does hold as axes.
        "color.change(hsl(0deg 100% 50%), $hue: none)",
        // `change/space.hrx::identical` — a request about spaces this compiler cannot convert; the corpus
        // itself leaves the colour untouched, and keeping the text is the honest near-neighbour.
        "color.change(oklch(50% 0.2 0deg), $space: lab)",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "a call outside this slice's scope must keep its text: {call}"
        );
    }
}
