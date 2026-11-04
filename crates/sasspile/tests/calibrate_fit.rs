//! The instrument's own mathematics, tested without the archive.
//!
//! `tests/common/fit.rs` was split out of `examples/calibrate_space.rs` for the line budget, and its header
//! claimed the split would let the maths be tested on its own — a claim that is only true once something
//! includes it in a test target, which nothing did. This file is that something. It matters because every
//! fitted digit this repository has pasted into `color/primaries.rs` came out of these functions: if `solve`
//! or `verdict` were quietly wrong, the "measured, not transcribed" argument for those tables would be empty.
//!
//! Both directions are covered deliberately: a fit that should be accepted, and one that should be refused.
//! A tool that only ever says `true` cannot tell a refuted hypothesis from an untested one, which is the exact
//! failure `tasks.md` 9.17 recorded for the version of this instrument that printed a plausible table and
//! exited zero for a wrong curve.

// Both modules are included whole, so the items only `examples/calibrate_space.rs` reaches (the archive
// readers, `smallest_linear`, a `Side::To` fit, a hypothesis's exponent) are unreachable **in this target**.
// Dead-code analysis is per compilation unit, not per repository; the alternative — a second copy of each
// module here — is the duplicate-truth mistake this repository has already been bitten by twice.
#[allow(dead_code)]
#[path = "common/fit.rs"]
mod fit;
#[allow(dead_code)]
#[path = "common/hrx_read.rs"]
mod hrx_read;

use crate::fit::{apply, invert, residual, smallest_curved_printed, solve, verdict};
use crate::hrx_read::{Hypothesis, Reading, Side};

/// A row of the synthetic archive: name, three inputs, three outputs.
fn row(case: &str, input: [f64; 3], output: [f64; 3]) -> Reading {
    Reading {
        case: case.into(),
        input,
        output,
        printed_input: input,
        printed_output: output,
    }
}

/// The rows `solve` reads, from a matrix chosen here so the answer is known: `white` (`1,1,1`) states the row
/// sums, `out_of_range/far` states one column outright, and `out_of_range/near` separates the remaining two.
fn skeleton(table: &[[f64; 3]; 3]) -> Vec<Reading> {
    [
        ("white", [1.0, 1.0, 1.0]),
        ("out_of_range/near", [-1.0, 0.4, 2.0]),
        ("out_of_range/far", [-999_999.0, 0.0, 0.0]),
    ]
    .into_iter()
    .map(|(case, input)| row(case, input, apply(table, input)))
    .collect()
}

const TESTED: [[f64; 3]; 3] = [[0.4, 0.3, 0.3], [0.2, 0.7, 0.1], [0.0, 0.1, 0.9]];

#[test]
fn a_relation_that_is_the_tested_matrix_solves_back_to_it() {
    let rows = skeleton(&TESTED);
    let solved = solve(&rows);
    // The rows are generated at full precision, so the fit is exact to floating-point rounding rather than to
    // the archive's tenth decimal - which is what makes `verdict` below meaningful on this input.
    (0..3).for_each(|r| {
        (0..3).for_each(|c| {
            assert!(
                (solved[r][c] - TESTED[r][c]).abs() < 1e-12,
                "entry ({r},{c}): solved {:?}, built {:?}",
                solved[r][c],
                TESTED[r][c]
            );
        })
    });
    assert!(verdict(&solved, &rows).0, "a correct fit must be accepted");
}

#[test]
fn a_hypothesis_that_does_not_describe_the_rows_is_refuted_not_reported() {
    // Rows built from one matrix, fitted under a claim about a different one: the same instrument that
    // accepts the fit above must call this false, and name the worst row while doing so.
    let rows = skeleton(&TESTED);
    let other = [[0.9, 0.05, 0.05], [0.1, 0.8, 0.1], [0.0, 0.2, 0.8]];
    let fitted = solve(&rows);
    let (holds, worst) = verdict(&other, &rows);
    assert!(
        !holds,
        "a matrix that is not the rows' relation must be refused"
    );
    assert!(
        worst > 1.0,
        "its ratio must exceed the resolution bound, got {worst}"
    );
    // And the control on the other side: the true relation passes the same call.
    assert!(
        verdict(&fitted, &rows).0,
        "the accepted fit must not be refused by the same check"
    );
    let (case, absolute, ..) = residual(&other, &rows);
    assert_ne!(case, "<no rows>", "a refuted fit still names its worst row");
    assert!(
        absolute > 1e-3,
        "the miss it reports should be large, got {absolute}"
    );
}

#[test]
fn the_inverse_direction_of_a_fit_agrees_with_the_forward_one() {
    // Why the instrument prints a per-entry spread at all: two independent readings of one relation are the
    // only way to know how much a single digit can move, and that number is the bar for overriding a
    // transcribed digit (`tasks.md` 9.16). Here both readings come from exact arithmetic, so they must agree.
    let rows = skeleton(&TESTED);
    let forward = solve(&rows);
    let back = invert(&forward);
    let unit = apply(&back, apply(&forward, [1.0, 2.0, 3.0]));
    (0..3).for_each(|index| {
        assert!(
            (unit[index] - [1.0, 2.0, 3.0][index]).abs() < 1e-12,
            "round-tripping through the inverse drifted: {unit:?}"
        );
    });
}

/// The curved-side probe, on rows that are not read off the archive: it must ignore the linear side entirely,
/// report nothing when no side is curved, and answer for the side named — not for whichever channel happens to
/// be smallest across both (`tasks.md` 9.18 recorded a version that pooled them and read like coverage).
#[test]
fn the_smallest_channel_probe_follows_the_side_that_carries_the_curve() {
    let rows = [row(
        "white",
        [0.25, 0.5, 1.0],
        [0.002_961_599_999_999_999_7, 0.2, 0.8],
    )];
    let curved_input = Some(Hypothesis {
        side: Side::From,
        exponent: 2.4,
        offset: None,
    });
    assert_eq!(
        smallest_curved_printed(&rows, curved_input),
        Some(0.25),
        "the curved side's own printed minimum, not the other side's 0.00296"
    );
    assert_eq!(
        smallest_curved_printed(&rows, None),
        None,
        "no curve, no segment"
    );
}
