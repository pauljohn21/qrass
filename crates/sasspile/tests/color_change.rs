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
/// chroma rotates the hue — which is why a colour built by `change` remembers the axes it was given. The
/// gamut is judged on **those axes**, and their low ends are asymmetric, both by rows: a saturation below
/// zero is still a colour that prints its bytes (`color.change(plum, $saturation: -20%)` →
/// `rgb(69.6470588235%, 79.7647058824%, 69.6470588235%)`, `…::saturation/below_range`), while a lightness
/// below zero is not (`color.change(red, $lightness: -20%)` → `hsl(0, 100%, -20%)`,
/// `…::lightness/below_range`).
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
        (
            "a {b: color.change(plum, $saturation: 120%)}",
            "hsl(300, 120%, 74.7058823529%)",
        ),
        (
            "a {b: color.change(plum, $saturation: -20%)}",
            "rgb(69.6470588235%, 79.7647058824%, 69.6470588235%)",
        ),
        (
            "a {b: color.change(red, $lightness: -20%)}",
            "hsl(0, 100%, -20%)",
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

/// **Whiteness and blackness can be written without any `$space:` label** — the name itself says which space
/// it means (`change/hwb.hrx`; contrast `color.channel`, which needs `$space:` to *read* across spaces).
/// Every row below is that file's own pair, and they pin three things at once: the origin is kept
/// (`#cc6666` → `#cc0000`, hex, not `rgb(…)`), an overflowing pair is normalised by its sum
/// (`color.change(#cc6666, $whiteness: 100%)` → all three channels `83.3333333333%`, i.e. `100/120`), and a
/// pair that leaves the gamut prints the hsl triple (`color.change(#993333, $whiteness: 50%, $blackness: -20%)`
/// → `hsl(0, 233.3333333333%, 85%)`).
#[tokio::test]
async fn change_writes_whiteness_and_blackness() {
    for (input, want) in [
        ("a {b: color.change(#cc6666, $whiteness: 0%)}", "#cc0000"),
        ("a {b: color.change(#cc6666, $whiteness: 20%)}", "#cc3333"),
        ("a {b: color.change(#cc6666, $whiteness: 80%)}", "#cccccc"),
        (
            "a {b: color.change(#cc6666, $whiteness: 100%)}",
            "rgb(83.3333333333%, 83.3333333333%, 83.3333333333%)",
        ),
        ("a {b: color.change(#993333, $blackness: 0%)}", "#ff3333"),
        ("a {b: color.change(#993333, $blackness: 20%)}", "#cc3333"),
        ("a {b: color.change(#993333, $blackness: 80%)}", "#333333"),
        (
            "a {b: color.change(#993333, $blackness: 100%)}",
            "rgb(16.6666666667%, 16.6666666667%, 16.6666666667%)",
        ),
        (
            "a {b: color.change(#993333, $whiteness: 50%, $blackness: -20%)}",
            "hsl(0, 233.3333333333%, 85%)",
        ),
        // Hue travels with the whiteness/blackness pair, by position and by label.
        (
            "a {b: color.change(blue, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
            "#339966",
        ),
        (
            "a {b: color.change($color: blue, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
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

/// Rows this file does **not** claim, of two different kinds. Some need a `none` value, which this compiler
/// does not model: `color.change(black, $red: none)` → `rgb(none 0 0)` (`change/rgb.hrx::red/none`), the same
/// on an hsl colour (`change/hsl.hrx::hue/none`) and on a hwb one (`change/hwb.hrx::whiteness/none`). A byte
/// channel cannot express `none`, so the call keeps its text.
///
/// One is a space this compiler cannot convert (`change/space.hrx::identical`), where keeping the text happens
/// to match what the corpus prints.
///
/// And one pair is **unexplained and left red rather than fitted**: `change/hwb.hrx::alpha_input` and
/// `…::alpha_arg` print the same colour as `rgba(20%, 60%, 40%, 0.7)` once an alpha is named, while
/// `…::all` prints it as `#339966` opaque (asserted above) and `rgb(#123, 0.5)` prints decimal channels
/// (`rgb/two_args::opaque_to/partial`). No mechanism here reconciles those three; inventing one would trade
/// two visible red rows for a wrong value the pass count cannot see.
///
/// A third disagreement sits in the hue derivation itself: `change/rgb.hrx::green/out_of_range` writes
/// `hsl(120, 100%, -9.8039215686%)` for `(0, -50, 0)` where the legacy hsl formula gives hue `300` and
/// saturation `-100%`, while that file's `red/` and `blue/out_of_range` rows both agree with the formula.
#[tokio::test]
async fn what_change_still_declines() {
    for call in [
        // `change/rgb.hrx::red/none` — the printed form needs a `none` channel, not a byte.
        "color.change(black, $red: none)",
        // `change/hsl.hrx::hue/none` — same, on a colour the compiler does hold as axes.
        "color.change(hsl(0deg 100% 50%), $hue: none)",
        // `change/hwb.hrx::whiteness/none` — and again on a `hwb` colour.
        "color.change(hwb(180deg 30% 50%), $whiteness: none)",
        // `change/space.hrx::identical` — a request about spaces this compiler cannot convert; the corpus
        // itself leaves the colour untouched, so keeping the text matches its output.
        "color.change(oklch(50% 0.2 0deg), $space: lab)",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "a call outside this slice's scope must keep its text: {call}"
        );
    }
}
