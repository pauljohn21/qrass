//! Reading one `.hrx` archive file as `(case, input text, output text)` triples — the part of
//! `examples/calibrate_space.rs` that is about the **format**, not about fitting anything, split out because
//! the instrument crossed this repository's 500-line file limit and the split point is where the two
//! concerns already differ.
//!
//! It is shared by `#[path]` (the way `tests/common/tracing_bridge.rs` is shared) rather than being a crate
//! module, so nothing in `src/` depends on it. `tests/hrx.rs` keeps its own parser on purpose: it maps
//! sections into a `Case` carrying verdict bookkeeping the harness needs (`extra_inputs`, `has_options`,
//! `error`) and returns `PASS`/`FAIL` lines, which is a different question from "give me the two texts of
//! this case"; merging them would make the measurement harness depend on a calibration tool.

use std::collections::BTreeMap;
use std::path::Path;

/// Which side of a pair is printed through a transfer curve. A **binary fact, so it is a value**: the first
/// version carried it as `"from"`/`"to"` string labels, compared by equality in three places, where a typo
/// was indistinguishable from a correct reading at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    From,
    To,
}

/// A curve hypothesis under test: which side of a pair is curved, and with which **decode** exponent
/// (`linear = printed^exponent`). The instrument never claims this is the archive's law — it fits under it and
/// reports whether the result can be told from the archive's own rows.
#[derive(Clone, Copy, Debug)]
pub struct Hypothesis {
    pub side: Side,
    pub exponent: f64,
}

/// One archive reading: the case name, the input triple, the printed output triple, and **the same two
/// triples before any linearisation**. The raw pair is kept because a piecewise transfer decides its branch
/// on the printed value (`prophoto-rgb` compares `|channel|` to `1/16`, `rec2020` to `0.081/4.5`), while a fit
/// runs on linearised ones — without both, "the segment branch was never exercised by these rows" would be a
/// reader's inference rather than a printed fact.
pub struct Reading {
    pub case: String,
    pub input: [f64; 3],
    pub output: [f64; 3],
    pub printed_input: [f64; 3],
    pub printed_output: [f64; 3],
}

/// `<===> <case>/input.scss` … `<===>` — the section convention the archive is written in. A bare
/// `<===>` followed by `====` is a group separator and carries no case.
pub fn sections(text: &str) -> BTreeMap<String, [String; 2]> {
    let lines: Vec<&str> = text.lines().collect();
    let mut bodies: BTreeMap<String, [String; 2]> = BTreeMap::new();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("<===>"))
        .for_each(|(index, line)| {
            let path = line["<===>".len()..].trim();
            if path.is_empty() || path.starts_with("====") {
                return;
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
/// ends at `)` or at the alpha separator `/`, and a `none` simply fails to parse — which is how a powerless
/// channel drops out of a fit here instead of being read as some number it is not.
pub fn triple(text: &str, name: &str) -> Option<[f64; 3]> {
    let (_, tail) = text.split_once(&format!("color({name} "))?;
    let body = tail.split([')', '/']).next()?;
    let channels: Vec<f64> = body
        .split_ascii_whitespace()
        .take(3)
        .map(|token| token.parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()?;
    (channels.len() == 3).then(|| [channels[0], channels[1], channels[2]])
}

/// The space name as it is written inside `color( )`: the directory spells `xyz_d50`, the value spells
/// `xyz-d50`, and `srgb_linear` writes `srgb-linear`.
pub fn space_name(directory: &str) -> String {
    directory.replace('_', "-")
}

/// The concrete rows of one `to_space/<from>/<to>.hrx`, each row's channels read in the order the archive
/// writes them: `color(<from> …)` on the input side, `color(<to> …)` on the output side.
///
/// When one side is printed through a transfer curve, the caller says which side and with which **decode**
/// exponent (`linear = printed^exponent`), and the readings are linearised here — the exponent is a
/// hypothesis the caller is testing, never a claim of this file.
pub fn readings(
    spec: &Path,
    from: &str,
    to: &str,
    file: &str,
    curved: Option<Hypothesis>,
) -> Vec<Reading> {
    let path = spec.join(from).join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    sections(&text)
        .into_iter()
        .filter_map(|(case, [input, output])| {
            let input = triple(&input, &space_name(from))?;
            let output = triple(&output, &space_name(to))?;
            let (printed_input, printed_output) = (input, output);
            let power = |channels: [f64; 3], exponent: f64| {
                channels.map(|channel| {
                    if channel == 0.0 {
                        0.0
                    } else {
                        channel.signum() * channel.abs().powf(exponent)
                    }
                })
            };
            let (input, output) = match curved {
                Some(hypothesis) if hypothesis.side == Side::From => {
                    (power(input, hypothesis.exponent), output)
                }
                Some(hypothesis) => (input, power(output, hypothesis.exponent)),
                None => (input, output),
            };
            Some(Reading {
                case,
                input,
                output,
                printed_input,
                printed_output,
            })
        })
        .collect()
}
