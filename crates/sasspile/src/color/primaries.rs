//! The **primaries tables** — each RGB-like space's linear-light channels to XYZ, as the archive computes
//! them. Split out of [`super::space`] because that file had grown past this repository's 500-line limit on
//! the strength of these tables plus the provenance each one now carries: a number that was fitted rather than
//! copied needs saying next to the number, and the place for that saying is here, not in the value type.
//!
//! Two rules govern what is written down:
//! * **One provenance per table.** A fitted table names the command that produced it, and the readings behind
//!   the decision live in `openspec/…/tasks.md` for that batch — because the same numbers copied into a second
//!   file is how this repository keeps acquiring a second answer (and one such copy was already an order of
//!   magnitude off; `tasks.md` 9.17).
//! * **A fit may replace a transcribed digit only where the fit can tell them apart** — see
//!   `tasks.md` 9.16 for the rule and `examples/calibrate_space.rs` for the instrument that prints, per entry,
//!   how far its two independent directions disagree. That spread is the noise floor of a single digit here;
//!   below it, the published digits stay.

use super::Space;

/// The primaries matrix, linear-light of this space to XYZ **at the white the table itself lands on**.
/// `None` for the spaces whose channels already are (or are derived from) XYZ — those answer with their own
/// math in [`super::matrix`], and [`Space::primaries`] is the arm that says which question is well-posed.
pub(super) fn table(space: Space) -> Option<&'static [[f64; 3]; 3]> {
    match space {
        Space::Srgb | Space::SrgbLinear => Some(&super::matrix::SRGB_PRIMARIES),
        Space::DisplayP3 | Space::DisplayP3Linear => Some(&[
            // `display-p3`'s Z-output-row's last two entries are the archive's, read off
            // `cargo run -q -p sasspile --example calibrate_space -- display_p3_linear xyz`; the other seven
            // entries are still the transcribed ones, and that split is a measurement rather than a taste —
            // the instrument fits one relation from both files (`<src>/<dst>.hrx` and `<dst>/<src>.hrx`) and
            // prints, per entry, how far the two fits disagree. For this matrix that noise floor runs
            // `1.8e-11`…`2.6e-10`: `[2][1]` and `[2][2]` miss the transcription by `5.1e-9` and `6.9e-9`
            // (36× and 393× their spread), while `[0][1]` misses by `5.3e-11` against a `2.6e-10` spread, so
            // the fit has no standing to call that digit wrong. Replacing the whole fitted matrix anyway
            // measured worse — `tasks.md` 9.16 has both readings.
            [
                0.486_570_948_648_212,
                0.265_667_693_169_093,
                0.198_217_285_234_362,
            ],
            [
                0.228_974_564_069_775,
                0.691_738_521_836_506,
                0.079_286_914_093_745,
            ],
            [0.0, 0.045_113_381_937_499_92, 1.043_944_368_862_499_9],
        ]),
        Space::A98Rgb => Some(&[
            // Fitted from `to_space/a98_rgb/xyz.hrx` under the curve hypothesis that survived the residual
            // comparison (`linear = printed^(563/256)`); re-measure with
            // `cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz 2.19921875`, and read the
            // candidates and residuals in `tasks.md` 9.17 — that curve was previously on the wrong side of the
            // pair in `super::matrix`, which is the bigger half of what was wrong here.
            //
            // Only `[0][0]` is kept from the transcription: every other entry misses the fit by more than its
            // own two-direction spread, and the Z output row is the worst of them. That is why the old table
            // failed the archive's own white invariant — `to_space/a98_rgb/xyz.hrx::white` states
            // `color(xyz 0.9504559271 1 1.0890577508)`, which this row now reproduces and previously answered
            // `color(xyz 0.9521708788 1.0000000001 1.0506135487)` (run `display-p3-solved-final`).
            [
                0.576_669_042_910_13,
                0.185_558_237_964_962_82,
                0.188_228_646_224_906_43,
            ],
            [
                0.297_344_975_250_536_2,
                0.627_363_566_245_409_3,
                0.075_291_458_504_054_44,
            ],
            [
                0.027_031_361_386_412_37,
                0.070_688_852_583_802_32,
                0.991_337_536_829_785_1,
            ],
        ]),
        Space::ProphotoRgb => Some(&[
            // Fitted from `to_space/prophoto_rgb/xyz.hrx` under `linear = printed^1.8`: re-measure with
            // `cargo run -q -p sasspile --example calibrate_space -- prophoto_rgb xyz 1.8` and read the
            // caveats in `tasks.md` 9.18 — in particular that the archive's prophoto channels never drop below
            // `0.1`, so this space's *linear segment* (`|channel| < 1/16`) is exercised by no row the fit saw.
            //
            // All nine entries missed the transcription by more than their own spreads, so the table is the
            // archive's whole: worst `[0][0]`, a `4.2e-2` miss that printed
            // `color(xyz 0.9845637886 0.9989843031 1.089497388)` for the row the archive states as
            // `color(xyz 0.9504559271 1 1.0890577508)` (`to_space/prophoto_rgb/xyz.hrx::white`). It lands on
            // the D65 white by itself, which is why `Space::white()` no longer adapts this space a second time.
            [
                0.755_590_742_296_920_9,
                0.112_719_842_707_560_62,
                0.082_145_342_095_518_49,
            ],
            [
                0.268_321_843_578_571_9,
                0.715_115_256_675_774_6,
                0.016_562_899_745_653_576,
            ],
            [
                0.003_915_972_762_425_8,
                -0.012_933_442_788_381_523,
                1.098_075_220_825_955_7,
            ],
        ]),
        Space::Rec2020 => Some(&[
            // Still the transcribed table: rec2020's transfer carries an offset (`α = 1.09929682680944`),
            // so a pure-power hypothesis is refused outright by the instrument's verdict rather than fitted and
            // believed — `tasks.md` 9.17 records that measurement (`-- rec2020 xyz 2.2` → 2.4e8× its resolution).
            [
                0.636_958_048_301_291,
                0.144_616_903_586_043,
                0.168_880_975_164_172,
            ],
            [
                0.262_703_970_245_312,
                0.677_998_271_772_319,
                0.059_297_688_361_440,
            ],
            [0.0, 0.028_072_693_049_087, 1.060_985_057_639_950],
        ]),
        _ => None,
    }
}
