//! A calibration instrument for the colour-space tables: it reads the archive's **own** `xyz ↔ xyz-d50`
//! rows and solves the white-point adaptation out of them, so the digits in
//! [`sasspile`'s table](../sasspile/src/color/matrix.rs) come from a re-runnable measurement instead of
//! from what I last transcribed.
//!
//! ```text
//! cargo run -q -p sasspile --example calibrate_space
//! ```
//!
//! Why this pair and not some other: `xyz` and `xyz-d50` carry the same kind of number with no transfer
//! curve between them, so one row of the archive states a linear equation and three well-spread rows pin
//! three unknowns exactly. Every other space in `to_space/**` puts a curve (or a cone root) in the way, so
//! a least-squares fit there would be fitting the wrong shape — which is precisely how a hand-copied
//! primaries table kept being defended by rows it could not have produced.
//!
//! The instrument does two things and reports both:
//! * it solves the D50→D65 matrix from `to_space/xyz_d50/xyz.hrx` and gives the residual against **every**
//!   concrete row of that file, including the rows the solve did not use;
//! * it applies that solved matrix **backwards** to `to_space/xyz/xyz_d50.hrx` — each of those rows says
//!   `color(xyz …)` became `color(xyz-d50 …)`, so multiplying its output by the forward table must return
//!   its input. That is the archive's own round trip, checked without ever writing down a second table
//!   ([`crate::color::matrix`] keeps exactly one direction and computes the other).
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
/// though it were one.
fn residual(matrix: &[[f64; 3]; 3], rows: &[Reading], forward: bool) -> (String, f64, f64, f64) {
    rows.iter()
        .flat_map(|row| {
            let (input, output) = if forward {
                (row.input, row.output)
            } else {
                (row.output, row.input)
            };
            apply(matrix, input)
                .into_iter()
                .zip(output)
                .map(move |(predicted, printed)| {
                    let error = (predicted - printed).abs();
                    (
                        row.case.clone(),
                        error,
                        error / printed.abs(),
                        printed.abs(),
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
    // One span for the whole measurement, so the reading says which archive it was taken against and how
    // long it took; the events below live inside it.
    let started = std::time::Instant::now();
    let spec = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sass-spec/spec/core_functions/color/to_space");
    let span = tracing::info_span!(
        "calibrate",
        stage = "adaptation",
        module = "color",
        archive = %spec.display()
    );
    let _guard = span.enter();
    let rows = readings(&spec, "xyz_d50", "xyz", "xyz.hrx");
    let solved = solve(&rows);
    tracing::info!(
        rows = rows.len(),
        from = "xyz_d50/xyz.hrx",
        "archive rows read (the `black` row is 0 → 0 and carries no information)"
    );
    solved.iter().enumerate().for_each(|(index, row)| {
        tracing::info!(
            entry = index,
            first = row[0],
            second = row[1],
            third = row[2],
            "solved D50→D65 row"
        );
    });
    let (case, absolute, relative, magnitude) = residual(&solved, &rows, true);
    tracing::info!(
        worst_case = %case,
        residual = absolute,
        relative = relative,
        printed_magnitude = magnitude,
        "forward rows reproduced"
    );
    let back = readings(&spec, "xyz", "xyz_d50", "xyz_d50.hrx");
    let (case, absolute, relative, magnitude) = residual(&solved, &back, false);
    tracing::info!(
        rows = back.len(),
        worst_case = %case,
        residual = absolute,
        relative = relative,
        printed_magnitude = magnitude,
        "applying the same table backwards to xyz/xyz_d50.hrx — the archive's own round trip"
    );
    // The archive states both reference whites; the table must carry one onto the other. The miss here is
    // a **compound** of two print roundings — the whites are read at the tenth decimal and the table was
    // solved from rows read at the tenth decimal — so it sits above the `5e-11` half-step rather than
    // contradicting it.
    let (d65, d50) = whites(&spec);
    let (case, absolute, relative, magnitude) = residual(
        &solved,
        &[Reading {
            case: "srgb/xyz_d50.hrx::white".into(),
            input: d50,
            output: d65,
        }],
        true,
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
    // The white the library should hold: the table read backwards, applied to the archive's D65 white. This
    // is the one place the `lab` divisor comes from, so it is printed rather than remembered.
    tracing::info!(d50_derived = ?backwards(&solved, d65), "table⁻¹ applied to the archive's D65 white");
    tracing::info!(
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "done"
    );
}

/// A determinant of a 3×3 written as rows.
fn det(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Cramer's rule on one column, so the backwards white needs no inverse matrix written down here either.
fn backwards(matrix: &[[f64; 3]; 3], target: [f64; 3]) -> [f64; 3] {
    let whole = det(*matrix);
    (0..3)
        .map(|column| {
            let swapped: [[f64; 3]; 3] = std::array::from_fn(|row| {
                std::array::from_fn(|place| {
                    if place == column {
                        target[row]
                    } else {
                        matrix[row][place]
                    }
                })
            });
            det(swapped) / whole
        })
        .collect::<Vec<f64>>()
        .try_into()
        .expect("three channels")
}

#[path = "../tests/common/tracing_bridge.rs"]
mod tracing_bridge;
