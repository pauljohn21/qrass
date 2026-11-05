//! The law this file keeps: **a colour that carries its own space is written through that space's channels,
//! named by that space's own vocabulary** — `color.change(color(srgb …), $red:)`, `change/xyz.hrx` writes the
//! same position through `$x`, `change/lab.hrx` through `$a`, `change/lch.hrx` through `$chroma`/`$hue`.
//!
//! Before this there was no such path at all: every `change/<space>.hrx` file was red end to end
//! (measured: `change/srgb.hrx` has 15 cases and 15 were failing in run `change-writes-absence-v4`). The
//! archive fixes four things at once, and each is a row rather than a convention:
//!
//! * **The names belong to the space** — `change/{xyz,lab,lch}.hrx` never spell `red/green/blue`, and
//!   `change/{srgb,display_p3,a98_rgb,prophoto_rgb,rec2020,…}` never spell `x/y/z`.
//! * **A percentage is read per channel, not per space.** `change/lab.hrx::lightness/percent` (`30%` → `30`)
//!   and `change/lab.hrx::a/percent` (`-40%` → `-50`) are the same *kind* of argument on two different bases,
//!   inside one colour; the oklab family puts its axes on `±0.4` (`change/oklab.hrx::a/percent`,
//!   `change/oklch.hrx::chroma/percent`) and `lch` puts its chroma on 150 (`change/lch.hrx::chroma/percent`).
//! * **A hue is an angle and a negative chroma turns the hue over half a circle** — `change/lch.hrx::hue/*`
//!   reads `-20deg` as `340deg` and `400deg` as `40deg`, while `…::chroma/negative` answers
//!   `color.change(lch(50% 20 30deg), $chroma: -10)` with `lch(50% 10 210deg)`; the `oklch` rows agree, so one
//!   fold serves the pair. No other channel folds: `change/{lab,oklab}.hrx::a/out_of_range` keep their
//!   negatives, and nothing clamps — `change/xyz.hrx::x/out_of_range` keeps the `1.2` it was given.
//! * **A unit that is neither `%` nor nothing is refused by name**, and a hue refuses a `%` outright:
//!   `change/error/units/srgb.hrx::red` → `$red: Expected 0.5px to have unit "%" or no units.`,
//!   `change/error/units/lch.hrx::hue` → `$hue: Expected 20% to have an angle unit (deg, grad, rad, turn).`
//!   Reading `0.5px` as the magnitude `0.5` — what this arm's first version did — silently answered 27 rows
//!   the archive calls errors, which is what those two assertions exist to keep out.
//!
//! What this file deliberately does **not** pin, with the measured reason each time:
//!
//! * **The four `change/{lab,lch,oklab,oklch}.hrx::lightness/out_of_range` rows.** For
//!   `color.change(lab(50% 20 -30), $lightness: 120%)` the archive prints
//!   `color-mix(in lab, color(xyz 1.7255148283 1.6190494947 2.5266428703) 100%, black)` — a `color-mix`
//!   *expression*, which this compiler cannot emit at all (recorded in `.qoder/rules/rust.md` as an
//!   independent capability gap). We now print `lab(120% 20 -30)` instead of echoing the call; the row stays
//!   red, and the difference the archive draws between the two spellings is not something these fixtures
//!   adjudicate, so no claim about it is made here.
//! * **The six `*/lightness/none` and `{lch,oklch}::hue/none` rows.** `$lightness` and `$hue` are also names
//!   of the *legacy* axes, and `route`'s pre-read of those axes (which is what raises the corpus's axis
//!   messages) declines a `none` before this arm is reached. Writing absence into an `hsl`/`hwb` axes triple is
//!   the same open item (`tasks.md` 9.25); until that exists, these six keep their text.
//! * **`adjust` and `scale` on a carried space** — deliberately unwired, since their arithmetic (`scale_toward`
//!   toward which bound? `share` of what base?) is a second set of laws these rows do not state. Their files
//!   stay red and are named in `tasks.md` 9.27.
//! * **A name the colour's space does not own** keeps its text rather than answering
//!   `Color space srgb doesn't have a channel with this name.` — the rows that ask for that on a *space*
//!   colour are `change/error/incompatible_channel.hrx::{legacy_channel,modern_both}` —
//!   `color.change(color(srgb 0.2 0.5 0.7), $whiteness: 50%)` and `…, $chroma: 50%` — all three of them red
//!   in run `change-in-carried-space-v5` as they were in the baseline. `change/error/mixed_formats.hrx` — the
//!   file that carries that message for the **legacy** groups — holds no space-colour row
//!   (`grep -c "color(srgb\|lab(\|lch(" → 0`), so this is not that file's law re-used by guess.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

async fn failure(scss: &str) -> String {
    match compile(scss).await {
        Ok(text) => panic!("expected {scss:?} to fail, but it compiled to:\n{text}"),
        Err(error) => error.to_string(),
    }
}

/// Each space is written by its own names, unclamped, and a `none` stays a `none`.
#[tokio::test]
async fn a_carried_space_is_written_through_its_own_channels() {
    let cases = [
        (
            "a {b: color.change(color(srgb 0.2 0.5 0.7), $red: 0.5)}",
            "color(srgb 0.5 0.5 0.7)",
        ),
        (
            "a {b: color.change(color(srgb 0.2 0.5 0.7), $blue: none)}",
            "color(srgb 0.2 0.5 none)",
        ),
        (
            "a {b: color.change(color(srgb 0.2 0.5 0.7), $red: 0.7, $green: 0.4, $blue: 0.2)}",
            "color(srgb 0.7 0.4 0.2)",
        ),
        (
            "a {b: color.change(color(xyz 0.2 0.5 0.7), $x: 1.2)}",
            "color(xyz 1.2 0.5 0.7)",
        ),
        (
            "a {b: color.change(oklab(50% 0.2 -0.3), $a: 1)}",
            "oklab(50% 1 -0.3)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $chroma: none)}",
            "lch(50% none 30deg)",
        ),
        (
            "a {b: color.change(color(xyz-d50 0.2 0.5 0.7), $z: none)}",
            "color(xyz-d50 0.2 0.5 none)",
        ),
        (
            "a {b: color.change(color(rec2020 0.2 0.5 0.7), $red: 0.5, $alpha: 0.9)}",
            "color(rec2020 0.5 0.5 0.7 / 0.9)",
        ),
    ];
    for (input, printed) in cases {
        let built = css(input).await;
        assert!(
            built.contains(printed),
            "{input}: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The percentage base is a property of **one channel**, and one space holds two of them: `lab`'s lightness is
/// on `0–100` and its `a` on `±125`, and the oklab family reads `±0.4`.
#[tokio::test]
async fn a_percentage_is_read_per_channel_and_not_per_space() {
    let cases = [
        (
            "a {b: color.change(lab(50% 20 -30), $lightness: 30%)}",
            "lab(30% 20 -30)",
        ),
        (
            "a {b: color.change(lab(50% 20 -30), $a: -40%)}",
            "lab(50% -50 -30)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $chroma: 40%)}",
            "lch(50% 60 30deg)",
        ),
        (
            "a {b: color.change(oklab(50% 0.2 -0.3), $a: -40%)}",
            "oklab(50% -0.16 -0.3)",
        ),
        (
            "a {b: color.change(oklch(50% 0.2 30deg), $chroma: 40%)}",
            "oklch(50% 0.16 30deg)",
        ),
        (
            "a {b: color.change(color(display-p3-linear 0.2 0.5 0.7), $green: 40%)}",
            "color(display-p3-linear 0.2 0.4 0.7)",
        ),
    ];
    for (input, printed) in cases {
        let built = css(input).await;
        assert!(
            built.contains(printed),
            "{input}: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// A hue arrives as an angle and folds into one turn; a **negative chroma** takes the hue half a circle with
/// it, in both polar spaces.
#[tokio::test]
async fn an_angle_folds_and_a_negative_chroma_turns_the_hue() {
    let cases = [
        (
            "a {b: color.change(lch(50% 20 30deg), $hue: 400deg)}",
            "lch(50% 20 40deg)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $hue: -20deg)}",
            "lch(50% 20 340deg)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $chroma: -10)}",
            "lch(50% 10 210deg)",
        ),
        (
            "a {b: color.change(oklch(50% 0.2 30deg), $chroma: -0.1)}",
            "oklch(50% 0.1 210deg)",
        ),
        (
            "a {b: color.change(oklch(50% 0.2 30deg), $hue: 400deg)}",
            "oklch(50% 0.2 40deg)",
        ),
    ];
    for (input, printed) in cases {
        let built = css(input).await;
        assert!(
            built.contains(printed),
            "{input}: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
    // …and no other channel folds: `lab`'s and `oklab`'s negative axes stay negative.
    let stays = css("a {b: color.change(lab(50% 20 -30), $b: -200)}").await;
    assert!(
        stays.contains("lab(50% 20 -200)"),
        "`change/lab.hrx::b/out_of_range` keeps the negative it was given; got:\n{stays}"
    );
}

/// The two unit refusals the archive states for these calls. Without them the write would answer a `0.5px`
/// with `0.5` — a value 27 rows call an error.
#[tokio::test]
async fn a_wrong_unit_is_refused_by_name() {
    assert_eq!(
        failure("a {b: color.change(color(srgb 0.2 0.5 0.7), $red: 0.5px)}").await,
        "$red: Expected 0.5px to have unit \"%\" or no units."
    );
    assert_eq!(
        failure("a {b: color.change(lch(50% 30 50deg), $hue: 20%)}").await,
        "$hue: Expected 20% to have an angle unit (deg, grad, rad, turn)."
    );
    assert_eq!(
        failure("a {b: color.change(lab(50% 30 -50), $a: 20px)}").await,
        "$a: Expected 20px to have unit \"%\" or no units."
    );
}

/// Why the 44 nested `to_space/{lab,lch,oklab,oklch}/*` rows that moved are not a second arithmetic: writing a
/// lightness and *stating* the same number in the literal must convert to the **same text**. Whatever those rows
/// differ by is the shared oklab/lab route's residue (`tasks.md` 9.14), which the assertion fails the day the
/// two spellings drift apart.
#[tokio::test]
async fn a_written_channel_and_the_same_number_written_out_convert_alike() {
    for (written, literal, target) in [
        (
            "color.change(oklab(0% -2 2), $lightness: -50%)",
            "oklab(-50% -2 2)",
            "srgb",
        ),
        (
            "color.change(oklch(30% 0.1 40deg), $hue: 400deg)",
            "oklch(30% 0.1 40deg)",
            "srgb",
        ),
        (
            "color.change(lab(50% 20 -30), $a: -30)",
            "lab(50% -30 -30)",
            "xyz",
        ),
    ] {
        let changed = css(&format!("a {{b: color.to-space({written}, {target})}}")).await;
        let literal = css(&format!("a {{b: color.to-space({literal}, {target})}}")).await;
        let value = |text: &str| {
            text.lines()
                .find(|line| line.contains("b:"))
                .unwrap_or(text)
                .trim()
                .to_string()
        };
        assert_eq!(
            value(&changed),
            value(&literal),
            "`{written}` and the colour `{literal}` name the same channels, so `{target}` cannot answer them \
             differently"
        );
    }
}
