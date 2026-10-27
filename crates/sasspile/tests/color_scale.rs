//! `color.scale`, pinned row by row on `core_functions/color/scale/{rgb,hsl,hwb,no_space,global}.hrx`.
//! Every input and expectation is copied from the fixture's own `input.scss` / `output.css`.
//!
//! The law this file exists for: a scale moves a channel **toward whichever bound its sign points at, by that
//! share of the distance still between the channel and that bound** — so `sienna`'s red `160` with
//! `$red: 12%` is `160 + (255 − 160) × 0.12`, and `-33%` of `lightcoral`'s red `240` is
//! `240 − 240 × 0.33 = 160.8`.
//! Because the target is the bound itself, a scale can never leave the gamut: `100%` lands **on** it, which is
//! why `scale/rgb.hrx` has no `above_max` row while `adjust/rgb.hrx` does.
//!
//! The other half of what this file pins is the **argument's reading**: as a share (`-33%` means 33), not as a
//! channel value (`byte()` would make `100%` into 255). That mistake was made here first and it sent every
//! `scale/rgb` row out of the gamut, so the rows that catch it are asserted twice over — once through a
//! keyword result (`red/max` → `red`) and once through a fractional one (`red/high` → `89.5137254902%`).

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// Bytes move toward `0` or `255` (`scale/rgb.hrx`). `black` with `$red: 100%` is `red` and `red` with
/// `-100%` is `black` — the bounds themselves; `lightcoral` (red 240) at `-33%` is `160.8` =
/// `63.0588235294%`, and `turquoise` (red 64) at `86%` is `228.26` = `89.5137254902%`. A whole-byte result
/// keeps the colour's own origin: `blue/max` → `blue` by name, `salmon` at `$blue: 42%` → percentages
/// because the moved blue `224.34` is not a whole byte.
#[tokio::test]
async fn bytes_move_toward_the_bound_the_sign_points_at() {
    for (input, want) in [
        ("a {b: color.scale(black, $red: 100%)}", "red"),
        ("a {b: color.scale(red, $red: -100%)}", "black"),
        ("a {b: color.scale(black, $red: 0%)}", "black"),
        (
            "a {b: color.scale(lightcoral, $red: -33%)}",
            "rgb(63.0588235294%, 50.1960784314%, 50.1960784314%)",
        ),
        (
            "a {b: color.scale(turquoise, $red: 86%)}",
            "rgb(89.5137254902%, 87.8431372549%, 81.568627451%)",
        ),
        ("a {b: color.scale(black, $green: 100%)}", "lime"),
        ("a {b: color.scale(lime, $green: -100%)}", "black"),
        ("a {b: color.scale(black, $blue: 100%)}", "blue"),
        ("a {b: color.scale(blue, $blue: -100%)}", "black"),
        (
            "a {b: color.scale(seagreen, $green: -86%)}",
            "rgb(18.0392156863%, 7.631372549%, 34.1176470588%)",
        ),
        (
            "a {b: color.scale(cadetblue, $green: 12%)}",
            "rgb(37.2549019608%, 66.5254901961%, 62.7450980392%)",
        ),
        (
            "a {b: color.scale(salmon, $blue: 42%)}",
            "rgb(98.0392156863%, 50.1960784314%, 67.9294117647%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// Three channels in one call, the labelled colour, and the alpha arguments — `scale/rgb.hrx::{all,named,
/// alpha_input,alpha_arg}`. `sienna` (160, 82, 45) scaled by 12 / 24 / 48% toward 255 is
/// `rgb(67.2156862745%, 48.4392156863%, 57.1764705882%)` whichever way the colour is named, and `$alpha: -70%`
/// moves the alpha from 1 toward 0 by that share → `0.3`, the same number `rgba(sienna, 0.3)` arrives at.
#[tokio::test]
async fn one_call_can_move_several_channels_and_the_alpha() {
    let sienna = "rgb(67.2156862745%, 48.4392156863%, 57.1764705882%)";
    for (input, want) in [
        (
            "a {b: color.scale(sienna, $red: 12%, $green: 24%, $blue: 48%)}",
            sienna,
        ),
        (
            "a {b: color.scale($color: sienna, $red: 12%, $green: 24%, $blue: 48%)}",
            sienna,
        ),
        (
            "a {b: color.scale(rgba(sienna, 0.3), $red: 12%, $green: 24%, $blue: 48%)}",
            "rgba(67.2156862745%, 48.4392156863%, 57.1764705882%, 0.3)",
        ),
        (
            "a {b: color.scale(sienna, $red: 12%, $green: 24%, $blue: 48%, $alpha: -70%)}",
            "rgba(67.2156862745%, 48.4392156863%, 57.1764705882%, 0.3)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The same formula on the hsl axes, whose bounds are `0–100` (`scale/hsl.hrx`). plum's own saturation is
/// 47.2868217054%, so `100%` reaches 100 → `#ff7eff` and `0%` leaves it exactly `plum`; red's lightness 50
/// scaled by `94%` gives 97, whose bytes are `rgb(100%, 94%, 94%)` — the lightness never prints here, because
/// a scale cannot leave the gamut for the out-of-gamut rule to react to.
#[tokio::test]
async fn the_hsl_axes_scale_between_zero_and_a_hundred() {
    for (input, want) in [
        ("a {b: color.scale(plum, $saturation: 100%)}", "#ff7eff"),
        ("a {b: color.scale(plum, $saturation: 0%)}", "plum"),
        (
            "a {b: color.scale(plum, $saturation: -100%)}",
            "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)",
        ),
        (
            "a {b: color.scale(plum, $saturation: 67%)}",
            "rgb(95.6%, 53.8117647059%, 95.6%)",
        ),
        (
            "a {b: color.scale(plum, $saturation: -43%)}",
            "rgb(81.5235294118%, 67.8882352941%, 81.5235294118%)",
        ),
        ("a {b: color.scale(red, $lightness: 100%)}", "white"),
        ("a {b: color.scale(red, $lightness: 0%)}", "red"),
        ("a {b: color.scale(red, $lightness: -100%)}", "black"),
        (
            "a {b: color.scale(red, $lightness: 94%)}",
            "rgb(100%, 94%, 94%)",
        ),
        (
            "a {b: color.scale(red, $lightness: -14%)}",
            "rgb(86%, 0%, 0%)",
        ),
        (
            "a {b: color.scale(turquoise, $saturation: 24%, $lightness: -48%)}",
            "rgb(6.2327249603%, 52.4966868045%, 47.87029062%)",
        ),
        (
            "a {b: color.scale(rgba(turquoise, 0.7), $saturation: 24%, $lightness: -48%)}",
            "rgba(6.2327249603%, 52.4966868045%, 47.87029062%, 0.7)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// Whiteness and blackness scale toward `100` and `0` too, and a pair that then overflows is **normalised by
/// its sum** exactly as `adjust` does it: `#66cc99` (w 40, b 20) with `$whiteness: 100%` becomes w 100 / b 20,
/// which is 83.3333333333% / 16.6666666667% after the normalisation — the grey `rgb(83.3333333333%, …)`
/// (`scale/hwb.hrx::whiteness/max`). `$whiteness: -100%` empties whiteness instead, giving `#00cc66`
/// (`…::whiteness/min`), and `#33cc80` at `50%` keeps a fractional blue (`70.0653594771%`) because the hue
/// survives the round trip (`…::whiteness/high`).
#[tokio::test]
async fn whiteness_and_blackness_scale_then_normalise() {
    for (input, want) in [
        (
            "a {b: color.scale(#66cc99, $whiteness: 100%)}",
            "rgb(83.3333333333%, 83.3333333333%, 83.3333333333%)",
        ),
        ("a {b: color.scale(#66cc99, $whiteness: 0%)}", "#66cc99"),
        ("a {b: color.scale(#66cc99, $whiteness: -100%)}", "#00cc66"),
        (
            "a {b: color.scale(#33cc80, $whiteness: 50%)}",
            "rgb(60%, 80%, 70.0653594771%)",
        ),
        (
            "a {b: color.scale(#66cc99, $whiteness: -50%)}",
            "rgb(20%, 80%, 50%)",
        ),
        (
            "a {b: color.scale(#339966, $blackness: 100%)}",
            "rgb(16.6666666667%, 16.6666666667%, 16.6666666667%)",
        ),
        ("a {b: color.scale(#339966, $blackness: 0%)}", "#339966"),
        ("a {b: color.scale(#339966, $blackness: -100%)}", "#33ff99"),
        (
            "a {b: color.scale(#33cc80, $blackness: 50%)}",
            "rgb(20%, 40%, 30.0653594771%)",
        ),
        (
            "a {b: color.scale(#66cc99, $whiteness: -50%, $blackness: 50%)}",
            "rgb(20%, 40%, 30%)",
        ),
        (
            "a {b: color.scale(rgba(#66cc99, 0.7), $whiteness: -50%, $blackness: 50%)}",
            "rgba(20%, 40%, 30%, 0.7)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The alpha's bounds are `0–1`, and a call naming **nothing else** is a no-op rather than an error
/// (`scale/no_space.hrx`). `rgba(red, 0.5)` with `$alpha: 100%` becomes opaque again — and an opaque colour
/// made from a keyword prints the keyword; `0%` changes nothing; `-100%` empties it. `14%` of the way from
/// 0.5 to 1 is `0.57`, and `-36%` of the way from 0.3 to 0 is `0.192`.
#[tokio::test]
async fn the_alpha_scales_between_zero_and_one_and_nothing_named_is_a_no_op() {
    for (input, want) in [
        ("a {b: color.scale(red)}", "red"),
        ("a {b: color.scale($color: red)}", "red"),
        ("a {b: color.scale(rgba(red, 0.5), $alpha: 100%)}", "red"),
        (
            "a {b: color.scale(rgba(red, 0.5), $alpha: 0%)}",
            "rgba(255, 0, 0, 0.5)",
        ),
        (
            "a {b: color.scale(rgba(red, 0.5), $alpha: -100%)}",
            "rgba(255, 0, 0, 0)",
        ),
        (
            "a {b: color.scale(rgba(red, 0.5), $alpha: 14%)}",
            "rgba(255, 0, 0, 0.57)",
        ),
        (
            "a {b: color.scale(rgba(red, 0.3), $alpha: -36%)}",
            "rgba(255, 0, 0, 0.192)",
        ),
        // The legacy global spelling is the same function (`scale/global.hrx::legacy`): pink's blue 203
        // moved 20% of the 52 left below 255.
        (
            "a {b: scale-color(pink, $blue: 20%)}",
            "rgb(100%, 75.2941176471%, 83.6862745098%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A boundary of **this slice**, not a corpus row: no `scale/*.hrx` names `$hue:`, and an angle has no bound
/// to scale toward, so borrowing `adjust`'s wrap (`adjust/hsl.hrx::hue/arg_above_max` reads 540 as 180) would
/// be inventing the law. The refusal keeps the call's text, which is the decline law already in force —
/// pinned here so a later change cannot quietly start answering it from a neighbour's rule.
#[tokio::test]
async fn a_scale_that_names_the_hue_is_refused_rather_than_invented() {
    for call in [
        "color.scale(red, $hue: 10%)",
        // `$space:` asks for a space this compiler has no conversion for (`scale/no_channels.hrx::identical`
        // is the oklch row, whose own expectation the text does not match — that row stays red on purpose).
        "color.scale(oklch(50% 0.2 0deg), $space: lab)",
        // A modern colour written as text is not read into sRGB (`scale/out_of_gamut.hrx::above_gamut/up`).
        "color.scale(color(srgb 1.2 0.5 0.7), $red: 10%)",
        // A `none` channel is not a number this slice can move (`scale/no_channels.hrx::missing`).
        "color.scale(rgb(none none none))",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "a call outside this slice's scope must keep its text: {call}"
        );
    }
}
