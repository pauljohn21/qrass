//! CSS Color 4's **one-argument** colour spelling (`rgb(18 52 86)`, `hsl(0 100% 50%)`), and the clamping
//! law the out-of-range percentage rows expose. Each expectation is that fixture's own `output.css`.
//!
//! The reading is a lexeme-level fact: `calls::split` used to strip whitespace from a whole call run to find
//! the callee, which left `rgb(18 52 86)` arriving as `[Word("18"), Word("52"), Word("86")]` with no
//! separators, so the space list was unreadable at any granularity below the lexeme. Keeping layout inside
//! the parentheses is what makes these rows readable at all.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `rgb/one_arg/no_alpha`: three space-separated channels are one argument, printed back in the legacy
/// comma form the same file's `three_args` rows use. A percentage anywhere converts the whole colour
/// (`percents/unitless/green`), out-of-range channels clamp (`unitless/clamped/*`), and a run with an
/// unparseable component (`none`) stays verbatim (`missing/*`).
#[tokio::test]
async fn rgb_space_separated_channels() {
    for (input, want) in [
        ("a {b: rgb(18 52 86)}", "rgb(18, 52, 86)"),
        ("a {b: rgb(190 173 237)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0 255 127)}", "rgb(0, 255, 127)"),
        // Deliberately NOT asserted: `unitless/named` writes `rgb($channels: 0 255 127)` and wants
        // `rgb(0, 255, 127)`, so `channels` is a real parameter of the one-argument form. Binding currently
        // hands a body only the parameters up to the first absent one, so a lone `$channels:` reaches the body
        // as nothing and the call keeps its text — that row stays red in `ledger`, and asserting today's
        // output here would write an unimplemented binding up as if it were the law.
        ("a {b: rgb(256 0 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0 -1 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(7.1% 20.4% 33.9%)}", "rgb(7.1%, 20.4%, 33.9%)"),
        (
            "a {b: rgb(74.7% 173 93%)}",
            "rgb(74.7%, 67.8431372549%, 93%)",
        ),
        (
            "a {b: rgb(190 68% 237)}",
            "rgb(74.5098039216%, 68%, 92.9411764706%)",
        ),
        ("a {b: rgb(0% 100% 50%)}", "rgb(0%, 100%, 50%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `rgb/one_arg/no_alpha::percents/clamped/*` states the clamping law no legacy comma fixture exercised: a
/// percentage that had to move **loses its `%` spelling** and prints as the byte it clamped to. An exact
/// `100%` (`percents/boundaries`) does not move, so it keeps the `%`. The same law reads back on the legacy
/// comma rows the change also turned green (`three_args/percents::clamped/{red,green,blue}`).
#[tokio::test]
async fn a_clamped_percentage_prints_as_a_byte() {
    for (input, want) in [
        ("a {b: rgb(100.1% 0 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0 -0.1% 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(0 0 200%)}", "rgb(0, 0, 255)"),
        ("a {b: rgb(100.1%, 0, 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0, -0.1%, 0)}", "rgb(0, 0, 0)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hsl/one_arg/no_alpha`: the same axes as the comma form, reached through the space-separated reading.
/// Hue folds to degrees (`units/hue/deg`), unitless saturation and lightness gain a `%`
/// (`units/{saturation,lightness}/unitless`), saturation clamps at 0 but lightness does not go negative in
/// the out-of-range direction the fixtures show (`out_of_gamut/saturation/above` keeps `500%`).
#[tokio::test]
async fn hsl_space_separated_axes() {
    for (input, want) in [
        ("a {b: hsl(0 100% 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(240 100% 50%)}", "hsl(240, 100%, 50%)"),
        ("a {b: hsl(60 60% 50%)}", "hsl(60, 60%, 50%)"),
        ("a {b: hsl(0 500% 50%)}", "hsl(0, 500%, 50%)"),
        ("a {b: hsl(0 -100% 50%)}", "hsl(0, 0%, 50%)"),
        ("a {b: hsl(0 100% 500%)}", "hsl(0, 100%, 500%)"),
        ("a {b: hsl(0deg 100% 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0 50 50%)}", "hsl(0, 50%, 50%)"),
        ("a {b: hsl(0 100% 50)}", "hsl(0, 100%, 50%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A component the reading cannot turn into a value refuses the whole run — so `rgb(none 52 66)` prints
/// exactly as written, including the interior spaces, and does not collapse into a three-argument list or a
/// division (`rgb/one_arg/no_alpha::missing/*`).
#[tokio::test]
async fn an_unreadable_component_keeps_the_run_verbatim() {
    for input in [
        "a {b: rgb(none 52 66)}",
        "a {b: rgb(18 none 66)}",
        "a {b: rgb(18 52 none)}",
    ] {
        let body = input
            .strip_prefix("a {b: ")
            .unwrap()
            .strip_suffix("}")
            .unwrap();
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {body};\n}}\n"),
            "for input {input}"
        );
    }
}
