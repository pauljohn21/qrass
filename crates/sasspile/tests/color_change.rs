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

/// Three rows in this file are **not** claimed, and the reasons are different:
/// * `red|green|blue/none` (`color.change(black, $red: none)` → `rgb(none 0 0)`) needs a `none` *value*,
///   which this compiler does not model — a byte channel cannot express it, so the call keeps its text;
/// * `alpha_arg` (`color.change(black, $red: 12, $green: 24, $blue: 48, $alpha: 0.3)` →
///   `rgba(12, 24, 48, 0.3)`) names `$alpha:` too, and whether `change` clamps an out-of-range alpha is a
///   question its rows have not answered yet, so the body declines rather than assuming `with_alpha`'s law;
/// * `green/out_of_range` disagrees with the hue derivation: the corpus writes
///   `hsl(120, 100%, -9.8039215686%)` for `(0, -50, 0)` while the legacy hsl formula gives hue `300` and
///   saturation `-100%` (while `red/out_of_range` and `blue/out_of_range` both agree with the formula).
#[tokio::test]
async fn what_change_still_declines() {
    for call in [
        "color.change(black, $red: none)",
        "color.change(black, $red: 12, $green: 24, $blue: 48, $alpha: 0.3)",
        "color.change(#abcdef, $hue: 180)",
        "color.change(red, $red: 1, $space: srgb)",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "a call outside this slice's scope must keep its text: {call}"
        );
    }
}
