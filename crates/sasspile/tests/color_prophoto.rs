//! The law this file keeps: **`prophoto-rgb`'s primaries are the archive's, and they already carry their own
//! white point** — so nothing adapts this space a second time.
//!
//! `Space::white()` used to answer D50 for `prophoto-rgb` while its table was the transcribed one, so a
//! conversion through this space adapted twice relative to what the corpus does. Fitting the table under the
//! surviving curve hypothesis (`linear = printed^1.8`, which `color/matrix.rs` checks against
//! `to_space/prophoto_rgb/xyz.hrx` — see `tasks.md` 9.18 for the numbers and the segment caveat) lands on the
//! D65 white by itself: `to_space/prophoto_rgb/xyz.hrx::white` states
//! `color(xyz 0.9504559271 1 1.0890577508)`, and the composed pair used to answer
//! `color(xyz 0.9845637886 0.9989843031 1.089497388)` — a `3.4e-2` miss in X on a row the archive reads at the
//! tenth decimal. (The `4.2e-2` figure belongs to the `[0][0]` entry itself, not to this row; the two are
//! different objects and were conflated in an earlier draft of this file.)
//!
//! Every one of the nine entries missed the transcription by more than its own two-direction spread, so the
//! whole table is the archive's now; `white()` therefore lists only `xyz-d50`, `lab` and `lch` as D50, and the
//! reference white of the prophoto pigment lives inside the matrix rather than in a second declaration.
//!
//! Not pinned, with the reason: `to_space/xyz/prophoto_rgb.hrx::white`, whose answer is one unit off in two
//! channels (ours `1.0429146547 0.9845782986`, the archive's `1.0429146548 0.9845782985`) — the tenth-decimal
//! ceiling named in `tasks.md` 9.14/9.16, not a new defect. Rows that go to or from `rec2020` are also left
//! out: that space's table is still the transcribed one, and all ten of this batch's `farther` rows involve
//! it.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// `prophoto-rgb → xyz`. `white` is the row that fixes the white point (a curve maps 1 to 1, so what comes
/// out is the table's row sums and nothing else); `middle` and `out_of_range/near` are what pins the
/// individual entries, including the negative channel that the signed curve has to carry through.
#[tokio::test]
async fn prophoto_rgb_reaches_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("1 1 1", "0.9504559271 1 1.0890577508"),
        ("0.2 0.4 0.8", "0.1183353555 0.1633232963 0.7325727364"),
        ("-1 0.4 2", "-0.4478815578 -0.0732156915 3.8173184875"),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(prophoto-rgb {input}), xyz)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz {printed})")),
            "for `color(prophoto-rgb {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}
