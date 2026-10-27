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
        // The one-argument spelling also arrives **by label**: `channels` is a real parameter of `rgb`/`hsl`
        // (the corpus's own diagnostics are prefixed `$channels:` — `rgb/error/one_arg.hrx`), and a labelled
        // argument binds wherever it sits (`rgb/one_arg/no_alpha.hrx::unitless/named`).
        ("a {b: rgb($channels: 0 255 127)}", "rgb(0, 255, 127)"),
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
        ("a {b: hsl($channels: 0 100% 50%)}", "hsl(0, 100%, 50%)"),
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

/// The modern **alpha-after-a-slash** spelling, one assertion per row of
/// `rgb/one_arg/alpha.hrx` and `hsl/one_arg/alpha.hrx`. The corpus states what the structure is: its
/// `slash_list` row writes `rgb(list.slash(0 255 127, 0))` and expects the same colour as
/// `rgb(0 255 127 / 0)` (`in_gamut/transparent`), so the alpha is a slash list's **second element**, not a
/// division of the last channel — which is also why a component like `127/0.3` (a `/` glued inside it) stays
/// arithmetic.
///
/// Rows still NOT claimed here, because they need abilities this compiler lacks:
/// * `slash_list` — `list.slash(…)` is not registered yet;
/// * `missing/slash` and `missing/slash_list` — `none` as an alpha, where the corpus keeps the run as text
///   for `rgb` but half-reformats it (`hsl(180deg 60% 50% / none)`) for `hsl`.
#[tokio::test]
async fn modern_alpha_follows_a_slash() {
    for (input, want) in [
        ("a {b: rgb(18 52 86 / 0.5)}", "rgba(18, 52, 86, 0.5)"),
        // A percentage alpha converts, same law as the comma form's fourth argument.
        ("a {b: rgb(18 52 86 / 50%)}", "rgba(18, 52, 86, 0.5)"),
        // An alpha of one returns the three-argument spelling; zero does not.
        ("a {b: rgb(190 173 237 / 1)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0 255 127 / 0)}", "rgba(0, 255, 127, 0)"),
        ("a {b: rgb(0 255 127 / 0.3)}", "rgba(0, 255, 127, 0.3)"),
        // The same call written with the label (`rgb/one_arg/alpha.hrx::in_gamut/named`).
        (
            "a {b: rgb($channels: 0 255 127 / 0.3)}",
            "rgba(0, 255, 127, 0.3)",
        ),
        ("a {b: hsl(180 60% 50% / 0.5)}", "hsla(180, 60%, 50%, 0.5)"),
        (
            "a {b: hsl($channels: 180 60% 50% / 0.4)}",
            "hsla(180, 60%, 50%, 0.4)",
        ),
        ("a {b: hsl(180 60% 50% / 1)}", "hsl(180, 60%, 50%)"),
        ("a {b: hsl(180 60% 50% / 0)}", "hsla(180, 60%, 50%, 0)"),
        // The alpha clamps, the axes keep their own laws (negative saturation to 0, lightness untouched).
        ("a {b: hsl(0 100% 50% / 1.1)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0 100% 50% / -0.1)}", "hsla(0, 100%, 50%, 0)"),
        ("a {b: hsl(0 -0.1% 50% / 0.5)}", "hsla(0, 0%, 50%, 0.5)"),
        (
            "a {b: hsl(0 100% 9999% / 0.5)}",
            "hsla(0, 100%, 9999%, 0.5)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
