//! The pure mathematics of the calibration instrument — split out of `examples/calibrate_space.rs` because
//! that file had reached 491 of this repository's 500 permitted lines, and the split point is one the
//! instrument already drew in its own words: **read the archive** (`hrx_read.rs`), **solve and judge** (this
//! file), **parse arguments and print** (the example). Nothing here touches the filesystem, and nothing here
//! is a claim about any colour space: every constant is either an exponent under test passed in by the caller
//! or a resolution bound derived from how the archive prints.
//!
//! Keeping the maths separate also means it can be tested without running a command; the bound constants in
//! particular are what decides whether a fitted digit may replace a transcribed one, and that decision has
//! been the subject of every correction in `tasks.md` 9.14 through 9.20.

use crate::hrx_read::{Hypothesis, Reading, Side};

/// How far the fit's own noise may sit above the archive's resolution before the hypothesis behind it has to
/// be called refuted. The floor per row is the coarser of the archive's tenth-decimal half-step and the
/// floating-point step of the value it printed; the headroom of `16.0` is that floor multiplied by the
/// **measured** worst entry spread of one fit against its mirror (`2.6e-10` over a `1.6e-11` half-step ≈ 16),
/// which is what a single well-conditioned digit can drift by. A refuted hypothesis exits non-zero, so it
/// cannot be pasted into a table by someone reading only the numbers.
pub const RESOLUTION_HALF_STEP: f64 = 5e-11;
pub const SPREAD_HEADROOM: f64 = 16.0;

/// The first entry of one output row, straight off the `out_of_range/far` row, whose input is
/// `(-999999, 0, 0)` in the archive's own print (and its linearised equivalent when one side is
/// linearised): that single nonzero channel means the printed output **is** the entry times the input, so
/// dividing recovers the entry at the row's full relative precision. The `1e6`-scale input is also why the
/// far row, not `white`, is the tightest constraint on that column: the archive's own miss is a step in its
/// last printed digit, and dividing by `1e6` shrinks it to about `1.2e-16` relative on the entry.
pub fn entry_one(row: &Reading, index: usize) -> f64 {
    assert!(
        row.input[1] == 0.0 && row.input[2] == 0.0,
        "the `out_of_range/far` row is only a column by itself when its other two channels are 0; got {:?}",
        row.input
    );
    row.output[index] / row.input[0]
}

/// The other two entries of one output row, from the `white` and the `out_of_range/near` row. Those two are
/// chosen for their **conditioning**, not for convenience: with the first entry known, `white` states
/// `b + c` (its input is `(1,1,1)` whatever the print, since a curve fixes 0 and 1) and `near` states
/// `n₁·b + n₂·c`, so the two separate only as far as `n₂ − n₁` allows — `1.6` for the archive's
/// `(-1, 0.4, 2)`. The general form is kept because a **linearised** `near` input is no longer that triple.
/// The `(0.1 0.2 0.3)`/`(0.2 0.4 0.8)` pair — the one a first attempt used — has `n₂ − n₁ = 0.04`, and the
/// archive's `5e-11` print rounding divides through by that, putting `1.25e-9` on an entry whose every
/// effect is read at the tenth decimal; the table it produced missed that decimal on rows of order one. Hence
/// the floor below, which rejects `0.04` and keeps `1.6`.
pub fn row_of(white: &Reading, near: &Reading, far: &Reading, index: usize) -> [f64; 3] {
    let first = entry_one(far, index);
    let sum = white.output[index] - first;
    let &[n0, n1, n2] = &near.input;
    let rest = near.output[index] - first * n0;
    let denominator = n2 - n1;
    assert!(
        denominator.abs() >= 1.0,
        "`white` and `out_of_range/near` separate the last two entries by only {denominator}, which would \
         spread the archive's {RESOLUTION_HALF_STEP} print rounding over them by {}× — too ill-conditioned to read a digit from",
        1.0 / denominator.abs()
    );
    let second = (sum * n2 - rest) / denominator;
    let third = (rest - n1 * sum) / denominator;
    [first, second, third]
}

/// The solved matrix, one output row at a time: the archive's `out_of_range/far` gives the first entry,
/// `white` and `out_of_range/near` the other two, and every remaining concrete row is left for
/// [`residual`] to check against.
///
/// Rows are found by **case name** rather than by their literal triples, because a linearised channel is no
/// longer the number printed in the file (a first attempt looked up `[-1.0, 0.4, 2.0]` and panicked on
/// `a98-rgb` for exactly that reason). It is **not** the case that every `to_space/**` file carries these
/// three cases: `to_space/xyz_d50/a98_rgb.hrx` has only `in_range`, `out_of_range` and `missing/*`, and its
/// `in_range` converts between two xyz-d50 colours, so it states no xyz_d50→a98-rgb relation at all. The
/// panic message therefore prints the cases it did find, and a missing one is a wrong pair, not a broken
/// archive.
pub fn solve(rows: &[Reading]) -> [[f64; 3]; 3] {
    let one = |case: &str| {
        rows.iter().find(|row| row.case == case).unwrap_or_else(|| {
            panic!(
                "no `{case}` row in this file; its cases are {:?} — if the file is a different relation, the pair is wrong",
                rows.iter()
                    .map(|row| row.case.as_str())
                    .collect::<Vec<&str>>()
            )
        })
    };
    let (white, near, far) = (
        one("white"),
        one("out_of_range/near"),
        one("out_of_range/far"),
    );
    std::array::from_fn(|index| row_of(white, near, far, index))
}

/// Whether a fit's rows are reproduced within what the archive can express, given the hypothesis it was
/// fitted under. This is a **verdict field** rather than a number to eyeball, because the instrument prints a
/// plausible-looking matrix either way: a wrong exponent on a curved pair (`rec2020` under `2.2`, worst
/// residual `0.19`) and a hypothesis that holds (`a98-rgb` under `563/256`, worst `5.0e-11`) otherwise differ
/// by eight orders of magnitude in the same field name, and nothing in the earlier shape of this output told
/// the reader which one they were holding. The bound per channel is the coarser of the archive's tenth-decimal
/// half-step and the floating-point step of the value it printed, times [`SPREAD_HEADROOM`].
pub fn verdict(matrix: &[[f64; 3]; 3], rows: &[Reading]) -> (bool, f64) {
    rows.iter().fold((true, 0.0_f64), |(holds, worst), row| {
        let (worst_here, holds_here) = apply(matrix, row.input)
            .into_iter()
            .zip(row.output)
            .map(|(predicted, printed)| {
                let floor = RESOLUTION_HALF_STEP.max(printed.abs() * f64::EPSILON * 2.0);
                let ratio = (predicted - printed).abs() / (floor * SPREAD_HEADROOM);
                (ratio, ratio <= 1.0)
            })
            .fold((0.0_f64, true), |seen, here| {
                (seen.0.max(here.0), seen.1 && here.1)
            });
        (holds && holds_here, worst.max(worst_here))
    })
}

/// The smallest nonzero **printed** channel on the curved side only. A piecewise transfer decides its branch on
/// the printed value of its own channels (`prophoto-rgb` against `1/16` in `src/color/matrix.rs`), so
/// this — not a pooled figure over both sides, and not the linearised value — is what says whether the segment
/// branch was exercised by these rows at all. `None` when neither side is curved: there is no segment to have
/// exercised. (A first version pooled both sides and reported prophoto's `0.0276` — that is an *XYZ output*,
/// while the archive's prophoto channels printed as a source never drop below `0.1`.)
pub fn smallest_curved_printed(rows: &[Reading], curved: Option<Hypothesis>) -> Option<f64> {
    let minimum = rows
        .iter()
        .flat_map(move |row| match curved {
            Some(hypothesis) if hypothesis.side == Side::From => row.printed_input,
            Some(_) => row.printed_output,
            None => [0.0; 3],
        })
        .map(f64::abs)
        .filter(|magnitude| *magnitude > 0.0)
        .fold(f64::INFINITY, f64::min);
    (curved.is_some())
        .then_some(minimum)
        .filter(|_| minimum.is_finite())
}

/// The smallest nonzero channel in the linearised space the fit actually solved in. Compared against a
/// segment threshold that lives on the linear side (an encode arm like `1/512` or `0.081`) this is the field
/// that answers; the printed figure does not.
pub fn smallest_linear(rows: &[Reading]) -> f64 {
    rows.iter()
        .flat_map(|row| row.input.into_iter().chain(row.output))
        .map(f64::abs)
        .filter(|magnitude| *magnitude > 0.0)
        .fold(f64::INFINITY, f64::min)
}

/// A 3×3 applied to a vector.
pub fn apply(matrix: &[[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| {
        (0..3)
            .map(|column| matrix[row][column] * vector[column])
            .sum::<f64>()
    })
}

/// The worst `|predicted - printed|` over a set of rows, reported **per channel** together with that same
/// channel's relative miss and printed magnitude. Taking the two maxima independently would let a `1e-5`
/// miss on a `-999999` channel borrow the relative error of a `0.5` channel — a rounding story told as
/// though it were one. A channel the archive printed as `0` has no relative miss to report, so the row is
/// named for its absolute miss and the share is left at `0`: an `inf` in that field reads as a measurement
/// and is not one.
pub fn residual(matrix: &[[f64; 3]; 3], rows: &[Reading]) -> (String, f64, f64, f64) {
    rows.iter()
        .flat_map(|row| {
            apply(matrix, row.input)
                .into_iter()
                .zip(row.output)
                .map(move |(predicted, printed)| {
                    let error = (predicted - printed).abs();
                    let magnitude = printed.abs();
                    (
                        row.case.clone(),
                        error,
                        if magnitude == 0.0 {
                            0.0
                        } else {
                            error / magnitude
                        },
                        magnitude,
                    )
                })
        })
        .fold(
            (String::from("<no rows>"), 0.0, 0.0, 0.0),
            |seen, channel| if channel.1 > seen.1 { channel } else { seen },
        )
}

/// A 3×3 inverse by cofactors over the determinant, so the instrument can line the two directions of one
/// fit up against each other and print their per-entry spread. The library computes its own inverse the
/// same way (`matrix::invert`, private to it); this exists only to compare readings, never to produce a
/// table the library then copies from in the other direction.
pub fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let cofactor = |row: usize, column: usize| -> f64 {
        let rest = |skip: usize| -> Vec<usize> { (0..3).filter(|index| *index != skip).collect() };
        let (rows, columns) = (rest(row), rest(column));
        let minor = m[rows[0]][columns[0]] * m[rows[1]][columns[1]]
            - m[rows[0]][columns[1]] * m[rows[1]][columns[0]];
        let sign = if (row + column) % 2 == 0 { 1.0 } else { -1.0 };
        sign * minor
    };
    let whole = det(*m);
    // Adjugate over the determinant: entry (i, j) is the cofactor of (j, i).
    std::array::from_fn(|i| std::array::from_fn(|j| cofactor(j, i) / whole))
}

/// A determinant of a 3×3 written as rows.
pub fn det(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}
