//! The law this file keeps: **`rec2020`'s channels are a plain signed gamma of `2.4` away from linear light —
//! no offset, and no linear segment.**
//!
//! `color/matrix.rs` carried a shifted power, `((v + (1.09929682680944 - 1)) / 1.09929682680944) ^ (1/0.45)`,
//! on both arms of that transfer. Neither number is what the corpus computes, and the shape was not mine to
//! guess: `cargo run -q -p sasspile --example calibrate_space -- rec2020 xyz <exponent> <alpha>` fits the
//! primaries under a stated curve hypothesis and reports the worst residual over the archive's own rows,
//! exiting non-zero when the hypothesis fails. `2.4 / 1.0` lands at `4.914e-11` (the half-step of the
//! corpus's tenth decimal); `1/0.45 / 1.09929682680944` — the former law — is off by `0.214`, and every
//! neighbour sits between `0.019` and `0.29`, eight to nine orders above that. `tasks.md` 9.20 holds the scan,
//! and the two A/B readings that separate the `4.5×` arms (`net=85` with them, `net=126` without) from the
//! fitted row 2 of the table (`net=126` before it, `net=141` after) - different objects, different pairs.
//!
//! The table beside the curve needed the same treatment, and two independent readings of the archive pointed
//! at the same row before any digit was touched: `to_space/rec2020/xyz.hrx::white` multiplies `(1,1,1)`, which
//! no transfer can move, so its three printed numbers **are** the row sums. The transcribed row 1 summed to
//! `0.9999999304` where the corpus states `1` — `7e-8`, impossible as a rounding story.
//!
//! What this file does **not** pin: `to_space/rec2020/xyz.hrx::out_of_range/far`, red on the `-999999`
//! magnitudes whose last printed digit the fit cannot resolve; and `to_space/rec2020/lab.hrx::white`, whose
//! got is `lab(100% -0.0000000002 -0.0000000025)` — **the same numbers `to_space/display_p3/lab.hrx::white`
//! gets**, which is why this batch does not claim to know which space owns that error (the attribution is
//! un-isolated; `tasks.md` 9.20 lists it as open). And the sixteen rows that the intermediate variant
//! (transcribed row 2) had won back: each misses by one unit in the tenth decimal, and they sit on **both**
//! sides of the conversion — `to_space/rec2020/{srgb,display_p3,…}::*` read the forward matrix, while
//! `to_space/*/rec2020::*` read the computed inverse. One table, one side; the side matching more of the
//! archive is kept, and the loss is named rather than hidden.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// `rec2020 → xyz`. `white` is the curve-free row (it fixes the row sums); `middle` is the one that separates
/// `2.4` from `1/0.45` outright, since mid-range channels are where a wrong exponent stops being a rounding
/// matter.
#[tokio::test]
async fn rec2020_reaches_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("rec2020 1 1 1", "color(xyz 0.9504559271 1 1.0890577508)"),
        (
            "rec2020 0.2 0.4 0.8",
            "color(xyz 0.1282769355 0.1154243414 0.6241614492)",
        ),
    ];
    for (source, printed) in cases {
        let built = css(&format!("a {{b: color.to-space(color({source}), xyz)}}")).await;
        assert!(
            built.contains(printed),
            "for `color({source})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The other direction, which goes through the computed inverse of the same table and the encode arm of the
/// same curve. A channel like `0.3678067823` is on neither segment of any plausible toe, so this row is the
/// encode side's equivalent of `middle`.
#[tokio::test]
async fn srgb_reaches_rec2020_on_the_archives_own_numbers() {
    let built = css("a {b: color.to-space(color(srgb 0.2 0.4 0.8), rec2020)}").await;
    assert!(
        built.contains("color(rec2020 0.3678067823 0.4291867351 0.7812845626)"),
        "`to_space/srgb/rec2020.hrx::middle` states those channels, this compiler answered:\n{built}"
    );
}
