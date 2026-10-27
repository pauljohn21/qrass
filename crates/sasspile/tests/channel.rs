//! `color.channel`, pinned on `core_functions/color/channel/{rgb,hsl,srgb,named}.hrx`. Every input and
//! expectation below is that fixture's own `input.scss` / `output.css` line, copied rather than shaped:
//! the reading is space-dependent (`"red"` of `pink` is `255` in `rgb` and `1` in `srgb`), so a plausible
//! number is not evidence.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// The colour's **own** space, which is what a call without `$space:` means
/// (`channel/rgb.hrx::local/*` reads hex, `…::foreign/*` reads a keyword-created colour).
#[tokio::test]
async fn channel_reads_the_colour_own_space() {
    for (input, want) in [
        ("a {b: color.channel(#abcdef, \"red\")}", "171"),
        ("a {b: color.channel(#abcdef, \"green\")}", "205"),
        ("a {b: color.channel(#abcdef, \"blue\")}", "239"),
        ("a {b: color.channel(pink, \"red\")}", "255"),
        ("a {b: color.channel(pink, \"green\")}", "192"),
        ("a {b: color.channel(pink, \"blue\")}", "203"),
        // An `hsl` colour's own space is hsl: hue carries `deg`, the other axes `%` (`channel/hsl.hrx`).
        ("a {b: color.channel(hsl(10 40% 60%), \"hue\")}", "10deg"),
        (
            "a {b: color.channel(hsl(10deg 40% 60%), \"saturation\")}",
            "40%",
        ),
        (
            "a {b: color.channel(hsl(10deg 40% 60%), \"lightness\")}",
            "60%",
        ),
        // The hue was already folded into `0–360` when the colour was built (`hsl(540 …)` → `180deg`).
        ("a {b: color.channel(hsl(540 40% 60%), \"hue\")}", "180deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A **cross-space** read happens only because the call says so with `$space:`
/// (`channel/hsl.hrx::foreign/*`, `channel/srgb.hrx::foreign/*`); the numbers are the same colour measured
/// on another ruler, so `srgb` divides the byte by 255 and prints a bare number.
#[tokio::test]
async fn a_space_label_selects_the_axes() {
    for (input, want) in [
        (
            "a {b: color.channel(pink, \"hue\", $space: hsl)}",
            "349.5238095238deg",
        ),
        (
            "a {b: color.channel(pink, \"saturation\", $space: hsl)}",
            "100%",
        ),
        (
            "a {b: color.channel(pink, \"lightness\", $space: hsl)}",
            "87.6470588235%",
        ),
        ("a {b: color.channel(pink, \"red\", $space: srgb)}", "1"),
        (
            "a {b: color.channel(pink, \"green\", $space: srgb)}",
            "0.7529411765",
        ),
        (
            "a {b: color.channel(pink, \"blue\", $space: srgb)}",
            "0.7960784314",
        ),
        // Every argument by label (`channel/named.hrx`).
        (
            "a {b: color.channel($color: pink, $channel: \"hue\", $space: hsl)}",
            "349.5238095238deg",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// What this slice must **not** answer. Each call keeps its written form because the space or the colour
/// needs a conversion that has no implementation here — and a number read off the wrong axes would be a
/// wrong value the pass count cannot see:
/// * `hwb` (`channel/hwb.hrx::foreign/*` wants `pink`'s whiteness as `75.2941176471%`) — no hwb conversion;
/// * an `srgb`-space colour written in `color()` syntax (`channel/srgb.hrx::local/red`) — `Color::of`
///   refuses parenthesised text for exactly this reason;
/// * `whiteness`/`blackness` of an `hsl` colour, and any unknown channel name.
#[tokio::test]
async fn an_unreachable_space_declines_rather_than_guesses() {
    for call in [
        "color.channel(pink, \"whiteness\", $space: hwb)",
        "color.channel(pink, \"blackness\", $space: hwb)",
        "color.channel(color(srgb 0.2 0.5 0.8), \"red\")",
        "color.channel(hsl(10deg 40% 60%), \"whiteness\", $space: hwb)",
        "color.channel(#abcdef, \"luminance\")",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unreadable channel request must not be answered: {call}"
        );
    }
}
