//! The colour law, pinned on `core_functions/color/rgb/**` goldens. Each expectation is that fixture's own
//! `output.css`, not a derivation: the representation a colour was created in decides how it prints, so
//! the same channels come out as `rgb(…)`, `rgb(…%…)`, `rgba(…)`, or `#112233`.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `rgb/three_args/unitless`: channels print as written, and out-of-range ones clamp to `0–255`
/// (`clamped/{red,green,blue}`).
#[tokio::test]
async fn unitless_channels_print_and_clamp() {
    for (input, want) in [
        ("a {b: rgb(18, 52, 86)}", "rgb(18, 52, 86)"),
        ("a {b: rgb(190, 173, 237)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0, 255, 127)}", "rgb(0, 255, 127)"),
        ("a {b: rgb(256, 0, 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0, -1, 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(0, 0, 9999)}", "rgb(0, 0, 255)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `rgb/three_args/percents`: a percentage anywhere makes the whole colour percentage-form, so a unitless
/// channel is converted (`173` → `67.8431372549%`) while a written percentage is kept exactly as spelled.
#[tokio::test]
async fn one_percentage_channel_makes_all_percentages() {
    assert_eq!(
        css("a {b: rgb(7.1%, 20.4%, 33.9%)}").await,
        "a {\n  b: rgb(7.1%, 20.4%, 33.9%);\n}\n"
    );
    assert_eq!(
        css("a {b: rgb(74.7%, 173, 93%)}").await,
        "a {\n  b: rgb(74.7%, 67.8431372549%, 93%);\n}\n"
    );
}

/// `rgb/two_args::opaque_to/*` and `rgb/four_args/clamped/*`: a colour built from hex prints as hex while
/// opaque and as `rgba(…)` the moment it is translucent, and an alpha argument selects the four-argument
/// spelling. A negative zero is `0`, not `-0`.
#[tokio::test]
async fn representation_and_alpha_choose_the_spelling() {
    for (input, want) in [
        ("a {b: rgb(#123, 1)}", "#112233"),
        ("a {b: rgb(#123, 0.5)}", "rgba(17, 34, 51, 0.5)"),
        ("a {b: rgb(256, 0, 0, 0.5)}", "rgba(255, 0, 0, 0.5)"),
        ("a {b: rgb(0, -0, 0, 0.5)}", "rgba(0, 0, 0, 0.5)"),
        ("a {b: rgb(0, 0, 9999, 0.5)}", "rgba(0, 0, 255, 0.5)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hsl/three_args/units.hrx` and `…/bounds.hrx`, read row by row: the **hue is folded to degrees and
/// wrapped into `0–360`** (`60rad` → `197.7467707849`, `0deg` → `0`), saturation and lightness **always**
/// carry a `%` (`50` → `50%`), and unlike an `rgb` channel they are **not clamped** (`500%` stays).
/// `hsl/four_args/alpha.hrx` adds the alpha law: a percentage alpha converts, and an alpha of one returns
/// the three-argument spelling.
#[tokio::test]
async fn hsl_folds_hue_and_prints_percentages() {
    for (input, want) in [
        ("a {b: hsl(0, 100%, 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0deg, 100%, 50%)}", "hsl(0, 100%, 50%)"),
        (
            "a {b: hsl(60rad, 100%, 50%)}",
            "hsl(197.7467707849, 100%, 50%)",
        ),
        ("a {b: hsl(0, 50, 50%)}", "hsl(0, 50%, 50%)"),
        ("a {b: hsl(0, 500%, 50%)}", "hsl(0, 500%, 50%)"),
        ("a {b: hsl(0, 0%, 0%, 45.6%)}", "hsla(0, 0%, 0%, 0.456)"),
        ("a {b: hsl(0, 0%, 0%, 100%)}", "hsl(0, 0%, 0%)"),
        ("a {b: hsl(0, 0%, 0%, 250%)}", "hsl(0, 0%, 0%)"),
        ("a {b: hsl(0, 0%, 0%, -10%)}", "hsla(0, 0%, 0%, 0)"),
        // Labels bind, for `hsl` and `rgb` alike — the reason the registry carries optional parameters at
        // all (`hsl/three_args/named.hrx`, `rgb/three_args/unitless::named`).
        (
            "a {b: hsl($hue: 0, $saturation: 100%, $lightness: 50%)}",
            "hsl(0, 100%, 50%)",
        ),
        (
            "a {b: rgb($red: 0, $green: 255, $blue: 127)}",
            "rgb(0, 255, 127)",
        ),
        // An unmodellable argument keeps the call as written (`hsl/three_args/special_functions::var/*`).
        ("a {b: hsl(var(--foo), 2%, 3%)}", "hsl(var(--foo), 2%, 3%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// What must **not** happen. An argument the colour law cannot read declines the call, and the run keeps
/// its verbatim text — `three_args::1_of_3` (`var()` is substituted before parsing, so the list can hold
/// anything) is a golden of exactly that. And a hex written straight into a declaration is not a `Color`
/// at all: `values/colors/alpha_hex/initial_letter` prints `#AbCd`, capitalisation intact, so normalising
/// it here would be a regression dressed as a win.
#[tokio::test]
async fn unmodellable_arguments_decline_and_hex_is_left_alone() {
    assert_eq!(
        css("a {b: rgb(var(--foo), 3, 0.4)}").await,
        "a {\n  b: rgb(var(--foo), 3, 0.4);\n}\n"
    );
    assert_eq!(css("a {b: #AbCd}").await, "a {\n  b: #AbCd;\n}\n");
    assert_eq!(css("a {b: #98765432}").await, "a {\n  b: #98765432;\n}\n");
    // Channels are numbers or percentages; `px` is neither, and no error message is invented for it (P7).
    assert_eq!(
        css("a {b: rgb(1px, 2px, 3px)}").await,
        "a {\n  b: rgb(1px, 2px, 3px);\n}\n"
    );
}

/// The `color.*` accessors, and with them the audit that option (a) promised: the keyword table comes from
/// the `color-name` crate's data, so every expectation here is a name the **corpus itself** pins.
/// `gray` is fixed twice over — `equality.hrx` states `gray == hsl(none 0% 50.196078431373%)` (128/255) and
/// `color.invert(gray)` → `#7f7f7f` (128 inverted is 127); `springgreen` is what
/// `rgb/three_args/unitless::springgreen` writes for `rgb(0, 255, 127)`; `purple` and `pink` come from
/// `equality.hrx` and `channel/hwb::foreign/whiteness` (`75.2941176471%` is `192/255`).
#[tokio::test]
async fn accessors_read_channels_and_the_keyword_table_matches_the_corpus() {
    for (input, want) in [
        // The fixture rows themselves (`core_functions/color/red.hrx::{max,min,middle,named}`).
        ("a {b: color.red(rgb(255, 0, 0))}", "255"),
        ("a {b: color.red(rgb(0, 0, 0))}", "0"),
        ("a {b: color.red(rgb(123, 0, 0))}", "123"),
        ("a {b: color.red($color: rgb(234, 0, 0))}", "234"),
        // `color.alpha` (`alpha.hrx::color/{max,middle,min,named}`).
        ("a {b: color.alpha(red)}", "1"),
        ("a {b: color.alpha(rgba(red, 0.42))}", "0.42"),
        ("a {b: color.alpha(rgba(red, 0))}", "0"),
        ("a {b: color.alpha($color: rgba(red, 0.73))}", "0.73"),
        // Keyword parsing, cross-checked against the corpus values above.
        ("a {b: color.red(black)}", "0"),
        ("a {b: color.red(white)}", "255"),
        ("a {b: color.red(gray)}", "128"),
        ("a {b: color.green(springgreen)}", "255"),
        ("a {b: color.blue(springgreen)}", "127"),
        ("a {b: color.red(purple)}", "128"),
        ("a {b: color.blue(pink)}", "203"),
        // A hex argument arrives as text and is read from there, not from a parsed declaration value.
        ("a {b: color.red(#123)}", "17"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The honest edge: a colour held in the **hsl** space has no `0–255` channels to hand out, and converting
/// is a separate change (its own fixture rows). Reporting a number computed off the wrong axes would be a
/// wrong answer scored as a pass, so the call declines and the text stands.
#[tokio::test]
async fn an_hsl_colour_declines_an_rgb_channel() {
    assert_eq!(
        css("a {b: color.red(hsl(0, 100%, 50%))}").await,
        "a {\n  b: color.red(hsl(0, 100%, 50%));\n}\n"
    );
}
