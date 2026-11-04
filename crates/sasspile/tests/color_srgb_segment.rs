//! The law this file keeps: **`srgb`'s transfer has a linear segment near black, and the corpus says so.**
//!
//! This is the opposite answer from the neighbouring spaces, which is why each was measured separately
//! (`tasks.md` 9.21 has all three readings): removing `rec2020`'s `4.5×` arms won 41 more rows, while removing
//! these `12.92` arms lost 156 — `ledger diff rec2020-audit-closure srgb-no-linear-arm` printed
//! `regress=156 net=0`, and `ledger shift` classified the moved rows `farther=109 closer=9`. So the arms are
//! not a stylistic inheritance from prose about the space; they are what the archive computes.
//!
//! The rows that decide it are the ones that reach the segment at all — conversions of black, where every
//! channel is `0` and the power branch is not what carries the value:
//! `to_space/hwb/srgb_linear.hrx::black` answers `color(srgb-linear 0.0008338051 0.0008338051 0.0008338051)`
//! without the arms, where the archive prints `color(srgb-linear 0 0 0)`, and
//! `to_space/srgb/xyz_d50.hrx::black` answers `color(xyz-d50 0.0008040347 0.0008338051 0.0006879764)` where
//! the archive prints `color(xyz-d50 0 0 0)`.
//!
//! What this file does **not** pin, deliberately: the **threshold** of the segment (`0.04045` on the printed
//! side, `0.0031308` on the linear one). Removing the arms tests that a segment exists; it says nothing about
//! where it ends, and the rows this instrument fits never enter that region —
//! `cargo run -q -p sasspile --example calibrate_space -- srgb xyz_d50 2.4 1.055` reports
//! `smallest_curved_printed_channel = 0.1`, so an archive row could only bound the threshold from above.
//! Moving the threshold is also not something the instrument can currently A/B (it can linearise under a power
//! and an offset, not under a segment), so those two numbers stay a transcription — and are stated as such in
//! `color/matrix.rs`, not asserted here.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// The two `black` rows, with the input line taken verbatim from each case's `input.scss` and the expected
/// line from its `output.css`. Both pass only while the linear arms are in place.
#[tokio::test]
async fn srgb_keeps_the_linear_segment_the_archive_computes() {
    let built = css("a {b: color.to-space(hwb(0deg 0% 100%), srgb-linear)}").await;
    assert!(
        built.contains("color(srgb-linear 0 0 0)"),
        "`to_space/hwb/srgb_linear.hrx::black` prints `color(srgb-linear 0 0 0)`; without the `12.92` arm this \
         compiler answered:\n{built}"
    );

    let built = css("a {b: color.to-space(color(srgb 0 0 0), xyz-d50)}").await;
    assert!(
        built.contains("color(xyz-d50 0 0 0)"),
        "`to_space/srgb/xyz_d50.hrx::black` prints `color(xyz-d50 0 0 0)`; without the arm this compiler \
         answered:\n{built}"
    );
}
