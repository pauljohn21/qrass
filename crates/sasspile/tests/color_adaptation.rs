//! The law this file keeps: **the white-point adaptation is the archive's own matrix, not a transcription.**
//!
//! `color/matrix.rs` used to carry the Bradford table as copied from the CSS Color 4 prose
//! (`0.9555766 -0.0230393 0.0631636 …`). The corpus does not compute with it: the archive's D50→D65 row sums
//! are `0.9956342097 1.0026671299 1.3221722918` (`to_space/xyz_d50/xyz.hrx::white`) while the copied table
//! summed to `0.9957009 0.9993688 1.3289869` — worst entry off by `6.9e-3` (`1.3303659262` against
//! `1.3372652`). A neutral barely noticed; anything reaching D50 did, which is every `lab`, `lch` and
//! `xyz-d50` conversion. The replacement was **solved out of the archive** by
//! `cargo run -q -p sasspile --example calibrate_space`, which reads `to_space/xyz_d50/xyz.hrx` from disk,
//! takes the first column off `out_of_range/far` (a `-999999` input makes that entry knowable to `5e-17`)
//! and the other two off the `white` + `out_of_range/near` pair (determinant `1.6`), then reports the worst
//! residual over the rows the solve did not use: `5.0e-11`, exactly the half-step of the archive's tenth
//! decimal place.
//!
//! What this file deliberately does **not** pin, and the measured reason: only one direction is written
//! down in the library, so the derived side of `to_space/xyz/xyz_d50.hrx` keeps two rows red — `gray`, the
//! archive's `0.5014942182` against our `0.5014942183` (`1e-10` absolute), and `out_of_range/far`, whose
//! worst channel is `−29627.7791422469` against our `−29627.7791449956` (`2.75e-6` absolute, `9.3e-11`
//! relative). `to_space/srgb/xyz_d50.hrx::white` is red the same way: the archive prints
//! `color(xyz-d50 0.9642956764 1 0.8251046025)` and we print `0.9642956765`. All three are one unit in the
//! tenth decimal, which is where a table fitted to ten-decimal rows can be expected to sit; they are named
//! in `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md` rather than copied into an
//! assertion, because pinning my own answer would freeze it as a requirement.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// The direction the table is fitted to: `to_space/xyz_d50/xyz.hrx`. Every concrete row of that file is
/// asserted here — the fit used three of them, so the rest are independent evidence, and a re-transcribed
/// digit fails on the tenth decimal. The `missing/*` rows of that file are absent: they ask for a second
/// capability (a `none` that survives a conversion), not for these numbers.
#[tokio::test]
async fn xyz_d50_converts_to_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("0 0 0", "0 0 0"),
        ("1 1 1", "0.9956342097 1.0026671299 1.3221722918"),
        ("0.5 0.5 0.5", "0.4978171049 0.501333565 0.6610861459"),
        ("0.2 0.4 0.8", "0.2324626969 0.4151573703 1.0585524842"),
        ("-1 0.4 2", "-0.8381943171 0.474450751 2.6402147779"),
        (
            "-999999 0 0",
            "-955472.4660146538 28369.6809641542 -12314.0025504671",
        ),
        (
            "0.1 0.2 0.3 / 0.4",
            "0.1099054241 0.205474541 0.3962396495 / 0.4",
        ),
        (
            "0.1 0.2 0.3 / 0.0",
            "0.1099054241 0.205474541 0.3962396495 / 0",
        ),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(xyz-d50 {input}), xyz)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz {printed})")),
            "for `color(xyz-d50 {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The direction the library derives (`invert` of the table above), read through
/// `to_space/xyz/xyz_d50.hrx`. Six of that file's eight concrete rows are reproducible at the archive's
/// resolution; `gray` and `out_of_range/far` are held by the red rows themselves, for the reason stated at
/// the top of this file.
#[tokio::test]
async fn xyz_converts_to_xyz_d50_through_the_computed_inverse() {
    let cases = [
        ("0 0 0", "0 0 0"),
        ("1 1 1", "1.0206843969 1.0029884365 0.7576864323"),
        ("0.2 0.4 0.8", "0.1786108937 0.3884402932 0.6056728936"),
        ("-1 0.4 2", "-1.1391355769 0.3323983638 1.5190136801"),
        (
            "0.1 0.2 0.3 / 0.4",
            "0.0943246735 0.1959275265 0.2276490187 / 0.4",
        ),
        (
            "0.1 0.2 0.3 / 0.0",
            "0.0943246735 0.1959275265 0.2276490187 / 0",
        ),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(xyz {input}), xyz-d50)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz-d50 {printed})")),
            "for `color(xyz {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The row that shows the transcription was wrong rather than merely imprecise, and the one measured while
/// the old digits were still in place: with the CSS Color 4 table, `color(xyz-d50 1 1 1)` answered
/// `color(xyz 0.9957009 0.9993688 1.3289869)` — three channels all off, the worst by `6.8e-3` — while the
/// archive's `to_space/xyz_d50/xyz.hrx::white` states
/// `color(xyz 0.9956342097 1.0026671299 1.3221722918)`.
#[tokio::test]
async fn the_white_row_is_what_a_transcription_used_to_miss() {
    let built = css("a {b: color.to-space(color(xyz-d50 1 1 1), xyz)}").await;
    assert!(
        built.contains("color(xyz 0.9956342097 1.0026671299 1.3221722918)"),
        "the archive's white row is three channels the copied table got wrong:\n{built}"
    );
}

/// The invariant the two tables have to hold **together**, and the reason the D50 white in
/// `color/matrix.rs` is derived instead of copied: a neutral converted to `lab` must have `a = b = 0` and
/// `L = 100`, digit for digit (`to_space/srgb/lab.hrx::white`). With the transcribed white this compiler
/// answered `lab(100.0000000018% 0.0000001274 0.0000000081)` — the `8.3e-10` mismatch between that literal
/// and the triple the adaptation actually lands on, multiplied by the `500 · (fx − fy)` in `xyz_to_lab`.
/// Deriving it makes the equality structural, so a second white literal cannot come back.
#[tokio::test]
async fn a_neutral_converts_to_a_neutral_lab() {
    let built = css("a {b: color.to-space(color(srgb 1 1 1), lab)}").await;
    assert!(
        built.contains("lab(100% 0 0)"),
        "the archive states `lab(100% 0 0)` for its own white; this compiler answered:\n{built}"
    );
    let grey = css("a {b: color.to-space(color(srgb 0.5 0.5 0.5), lab)}").await;
    assert!(
        grey.contains("lab(53.3889647411% 0 0)"),
        "`to_space/srgb/lab.hrx::gray` states `lab(53.3889647411% 0 0)`; this compiler answered:\n{grey}"
    );
}
