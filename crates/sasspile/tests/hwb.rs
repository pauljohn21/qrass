//! `hwb` / `color.hwb`, pinned on `core_functions/color/hwb/**` and `channel/hwb.hrx`. Each input and each
//! expectation is that fixture's own `input.scss` / `output.css` line.
//!
//! Two laws here are unlike anything in `rgb`/`hsl`, and both come from rows rather than from reasoning:
//! * a `hwb` colour **prints as hsl** (`hwb(0deg, 30%, 40%)` → `hsl(0, 33.3333333333%, 45%)`) yet keeps its
//!   whiteness and blackness readable (`color.channel(hwb(10deg 70% 50%), "whiteness")` → `58.3333333333%`,
//!   i.e. `70/120` — the overflow is normalised **at creation**);
//! * a whole-byte `hwb` colour whose bytes name a keyword prints **that keyword** (`red`), which an `rgb`
//!   call does not do (`rgb(0, 0, 0)` prints itself); the corpus does not say why, so nothing here
//!   generalises it.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `hwb/three_args/units.hrx`: the hue follows the `hsl` angle law, and a unit that is **not** an angle is
/// taken as a plain number instead of being rejected (`hwb/three_args/named.hrx` binds the labels).
#[tokio::test]
async fn hwb_prints_as_hsl_and_folds_its_hue() {
    for (input, want) in [
        ("a {b: hwb(0deg, 30%, 40%)}", "hsl(0, 33.3333333333%, 45%)"),
        (
            "a {b: hwb(1rad, 30%, 40%)}",
            "hsl(57.2957795131, 33.3333333333%, 45%)",
        ),
        ("a {b: hwb(1in, 30%, 40%)}", "hsl(1, 33.3333333333%, 45%)"),
        (
            "a {b: color.hwb($hue: 0, $whiteness: 30%, $blackness: 40%)}",
            "hsl(0, 33.3333333333%, 45%)",
        ),
        // The namespaced spelling, and the alpha: one returns the three-argument form, zero does not
        // (`hwb/global.hrx::alpha/in_gamut/{opaque,transparent}`).
        (
            "a {b: color.hwb(0, 30%, 40%)}",
            "hsl(0, 33.3333333333%, 45%)",
        ),
        (
            "a {b: hwb(180 30% 40% / 1)}",
            "hsl(180, 33.3333333333%, 45%)",
        ),
        (
            "a {b: hwb(180 30% 40% / 0)}",
            "hsla(180, 33.3333333333%, 45%, 0)",
        ),
        (
            "a {b: hwb(180 30% 40% / 0.5)}",
            "hsla(180, 33.3333333333%, 45%, 0.5)",
        ),
        (
            "a {b: hwb($channels: 180 30% 40% / 0.4)}",
            "hsla(180, 33.3333333333%, 45%, 0.4)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// Whiteness and blackness are not clamped, but a pair that overflows the colour is **normalised by its
/// sum** — `hwb(0 101% 40%)` gives `1.01 / 1.41`, and a negative whiteness is left to push the saturation
/// past 100% (`hwb/one_arg.hrx::{whiteness/above_max,whiteness/below_min}`, same numbers as the
/// comma-spelled `hwb/four_args` rows).
#[tokio::test]
async fn overflowing_whiteness_and_blackness_normalise() {
    for (input, want) in [
        ("a {b: hwb(0 101% 40%)}", "hsl(0, 0%, 71.6312056738%)"),
        ("a {b: hwb(0 -1% 40%)}", "hsl(0, 103.3898305085%, 29.5%)"),
        (
            "a {b: hwb(0, 101%, 40%, 0.5)}",
            "hsla(0, 0%, 71.6312056738%, 0.5)",
        ),
        (
            "a {b: hwb(0, -1%, 40%, 0.5)}",
            "hsla(0, 103.3898305085%, 29.5%, 0.5)",
        ),
        // The normalisation is visible when the channels are read back (`channel/hwb.hrx::local/normalized`).
        (
            "a {b: color.channel(hwb(10deg 70% 50%), \"whiteness\")}",
            "58.3333333333%",
        ),
        (
            "a {b: color.channel(hwb(10deg 70% 50%), \"blackness\")}",
            "41.6666666667%",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hwb/four_args::alpha/percent/max`: `color.hwb(0, 0%, 0%, 100%)` is pure red and prints **`red`** —
/// while `rgb(0, 0, 0)` prints itself (`rgb/three_args/unitless::numbers`). A non-keyword `hwb` colour keeps
/// the hsl form, and a `hwb` colour moved by an axis operation loses its hue when the result is a grey
/// (`grayscale.hrx::legacy/powerless/different`), which is why that path returns an `hsl` colour.
#[tokio::test]
async fn a_hwb_colour_prints_its_keyword_and_loses_hue_when_greyed() {
    for (input, want) in [
        ("a {b: color.hwb(0, 0%, 0%, 100%)}", "red"),
        ("a {b: hwb(0deg, 30%, 40%)}", "hsl(0, 33.3333333333%, 45%)"),
        (
            "a {b: color.grayscale(hwb(120deg 10% 20%))}",
            "hsl(0, 0%, 45%)",
        ),
        // An `hwb` colour answers its own channels, and `hue` is the stored angle.
        ("a {b: color.channel(hwb(10 30% 40%), \"hue\")}", "10deg"),
        (
            "a {b: color.channel(pink, \"whiteness\", $space: hwb)}",
            "75.2941176471%",
        ),
        (
            "a {b: color.channel(pink, \"blackness\", $space: hwb)}",
            "0%",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A `var(` argument makes the whole call unreadable, and every such row keeps its text — including the
/// interior spaces (`hwb/one_arg.hrx::{hue/var,whiteness/var,blackness/var/no_alpha}`).
#[tokio::test]
async fn an_unreadable_hwb_argument_keeps_the_run_verbatim() {
    for call in [
        "hwb(var(--c) 30% 40%)",
        "hwb(0 var(--c) 40%)",
        "hwb(0 30% var(--c))",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unreadable hwb call must not be answered: {call}"
        );
    }
}
