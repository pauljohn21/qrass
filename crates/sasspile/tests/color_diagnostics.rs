//! The messages `color.change` / `color.adjust` / `color.scale` give when an argument **cannot be used**,
//! pinned on `core_functions/color/{change,adjust,scale}/error/{args,type,units,bounds}.hrx` and
//! `core_functions/color/saturate.hrx::error/too_few_args`. Every input and every message below is copied
//! from the fixture's own `input.scss` and `error` section.
//!
//! The point of this file is that the three families do **not** share one wording, and inventing a single
//! generic message would answer a row the corpus does not state:
//! * `change` also accepts an unquoted `none` for a channel — it means a *missing* channel — so its type
//!   message says `is not a number or unquoted "none".`, and an unquoted `none` itself is refused quietly
//!   (`change/hwb.hrx::whiteness/none` expects the output `hwb(180deg none 50%)`, not an error; that decline
//!   is pinned in `tests/color_change.rs`);
//! * `adjust` and `scale` say `is not a number.` even for `none`;
//! * only a **scale** demands `%` on every amount and checks the share against `-100%…100%`.

use sasspile::compile;

/// The message a call is expected to fail with — the harness compares the same first line of the corpus's
/// `error` section (`tests/hrx.rs::oracle_header`). A call that **compiles** is the failure here.
async fn failure(input: &str) -> String {
    match compile(input).await {
        Ok(css) => panic!("{input} was expected to fail, but it compiled to:{css}"),
        Err(error) => error.message,
    }
}

/// `$color: 1 is not a color.` is the message for a **number** in the colour slot, and all three families
/// say it the same way (`{change,adjust,scale}/error/type.hrx::color`). A colour in a space this compiler
/// cannot read gets no message at all — it keeps its text, because answering `color(srgb …)` or `lab(…)` in
/// sRGB would be a wrong value.
#[tokio::test]
async fn a_number_in_the_colour_slot_is_not_a_colour() {
    for input in [
        "a {b: color.change(1)}",
        "a {b: color.adjust(1)}",
        "a {b: color.scale(1)}",
    ] {
        assert_eq!(
            failure(input).await,
            "$color: 1 is not a color.",
            "for {input}"
        );
    }
}

/// The type message, per family: `change` names the unquoted `none` it also takes
/// (`change/error/type.hrx::{red,alpha,quoted_none}`), while `adjust`/`scale` do not and reject even that
/// word (`adjust/error/type.hrx::none` is `color.adjust(red, $alpha: none)` →
/// `Error: $alpha: none is not a number.`, and `scale/error/type.hrx::none` is the same for a scale).
#[tokio::test]
async fn an_amount_that_is_not_a_number_uses_its_family_wording() {
    for (input, want) in [
        (
            "a {b: color.change(red, $red: c)}",
            "$red: c is not a number or unquoted \"none\".",
        ),
        (
            "a {b: color.change(red, $alpha: \"none\")}",
            "$alpha: \"none\" is not a number or unquoted \"none\".",
        ),
        (
            "a {b: color.change(red, $saturation: c)}",
            "$saturation: c is not a number or unquoted \"none\".",
        ),
        (
            "a {b: color.adjust(red, $red: c)}",
            "$red: c is not a number.",
        ),
        (
            "a {b: color.adjust(red, $alpha: none)}",
            "$alpha: none is not a number.",
        ),
        (
            "a {b: color.scale(red, $red: c)}",
            "$red: c is not a number.",
        ),
        (
            "a {b: color.scale(red, $alpha: none)}",
            "$alpha: none is not a number.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// A scale's amount is a **share**, so only a percentage can carry it: `color.scale(white, $red: 1)` and
/// `color.scale(white, $red: 1px)` are refused with `Expected 1 to have unit "%".` and
/// `Expected 1px to have unit "%".` (`scale/error/units/rgb.hrx::{none,wrong}/*`, and the same wording for
/// the axes in `…/units/hsl.hrx` and `…/units/hwb.hrx`).
#[tokio::test]
async fn a_scale_amount_has_to_be_a_percentage() {
    for (input, want) in [
        (
            "a {b: color.scale(white, $red: 1)}",
            "$red: Expected 1 to have unit \"%\".",
        ),
        (
            "a {b: color.scale(white, $red: 1px)}",
            "$red: Expected 1px to have unit \"%\".",
        ),
        (
            "a {b: color.scale(white, $saturation: 1)}",
            "$saturation: Expected 1 to have unit \"%\".",
        ),
        (
            "a {b: color.scale(white, $whiteness: 1px)}",
            "$whiteness: Expected 1px to have unit \"%\".",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// `adjust` and `change` take a unitless number for rgb channels and the axes — but **not** for whiteness
/// and blackness, which are percentages by statement: `color.adjust(white, $whiteness: 1)` and
/// `color.change(black, $blackness: 1)` → `Expected 1 to have unit "%".`
/// (`adjust/error/units/hwb.hrx::none/*`, same file under `change`).
#[tokio::test]
async fn whiteness_and_blackness_are_percentages_for_the_other_two_as_well() {
    for (input, want) in [
        (
            "a {b: color.adjust(white, $whiteness: 1)}",
            "$whiteness: Expected 1 to have unit \"%\".",
        ),
        (
            "a {b: color.adjust(black, $blackness: 1px)}",
            "$blackness: Expected 1px to have unit \"%\".",
        ),
        (
            "a {b: color.change(black, $blackness: 1)}",
            "$blackness: Expected 1 to have unit \"%\".",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// The share itself lies within `-100%` and `100%`: `color.scale(red, $saturation: -100.001%)` →
/// `Error: $saturation: Expected -100.001% to be within -100% and 100%.` and `…/legacy/too_high` is
/// `$whiteness: 100.001%` (`scale/error/bounds.hrx`).
#[tokio::test]
async fn a_scale_share_stays_between_minus_a_hundred_and_a_hundred() {
    for (input, want) in [
        (
            "a {b: color.scale(red, $saturation: -100.001%)}",
            "$saturation: Expected -100.001% to be within -100% and 100%.",
        ),
        (
            "a {b: color.scale(red, $whiteness: 100.001%)}",
            "$whiteness: Expected 100.001% to be within -100% and 100%.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// A hue has no bound to move toward: `color.scale(white, $hue: 10%)` →
/// `Error: $hue: Channel isn't scalable.` (`scale/error/args.hrx::hue`, and the three rows of
/// `scale/error/polar.hrx` say the same). `adjust` does wrap a hue (`adjust/hsl.hrx::hue/arg_above_max`
/// reads 540 as 180), which is why this refusal is per family and not a general rule about angles.
#[tokio::test]
async fn a_hue_is_not_scalable() {
    assert_eq!(
        failure("a {b: color.scale(white, $hue: 10%)}").await,
        "$hue: Channel isn't scalable."
    );
}

/// One colour positionally, every channel by name: `color.change(red, 1)` →
/// `Error: Only one positional argument is allowed. All other arguments must be passed by name.`
/// (`{change,adjust,scale}/error/args.hrx::too_many`).
#[tokio::test]
async fn the_colour_is_the_only_positional_argument() {
    let want =
        "Only one positional argument is allowed. All other arguments must be passed by name.";
    for input in [
        "a {b: color.change(red, 1)}",
        "a {b: color.adjust(red, 1)}",
        "a {b: color.scale(red, 1)}",
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// An empty call names the parameter it lacks, which the registry's generic variadic message could not:
/// `color.change()` → `Missing argument $color.` (`{change,adjust,scale}/error/args.hrx::too_few`), while
/// `saturate()` — whose colour is optional and whose amount is not — → `Missing argument $amount.`
/// (`core_functions/color/saturate.hrx::error/too_few_args`).
#[tokio::test]
async fn a_missing_argument_is_named_by_the_function_that_lacks_it() {
    for input in [
        "a {b: color.change()}",
        "a {b: color.adjust()}",
        "a {b: color.scale()}",
    ] {
        assert_eq!(
            failure(input).await,
            "Missing argument $color.",
            "for input {input}"
        );
    }
    assert_eq!(
        failure("a {b: saturate()}").await,
        "Missing argument $amount.",
        "saturate's colour is optional, its amount is not"
    );
}

/// `$space:` must be a string: `color.scale(red, $space: 1)` → `Error: $space: 1 is not a string.`
/// (`scale/error/type.hrx::space`, and `adjust`/`change` have the same row). A `$space:` that *is* a string
/// is a different case — a space this compiler cannot convert — and keeps the call's text instead
/// (`adjust/error/units/*.hrx` and `…/space.hrx` stay red for that reason, not for this one).
#[tokio::test]
async fn a_space_that_is_not_a_string_is_said() {
    for input in [
        "a {b: color.scale(red, $space: 1)}",
        "a {b: color.adjust(red, $space: 1)}",
        "a {b: color.change(red, $space: 1)}",
    ] {
        assert_eq!(
            failure(input).await,
            "$space: 1 is not a string.",
            "for input {input}"
        );
    }
}
