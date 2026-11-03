//! The law this file keeps: **`a98-rgb`'s channels are a `563/256` gamma away from linear light, on the
//! side the archive puts them.**
//!
//! `color/matrix.rs` decoded with `1/2.19921` and encoded with `2.19921` — the inverse of what the corpus
//! computes — and rounded the gamma to boot. Neither showed up in the readings that motivated them, because
//! reciprocal exponents look symmetric: a curve fixes 0 and 1, so only a row converted through the pair tells
//! them apart. What separates the candidates is the residual of a fit under each —
//! `cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz <exponent>`, which now exits non-zero
//! when the hypothesis fails — recorded once with its numbers in `openspec/…/tasks.md` 9.17. The same probe
//! on `prophoto-rgb` says its direction was already right, so this is one arm's mistake, not a convention's.
//!
//! The table beside the curve was fitted too, under the surviving hypothesis: the transcription was kept only
//! where it agrees with the fit within that entry's own two-direction spread, and the Z output row is not such
//! an entry — which is why the old table failed the archive's own white invariant,
//! `to_space/a98_rgb/xyz.hrx::white` = `color(xyz 0.9504559271 1 1.0890577508)`.
//!
//! What this file does **not** pin: `to_space/a98_rgb/xyz.hrx::middle` and `to_space/xyz/a98_rgb.hrx::middle`,
//! each red by one unit in the tenth decimal (the fit's residual sits at `5.0139e-11` on exactly that row),
//! and every row whose remaining error is another space's table — those are held by their own red rows and
//! by `tasks.md`, not by copying my answer into an assertion.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// The two rows that together fix curve *and* table. `white` is curve-free (a power maps 1 to 1), so it
/// states the row sums; `out_of_range/near` has mixed-sign channels `(-1, 0.4, 2)`, which only land on the
/// archive's numbers if the gamma sits on the decode side — with the former inverted exponent this compiler
/// answered that row `color(xyz -0.1947713003 0.2194350107 1.3255759776)` (stored in run
/// `display-p3-solved-final`) where the corpus prints `color(xyz 0.3124701203 0.1320464482 4.5349169732)`.
#[tokio::test]
async fn a98_rgb_reaches_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("a98-rgb 1 1 1", "color(xyz 0.9504559271 1 1.0890577508)"),
        (
            "a98-rgb -1 0.4 2",
            "color(xyz 0.3124701203 0.1320464482 4.5349169732)",
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

/// The other direction, which uses the same curve and the computed inverse of the same table.
#[tokio::test]
async fn xyz_reaches_a98_rgb_on_the_archives_own_numbers() {
    let built = css("a {b: color.to-space(color(xyz 1 1 1), a98-rgb)}").await;
    assert!(
        built.contains("color(a98-rgb 1.0579328451 0.9761412908 0.9581457442)"),
        "`to_space/xyz/a98_rgb.hrx::white` states those channels, this compiler answered:\n{built}"
    );
}
