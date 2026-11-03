//! A calibration instrument for the colour-space tables: it reads the archive's **own** rows for one
//! *linear* pair of spaces and solves the matrix between them, so the digits in
//! [`sasspile`'s tables](../sasspile/src/color/matrix.rs) come from a re-runnable measurement instead of
//! from what I last transcribed.
//!
//! ```text
//! cargo run -q -p sasspile --example calibrate_space -- xyz_d50 xyz        # the white-point adaptation
//! cargo run -q -p sasspile --example calibrate_space -- display_p3_linear xyz
//! ```
//!
//! Only a **linear** pair is accepted, and the rule is executed: both names must be one of `xyz`,
//! `xyz-d50`, `srgb-linear` or `display-p3-linear`, because those are the spaces whose channels are printed
//! as the matrix's own output — no transfer curve, no cone root in between. Any other pair is refused
//! rather than fitted, because a fitted matrix for a curved relation prints exactly like evidence. (The
//! first version of this instrument only said this in prose, and `-- display_p3 xyz` cheerfully produced
//! a table with a `1.6e7` residual.)
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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One archive reading: the case name, the input triple, the printed output triple.
struct Reading {
    case: String,
    input: [f64; 3],
    output: [f64; 3],
}

/// `<===> <case>/input.scss` … `<===>` — the same section convention the harness parses, kept local to this
/// instrument because the harness's own parser lives inside a `#[tokio::test]` file and is not a library.
fn sections(text: &str) -> BTreeMap<String, [String; 2]> {
    let lines: Vec<&str> = text.lines().collect();
    let mut bodies: BTreeMap<String, [String; 2]> = BTreeMap::new();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("<===>"))
        .for_each(|(index, line)| {
            let path = line["<===>".len()..].trim();
            if path.is_empty() || path.starts_with("====") {
                return; // the group separator carries no case
            }
            let (key, file) = match path.rsplit_once('/') {
                Some((key, file)) => (key.to_string(), file),
                None => return,
            };
            let body: String = lines[index + 1..]
                .iter()
                .take_while(|next| !next.starts_with("<===>"))
                .map(|line| format!("{line}\n"))
                .collect();
            let slot = bodies.entry(key).or_default();
            match file {
                "input.scss" => slot[0] = body,
                "output.css" => slot[1] = body,
                _ => {}
            }
        });
    bodies
}

/// The three numbers after `color(<name> `, or `None` when the row is not fully numeric. The channel list
/// ends at `)` or at the alpha separator `/`, and a `none` simply fails to parse.
fn triple(text: &str, name: &str) -> Option<[f64; 3]> {
    let (_, tail) = text.split_once(&format!("color({name} "))?;
    let body = tail.split([')', '/']).next()?;
    let channels: Vec<f64> = body
        .split_ascii_whitespace()
        .take(3)
        .map(|token| token.parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()?;
    (channels.len() == 3).then(|| [channels[0], channels[1], channels[2]])
}

/// The concrete rows of one `to_space/<from>/<to>.hrx`, with each row's channels read in the order the
/// archive writes them: `color(<from> …)` on the input side, `color(<to> …)` on the output side.
fn readings(spec: &Path, from: &str, to: &str, file: &str) -> Vec<Reading> {
    let path = PathBuf::from(spec).join(from).join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    sections(&text)
        .into_iter()
        .filter_map(|(case, [input, output])| {
            let input = triple(&input, &space_name(from))?;
            let output = triple(&output, &space_name(to))?;
            Some(Reading {
                case,
                input,
                output,
            })
        })
        .collect()
}

/// The space name as it is written inside `color( )`: the directory spells `xyz_d50`, the value spells
/// `xyz-d50`, and `srgb_linear` writes `srgb-linear`.
fn space_name(directory: &str) -> String {
    directory.replace('_', "-")
}

/// The first entry of one output row, straight off the `out_of_range/far` row: it multiplies
/// `color(xyz-d50 -999999 0 0)`, so that one printed number **is** the entry times -999999. Dividing the
/// archive's miss (`5e-11` at that magnitude, or the `1.2e-16` relative step the print itself cannot beat)
/// by `999999` is what makes this the most precisely-determined entry of the whole table.
fn entry_one(row: &Reading, index: usize) -> f64 {
    row.output[index] / row.input[0]
}

/// The other two entries of one output row, from the `white` `(1 1 1)` row and the `out_of_range/near`
/// `(-1 0.4 2)` row. Those two are chosen for their **conditioning**, not for convenience: with the first
/// entry already known, `white` states `b + c` and `near` states `0.4b + 2c`, whose determinant is `1.6`.
/// The `(0.1 0.2 0.3)` and `(0.2 0.4 0.8)` pair — the one a first attempt used — has determinant `0.04`, so
/// it spreads the archive's own `5e-11` print rounding over the entries about forty times wider, and the
/// table it produced missed the tenth decimal on rows whose input was of order one.
fn row_of(white: &Reading, near: &Reading, far: &Reading, index: usize) -> [f64; 3] {
    let first = entry_one(far, index);
    let sum = white.output[index] - first;
    let rest = near.output[index] + first;
    let second = (2.0 * sum - rest) / 1.6;
    let third = (rest - 0.4 * sum) / 1.6;
    [first, second, third]
}

/// The solved matrix, one output row at a time: `far` gives the first entry, `white` and `near` give the
/// other two, and every remaining concrete row is left for [`residual`] to check against.
fn solve(rows: &[Reading]) -> [[f64; 3]; 3] {
    let one = |input: [f64; 3]| {
        rows.iter()
            .find(|row| row.input == input)
            .unwrap_or_else(|| panic!("the archive has no row with input {input:?}"))
    };
    std::array::from_fn(|index| {
        row_of(
            one([1.0, 1.0, 1.0]),
            one([-1.0, 0.4, 2.0]),
            one([-999_999.0, 0.0, 0.0]),
            index,
        )
    })
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
    let Some([src, dst]) = (argv.len() == 2).then(|| [&argv[0], &argv[1]]) else {
        tracing::error!(
            args = argv.len(),
            usage = "cargo run -q -p sasspile --example calibrate_space -- <src> <dst>",
            "exactly two space names, each of: xyz, xyz_d50, srgb_linear, display_p3_linear"
        );
        std::process::exit(2);
    };
    // The selection rule, executed rather than only stated: these are the spaces whose channels are printed
    // with no transfer curve and no cone root between them. Fitting any other pair would print a matrix for
    // a relation that is not linear, and that output would look exactly like evidence.
    const LINEAR: [&str; 4] = ["xyz", "xyz_d50", "srgb_linear", "display_p3_linear"];
    [src, dst].iter().for_each(|name| {
        if !LINEAR.contains(&name.as_str()) {
            tracing::error!(
                name = %name,
                allowed = ?LINEAR,
                "not a linear-printed space: its channels go through a transfer curve or a cone root, so no matrix can be solved from its rows"
            );
            std::process::exit(3);
        }
    });
    let spec = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sass-spec/spec/core_functions/color/to_space");
    let span = tracing::info_span!(
        "calibrate",
        stage = "linear-pair",
        module = "color",
        pair = %format!("{src} → {dst}"),
        archive = %spec.display()
    );
    let _guard = span.enter();
    let file = format!("{dst}.hrx");
    let mirrored = format!("{src}.hrx");
    let rows = readings(&spec, src.as_str(), dst.as_str(), &file);
    let solved = solve(&rows);
    // The same relation fitted from the mirrored file's own rows. Both directions of one matrix are then
    // measured twice, and the gap between the two readings **is** the instrument's per-entry noise. That is
    // the number that decides whether a fit may override a transcribed digit; a row residual answers a
    // different question and would let one entry's error borrow another's precision.
    let back = readings(&spec, dst.as_str(), src.as_str(), &mirrored);
    let direct = solve(&back);
    let via_inverse = invert(&solved);
    tracing::info!(rows = rows.len(), from = %format!("{src}/{file}"), "archive rows read");
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
    tracing::info!(
        worst_case = %case,
        residual = absolute,
        relative = relative,
        printed_magnitude = magnitude,
        "forward rows reproduced"
    );
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
