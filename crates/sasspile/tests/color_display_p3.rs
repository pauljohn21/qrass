//! The law this file keeps: **`display-p3`'s primaries table is the archive's, and a fit may only replace
//! what it can distinguish.**
//!
//! The blue column's two entries of the Z output row were `5.0778e-9` and `6.8716e-9` off the numbers the
//! corpus computes with, and that pair held 165 rows red. `cargo run -q -p sasspile --example
//! calibrate_space -- display_p3_linear xyz` solves them out of `to_space/display_p3_linear/xyz.hrx`,
//! reproduces its own rows to `5.0e-11` — the half-step of the archive's tenth decimal place — and prints
//! the spread between its two directions, which is the noise floor for a single digit here.
//!
//! The seven entries that already agreed within that noise **kept their published digits**. That is not
//! conservatism, it is measured: pasting the whole fitted matrix instead of only the two distinguishable
//! entries scored `net=136 regress=0`, and the fit taken from the other side scored higher (`net=168`) but
//! broke `regress=0` on `to_space/display_p3_linear/srgb.hrx::out_of_range/far` — whose `-999999` input
//! amplifies the fit's own noise past the tenth decimal — against `regress=0 net=165` for the restricted
//! override. A fit that cannot tell a digit is not evidence that the digit is wrong.
//!
//! Every value below is copied from the named case's `output.css`, and every case here was red before the
//! change and is green after it (`ledger failures missing-rectangular-v2 <key>` vs `… display-p3-two-entries
//! <key>`). Deliberately **not** pinned: `to_space/xyz/display_p3.hrx::gray`, whose third channel is the
//! archive's `0.7071783379` against our `0.7071783378` — the same last-digit ceiling as the three rows
//! `tests/color_adaptation.rs` names, and this table does not reach it.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// XYZ → display-p3: the encoded side, where the blue column's error used to show up in the third channel
/// (`0.9623860263` against the archive's `0.9623860213`).
#[tokio::test]
async fn xyz_converts_to_display_p3_on_the_archives_own_numbers() {
    let cases = [
        ("1 1 1", "1.067061664 0.9807650127 0.9623860213"),
        ("0.2 0.4 0.8", "-0.4800324283 0.7723805169 0.8767615059"),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(xyz {input}), display-p3)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(display-p3 {printed})")),
            "for `color(xyz {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The forward direction of the pair the fit was taken from — the linear side, where no transfer curve sits
/// between the number and the matrix.
#[tokio::test]
async fn display_p3_linear_converts_to_xyz_through_the_fitted_column() {
    let cases = [
        ("1 1 1", "0.9504559271 1 1.0890577508"),
        ("0.5 0.5 0.5", "0.4752279635 0.5 0.5445288754"),
        ("0.2 0.4 0.8", "0.3621550952 0.3859198528 0.8532008479"),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(display-p3-linear {input}), xyz)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz {printed})")),
            "for `color(display-p3-linear {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The encoded source in the same direction, so the curve and the table are checked together rather than
/// one at a time.
#[tokio::test]
async fn display_p3_converts_to_xyz_with_the_curve_applied() {
    let built = css("a {b: color.to-space(color(display-p3 0.2 0.4 0.8), xyz)}").await;
    assert!(
        built.contains("color(xyz 0.171095654 0.1473658922 0.6363562895)"),
        "`to_space/display_p3/xyz.hrx::middle` states those three numbers, this compiler answered:\n{built}"
    );
}
