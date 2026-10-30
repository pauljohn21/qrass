//! The law this file keeps: **the two directions of a colour-space conversion are one table, not two.**
//!
//! `to_space/oklab/oklch.hrx` and `to_space/oklch/oklab.hrx` are the corpus's own round trip — read an
//! `oklab`, print an `oklch`, and back — and thirteen of those rows were red for a reason that no amount of
//! matrix calibration would fix: `color/matrix.rs` carried the forward cone matrix *and* a hand-copied
//! inverse, and the two disagreed. A colour that went out and came back did not come back as it went in.
//! The inverse is now computed from the forward table, so the pair is mutual by construction — whatever
//! digits either table carries, a round trip cannot corrupt the value again.
//!
//! What this file deliberately does **not** pin: the absolute oklab↔XYZ numbers. `to_space/srgb/oklab.hrx`
//! answers `color(srgb 0.5 0.5 0.5)` with `oklab(59.8180730527% 0 0)` and this compiler still prints a
//! non-zero `a` and `b` for it, so the cone table's *digits* are not the corpus's yet — that gap is held by
//! those red rows, and copying a near-miss value here would only freeze my mistake as a requirement.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// One corpus reading, copied verbatim from `to_space/oklab/oklch.hrx::middle`: the lightness survives, the
/// two chromas become a chroma and a hue. This depends on the cube-root matrix and the polar transform only,
/// so it stays true regardless of how the cone table is calibrated.
#[tokio::test]
async fn oklab_reads_as_oklch_the_way_the_corpus_prints_it() {
    let built = css("a {b: color.to-space(oklab(50% 0.2 -0.3), oklch)}").await;
    assert!(
        built.contains("oklch(50% 0.3605551275 303.690067526deg)"),
        "expected the archive's own oklch reading in: {built}"
    );
}

/// The round trip is the invariant that broke. Both halves go through the compiler, so the assertion cannot
/// be satisfied by a value this file invented — and it fails loudly if a second hand-copied inverse table
/// ever returns, whatever digits either table carries.
#[tokio::test]
async fn oklab_to_oklch_and_back_returns_the_colour_it_started_from() {
    let direct = css("a {b: color.to-space(oklab(50% 0.2 -0.3), oklab)}").await;
    let round_trip =
        css("a {b: color.to-space(color.to-space(oklab(50% 0.2 -0.3), oklch), oklab)}").await;
    assert_eq!(direct, round_trip);
}
