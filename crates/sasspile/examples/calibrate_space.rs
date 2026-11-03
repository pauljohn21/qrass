//! A calibration instrument for the colour-space tables: it reads the archive's **own** rows for one
//! *linear* pair of spaces and solves the matrix between them, so the digits in
//! [`sasspile`'s tables](../sasspile/src/color/matrix.rs) come from a re-runnable measurement instead of
//! from what I last transcribed.
//!
//! ```text
//! cargo run -q -p sasspile --example calibrate_space -- xyz_d50 xyz        # the white-point adaptation
//! cargo run -q -p sasspile --example calibrate_space -- display_p3_linear xyz
//! cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz 2.19921875
//! ```
//!
//! A pair is admissible when at most one side is printed through a transfer curve, and then only with an
//! explicit **decode exponent** (the third argument, meaning `linear = printed^exponent`): the exponent is a
//! hypothesis under test, the fit runs on linearised readings, and [`verdict`] decides whether it held —
//! a refuted one exits non-zero, because a fitted matrix for a relation that is not linear under that
//! hypothesis prints exactly like evidence and is otherwise indistinguishable from a good one.
//!
//! For an accepted pair it reports:
//! * the matrix fitted from `to_space/<src>/<dst>.hrx`, and its residual against **every** concrete row of
//!   that file, including the rows the fit did not use;
//! * the same relation fitted from `to_space/<dst>/<src>.hrx`, and **per entry how far the two directions
//!   disagree** — that spread is this instrument's noise floor for one digit, which is the number that
//!   decides whether a fit may override a transcribed digit at all;
//!
//! Numbers are read at the archive's resolution, and that resolution is **not** one constant: the corpus
//! prints ten decimal places, so a channel near `1` is known to ±5e-11 while a channel near `-999999` is
//! known only to the floating-point step at that magnitude (`1.2e-16` relative). The residuals below are
//! reported per channel with both the absolute miss and the miss relative to that channel, because the two
//! readings answer different questions and a row that misses by `1e-5` against `-999999` is a rounding
//! story, while the same miss against `0.5` is a defect.

use std::path::Path;

#[path = "../tests/common/hrx_read.rs"]
mod hrx_read;

use crate::hrx_read::{Reading, readings, sections, triple};

/// One archive reading lives in `hrx_read`; what is left here is the fit and its verdicts.

/// The first entry of one output row, straight off the `out_of_range/far` row, whose input is
/// `(-999999, 0, 0)` in the archive's own print (and its linearised equivalent when one side is
/// linearised): that single nonzero channel means the printed output **is** the entry times the input, so
/// dividing recovers the entry at the row's full relative precision. The `1e6`-scale input is also why the
/// far row, not `white`, is the tightest constraint on that column: the archive's own miss is a step in its
/// last printed digit, and dividing by `1e6` shrinks it to about `1.2e-16` relative on the entry.
fn entry_one(row: &Reading, index: usize) -> f64 {
    assert!(
        row.input[1] == 0.0 && row.input[2] == 0.0,
        "the `out_of_range/far` row is only a column by itself when its other two channels are 0; got {:?}",
        row.input
    );
    row.output[index] / row.input[0]
}

/// How far the fit's own noise may sit above the archive's resolution before the hypothesis behind it has to
/// be called refuted. The floor per row is the coarser of the archive's tenth-decimal half-step and the
/// floating-point step of the value it printed; the headroom of `16.0` is that floor multiplied by the
/// **measured** worst entry spread of one fit against its mirror (`2.6e-10` over a `1.6e-11` half-step ≈ 16),
/// which is what a single well-conditioned digit can drift by. A refuted hypothesis exits non-zero, so it
/// cannot be pasted into a table by someone reading only the numbers.
const RESOLUTION_HALF_STEP: f64 = 5e-11;
const SPREAD_HEADROOM: f64 = 16.0;

/// The other two entries of one output row, from the `white` and the `out_of_range/near` row. Those two are
/// chosen for their **conditioning**, not for convenience: with the first entry known, `white` states
/// `b + c` (its input is `(1,1,1)` whatever the print, since a curve fixes 0 and 1) and `near` states
/// `n₁·b + n₂·c`, so the two separate only as far as `n₂ − n₁` allows — `1.6` for the archive's
/// `(-1, 0.4, 2)`. The general form is kept because a **linearised** `near` input is no longer that triple.
/// The `(0.1 0.2 0.3)`/`(0.2 0.4 0.8)` pair — the one a first attempt used — has `n₂ − n₁ = 0.04`, and the
/// archive's `5e-11` print rounding divides through by that, putting `1.25e-9` on an entry whose every
/// effect is read at the tenth decimal; the table it produced missed that decimal on rows of order one. Hence
/// the floor below, which rejects `0.04` and keeps `1.6`.
fn row_of(white: &Reading, near: &Reading, far: &Reading, index: usize) -> [f64; 3] {
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
fn solve(rows: &[Reading]) -> [[f64; 3]; 3] {
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
/// plausible-looking matrix either way: a curved pair linearised under a pure power (`rec2020`, worst
/// residual `0.19`) and a hypothesis that holds (`a98-rgb` under `563/256`, worst `5.0e-11`) otherwise differ
/// by eight orders of magnitude in the same field name, and nothing in the earlier shape of this output told
/// the reader which one they were holding. The bound per channel is the coarser of the archive's tenth-decimal
/// half-step and the floating-point step of the value it printed, times [`SPREAD_HEADROOM`].
fn verdict(matrix: &[[f64; 3]; 3], rows: &[Reading]) -> (bool, f64) {
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

/// The smallest nonzero magnitude this fit actually saw, **in the linearised space it fitted in** (so
/// `prophoto-rgb`'s printed `0.1` arrives as `0.0158` under `1.8`). A piecewise curve (`prophoto-rgb`'s linear
/// segment below `1/16` on the printed side, `rec2020`'s below `0.081/4.5`) is **not** constrained by rows
/// that never enter that segment, and a residual cannot say otherwise — so this is printed beside the verdict
/// instead of leaving the reader to re-derive which branches the archive's rows exercised. Compare it against
/// the segment's own domain before reading it: printed-side thresholds need the printed minimum, which this
/// file no longer shows.
fn smallest_channel(rows: &[Reading]) -> f64 {
    rows.iter()
        .flat_map(|row| row.input.into_iter().chain(row.output))
        .map(f64::abs)
        .filter(|magnitude| *magnitude > 0.0)
        .fold(f64::INFINITY, f64::min)
}

fn apply(matrix: &[[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
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
fn residual(matrix: &[[f64; 3]; 3], rows: &[Reading]) -> (String, f64, f64, f64) {
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

/// The archive's two reference whites, read off the rows that print them: `to_space/srgb/xyz.hrx::white`
/// and `to_space/srgb/xyz_d50.hrx::white`. Multiplying the D50 one by the solved table must land on the
/// D65 one — the check that caught adapting twice.
fn whites(spec: &Path) -> ([f64; 3], [f64; 3]) {
    let one = |file: &str, name: &str| {
        let path = spec.join("srgb").join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        sections(&text)
            .into_iter()
            .find(|(case, _)| case == "white")
            .and_then(|(_, [_, output])| triple(&output, name))
            .unwrap_or_else(|| panic!("{} has no `white` row", path.display()))
    };
    (one("xyz.hrx", "xyz"), one("xyz_d50.hrx", "xyz-d50"))
}

fn main() {
    tracing_bridge::init();
    // One span for the whole measurement, so the reading says which archive and which pair it was taken
    // against and how long it took; the events below live inside it.
    let started = std::time::Instant::now();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // No default pair: a run that silently fell back to `xyz_d50 → xyz` would hand the next reader a
    // correct-looking answer to a question they never asked.
    let Some(([src, dst], gamma)) = (match argv.as_slice() {
        [src, dst] => Some(([src, dst], None)),
        [src, dst, gamma] => Some((
            [src, dst],
            Some(gamma.parse::<f64>().unwrap_or_else(|error| {
                tracing::error!(gamma = %gamma, e = %error, "the third argument must be a number: the hypothesised curve exponent");
                std::process::exit(2);
            })),
        )),
        _ => None,
    }) else {
        tracing::error!(
            args = argv.len(),
            usage = "cargo run -q -p sasspile --example calibrate_space -- <src> <dst> [gamma]",
            "two space names, plus the curve exponent when one side is printed through a transfer curve"
        );
        std::process::exit(2);
    };
    // A pair is admissible when at most one side needs a curve, and then only with an explicit exponent.
    // Both sides curved is refused because one exponent cannot describe two curves. A *piecewise* space
    // (`prophoto-rgb`, `rec2020`) may still be named as the curved side and will be fitted under whatever
    // exponent is passed — this instrument cannot tell a pure power from a segmented curve, which is why
    // [`verdict`] exists and why a failed hypothesis exits non-zero rather than leaving a plausible table on
    // screen: a pure-power guess on `rec2020` fits with a worst residual of `0.19`, and only the verdict says
    // that is not the archive's relation. (An earlier draft of this comment claimed such pairs are refused by
    // name, which the measured `rec2020` run disproved.)
    const LINEAR: [&str; 4] = ["xyz", "xyz_d50", "srgb_linear", "display_p3_linear"];
    let curved_side = match (
        LINEAR.contains(&src.as_str()),
        LINEAR.contains(&dst.as_str()),
        gamma,
    ) {
        (true, true, None) => None,
        (false, true, Some(exponent)) => Some(("from", exponent)),
        (true, false, Some(exponent)) => Some(("to", exponent)),
        (true, true, Some(_)) => {
            tracing::error!(
                "both sides are linear-printed; a third argument would imply a curve that is not there"
            );
            std::process::exit(3);
        }
        (false, false, _) => {
            tracing::error!(
                src = %src,
                dst = %dst,
                linear = ?LINEAR,
                "both sides need a curve hypothesis: at most one side may be non-linear per fit"
            );
            std::process::exit(3);
        }
        _ => {
            tracing::error!(
                src = %src,
                dst = %dst,
                "a curved side needs its hypothesised exponent as the third argument"
            );
            std::process::exit(3);
        }
    };
    let spec = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sass-spec/spec/core_functions/color/to_space");
    let span = tracing::info_span!(
        "calibrate",
        stage = if curved_side.is_some() {
            "curved-pair"
        } else {
            "linear-pair"
        },
        module = "color",
        pair = %format!("{src} → {dst}"),
        archive = %spec.display()
    );
    let _guard = span.enter();
    let file = format!("{dst}.hrx");
    let mirrored = format!("{src}.hrx");
    let rows = readings(&spec, src.as_str(), dst.as_str(), &file, curved_side);
    let solved = solve(&rows);
    // The same relation fitted from the mirrored file's own rows. Both directions of one matrix are then
    // measured twice, and the gap between the two readings **is** the instrument's per-entry noise. That is
    // the number that decides whether a fit may override a transcribed digit; a row residual answers a
    // different question and would let one entry's error borrow another's precision.
    let back = readings(
        &spec,
        dst.as_str(),
        src.as_str(),
        &mirrored,
        curved_side.map(|(side, exponent)| (if side == "from" { "to" } else { "from" }, exponent)),
    );
    let direct = solve(&back);
    let via_inverse = invert(&solved);
    tracing::info!(
        rows = rows.len(),
        from = %format!("{src}/{file}"),
        hypothesis = curved_side.map_or("none (both sides linear)".to_string(), |(side, exponent)| format!("{side} side linearised as a signed power {exponent}")),
        "archive rows read"
    );
    (0..3).for_each(|index| {
        let (fitted, mirrored_fit, inverted) = (solved[index], direct[index], via_inverse[index]);
        let spread = |column: usize| (mirrored_fit[column] - inverted[column]).abs();
        tracing::info!(
            entry = index,
            fitted_first = fitted[0],
            fitted_second = fitted[1],
            fitted_third = fitted[2],
            spread_first = spread(0),
            spread_second = spread(1),
            spread_third = spread(2),
            "this output row as fitted from `<src>/<dst>.hrx`, and per entry how far the same row sits from the mirrored fit (that spread is the noise floor of a digit here)"
        );
    });
    let (case, absolute, relative, magnitude) = residual(&solved, &rows);
    let (holds, worst_ratio) = verdict(&solved, &rows);
    tracing::info!(
        worst_case = %case,
        residual = absolute,
        relative = relative,
        printed_magnitude = magnitude,
        smallest_channel = smallest_channel(&rows),
        hypothesis_holds = holds,
        worst_within_resolution_times = worst_ratio,
        "forward rows reproduced"
    );
    // A refuted hypothesis exits non-zero rather than leaving a fitted table on screen next to a verdict a
    // reader might not have reached yet: this is the difference between `a98-rgb` under `563/256` (holds) and
    // the same pair under a rounded or inverted exponent (does not), or `rec2020` under any pure power.
    if !holds {
        tracing::error!(
            pair = %format!("{src} → {dst}"),
            worst_within_resolution_times = worst_ratio,
            "the hypothesis behind this fit is refuted: its rows are not reproducible within the archive's resolution"
        );
        std::process::exit(4);
    }
    let (case, absolute, relative, magnitude) = residual(&direct, &back);
    tracing::info!(
        rows = back.len(),
        worst_case = %case,
        residual = absolute,
        relative = relative,
        printed_magnitude = magnitude,
        from = %format!("{dst}/{mirrored}"),
        "mirrored rows reproduced by their own fit"
    );
    // Only the white-point pair has a third statement in the archive to check against: it also prints both
    // reference whites, and the table must carry one onto the other. The miss there is a **compound** of two
    // print roundings (the whites are read at the tenth decimal as well), so it sits above the `5e-11`
    // half-step without contradicting it.
    if (src.as_str(), dst.as_str()) == ("xyz_d50", "xyz") {
        let (d65, d50) = whites(&spec);
        let (case, absolute, relative, magnitude) = residual(
            &solved,
            &[Reading {
                case: "srgb/xyz_d50.hrx::white".into(),
                input: d50,
                output: d65,
            }],
        );
        tracing::info!(
            d65_white = ?d65,
            d50_white = ?d50,
            worst_case = %case,
            residual = absolute,
            relative = relative,
            printed_magnitude = magnitude,
            "the table applied to the archive's D50 white must land on its D65 white"
        );
        // The white the library should hold: the mirrored fit applied to the archive's D65 white. This is the
        // one place the `lab` divisor comes from, so it is printed rather than remembered.
        tracing::info!(
            d50_derived = ?apply(&direct, d65),
            "D50 derived from the archive's own D65 white"
        );
    }
    tracing::info!(
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "done"
    );
}

/// A 3×3 inverse by cofactors over the determinant, so the instrument can line the two directions of one
/// fit up against each other and print their per-entry spread. The library computes its own inverse the
/// same way (`matrix::invert`, private to it); this exists only to compare readings, never to produce a
/// table the library then copies from in the other direction.
fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
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
fn det(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

#[path = "../tests/common/tracing_bridge.rs"]
mod tracing_bridge;
