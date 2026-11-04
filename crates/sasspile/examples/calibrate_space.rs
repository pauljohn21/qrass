//! A calibration instrument for the colour-space tables: it reads the archive's **own** rows for one
//! *linear* pair of spaces and solves the matrix between them, so the digits in
//! [`sasspile`'s tables](../sasspile/src/color/matrix.rs) come from a re-runnable measurement instead of
//! from what I last transcribed.
//!
//! ```text
//! cargo run -q -p sasspile --example calibrate_space -- xyz_d50 xyz        # the white-point adaptation
//! cargo run -q -p sasspile --example calibrate_space -- display_p3_linear xyz
//! cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz 2.19921875
//! cargo run -q -p sasspile --example calibrate_space -- xyz rec2020 to 2.4 1.0
//! ```
//!
//! A curve may be claimed for at most one side, and then only with an explicit **decode exponent** plus an
//! optional **`alpha`** offset (`linear = ((printed + alpha - 1)/alpha)^exponent`; omit `alpha`, or pass `1.0`,
//! for a pure gamma). The side defaults to the input (`<src>`); name it outright with `from`/`to` when the
//! curve sits on the output. Either way the exponent is a hypothesis under test, the fit runs on linearised
//! readings, and [`verdict`] decides whether it held - a refuted one exits non-zero, because a fitted matrix
//! for a relation that is not linear under that hypothesis prints exactly like evidence and is otherwise
//! indistinguishable from a good one. Which spaces are curve-free is **not** restated here: `tasks.md` 9.19
//! records how a second list of that partition drifted from the library's own.
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

/// The solve-and-judge half of this instrument, shared by `#[path]` like the reader above it: the example is
/// arguments and reporting, nothing else.
#[path = "../tests/common/fit.rs"]
mod fit;

use crate::fit::{
    apply, invert, residual, smallest_curved_printed, smallest_linear, solve, verdict,
};
use crate::hrx_read::{Hypothesis, Reading, Side, readings, whites};

fn parse_number(text: &str, what: &str) -> f64 {
    text.parse::<f64>().unwrap_or_else(|error| {
        tracing::error!(value = %text, e = %error, what = %what, "this argument must be a number");
        std::process::exit(2);
    })
}

fn main() {
    tracing_bridge::init();
    // One span for the whole measurement, so the reading says which archive and which pair it was taken
    // against and how long it took; the events below live inside it.
    let started = std::time::Instant::now();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // No default pair: a run that silently fell back to `xyz_d50 → xyz` would hand the next reader a
    // correct-looking answer to a question they never asked.
    if argv.len() < 2 {
        tracing::error!(
            args = argv.len(),
            usage = "cargo run -q -p sasspile --example calibrate_space -- <src> <dst> [from|to] [exponent] [alpha]",
            "two space names are required"
        );
        std::process::exit(2);
    }
    // Whether a pair is admissible is **not** decided by a list of names kept here — this file used to hold
    // `const LINEAR: [&str; 4]`, and `tasks.md` 9.19/9.20 recorded it drifting from `Space::transfer()` (which
    // answers curve-free for `lab`/`lch`/`oklab`/`oklch` too, and is not reachable from here without making the
    // `color` module part of the crate's public surface - a decision this tool does not get to take silently).
    // The archive decides instead: fit under whatever is claimed and let [`verdict`] refute it. Measured on the
    // pairs this instrument has settled - a curved side fitted as if linear lands at `0.019`…`0.29` residual,
    // eight to nine orders above the resolution bound, so refutation is loud rather than a table that looks fine.
    //
    // Forms: `<src> <dst>` (no curve claimed), or `<src> <dst> <exponent> [alpha]` for a curve on the **input**
    // side, or `<src> <dst> from|to <exponent> [alpha]` to name the side outright. A *piecewise* space can be
    // named as the curved side and gets fitted under whatever exponent is passed - one power cannot describe a
    // segment, which is another reason the verdict, not a name list, is what settles a hypothesis
    // (`tasks.md` 9.20 measured `-- rec2020 xyz 2.2` refuted at `2.4e8×` its resolution, exit 4).
    let (src, dst) = (&argv[0], &argv[1]);
    // The third argument decides the shape rather than the arity: a number means "the input side is curved,
    // this is its exponent", a word means the side is being named. Ordered by content because a four-element
    // slice cannot tell `[exp, alpha]` from `[side, exp]` by position alone - matching by position first
    // produced an `unreachable pattern` warning, which is the compiler saying the two forms overlap.
    let curved_side = match argv.as_slice() {
        [_, _] => None,
        // A number third: the curve is claimed for the input side, with an optional offset after it.
        [_, _, third, ..] if third.parse::<f64>().is_ok() => Some(Hypothesis {
            side: Side::From,
            exponent: parse_number(third, "the hypothesised curve exponent"),
            offset: argv
                .get(3)
                .map(|text| parse_number(text, "the hypothesised curve offset alpha")),
        }),
        // A word third: the side is named outright, so the exponent must follow it.
        [_, _, side, exponent, ..] => {
            let which = match side.as_str() {
                "from" => Side::From,
                "to" => Side::To,
                other => {
                    tracing::error!(
                        side = %other,
                        third = %side,
                        "the third argument is a number (the exponent, applied to the input side) or the word `from` / `to`"
                    );
                    std::process::exit(2);
                }
            };
            Some(Hypothesis {
                side: which,
                exponent: parse_number(exponent, "the hypothesised curve exponent"),
                offset: argv
                    .get(4)
                    .map(|text| parse_number(text, "the hypothesised curve offset alpha")),
            })
        }
        _ => {
            tracing::error!(
                args = argv.len(),
                usage = "cargo run -q -p sasspile --example calibrate_space -- <src> <dst> [from|to] [exponent] [alpha]",
                "two space names, then optionally the curve hypothesis"
            );
            std::process::exit(2);
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
        // The mirrored file swaps which side carries the curve; the enum makes that swap exhaustive.
        curved_side.map(|hypothesis| Hypothesis {
            side: match hypothesis.side {
                Side::From => Side::To,
                Side::To => Side::From,
            },
            exponent: hypothesis.exponent,
            offset: hypothesis.offset,
        }),
    );
    let direct = solve(&back);
    let via_inverse = invert(&solved);
    tracing::info!(
        rows = rows.len(),
        from = %format!("{src}/{file}"),
        hypothesis = curved_side.map_or("none (both sides linear)".to_string(), |hypothesis| {
            format!(
                "{} side linearised as a signed power {}{}",
                match hypothesis.side {
                    Side::From => "input",
                    Side::To => "output",
                },
                hypothesis.exponent,
                hypothesis.offset.map_or(String::new(), |alpha| format!(
                    " with offset alpha={alpha} (((v+alpha-1)/alpha)^{hypothesis_exponent})",
                    hypothesis_exponent = hypothesis.exponent
                ))
            )
        }),
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
        smallest_curved_printed_channel = smallest_curved_printed(&rows, curved_side),
        smallest_linear_channel = smallest_linear(&rows),
        hypothesis_holds = holds,
        worst_within_resolution_times = worst_ratio,
        "forward rows reproduced"
    );
    // A refuted hypothesis exits non-zero rather than leaving a fitted table on screen next to a verdict a
    // reader might not have reached yet: this is the difference between `a98-rgb` under `563/256` (holds) and
    // the same pair under a rounded or inverted exponent (does not), or `rec2020` under `2.2`.
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
                // This pair is linear on both sides, so the printed and linearised triples are the same.
                printed_input: d50,
                printed_output: d65,
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

#[path = "../tests/common/tracing_bridge.rs"]
mod tracing_bridge;
