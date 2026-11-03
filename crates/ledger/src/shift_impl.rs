//! The distance split of the rows `ledger changed` lists. Kept out of the binary so the classifier has a
//! positive control (`crates/ledger/tests/shift_split.rs`), which is included the same way the sasspile
//! tests include `tests/common/tracing_bridge.rs` — the ledger's `lib.rs` sits at its line budget and this
//! is not library surface: nothing but the `shift` command answers the question.

/// How many characters form one number starting at `start`, or `None` when it is not a number. One
/// definition serves both [`skeleton`] and [`numbers`], so the mask and the distance cannot disagree about
/// what counted as a number. A leading sign **is** part of it: a value that flipped its sign is then a big
/// distance rather than a change of kind, which is what the rules ask for (a wrong sign is a wrong value,
/// not a new sort of answer).
fn number_len(chars: &[char], start: usize) -> Option<usize> {
    let digit = |i: usize| chars.get(i).is_some_and(|c| c.is_ascii_digit());
    let signed = matches!(chars.get(start), Some('+') | Some('-'));
    let body = if signed { start + 1 } else { start };
    if !digit(body) && !(chars.get(body) == Some(&'.') && digit(body + 1)) {
        return None;
    }
    let mut i = body;
    while digit(i) {
        i += 1;
    }
    // A decimal point belongs to the run only when digits follow it.
    if chars.get(i) == Some(&'.') {
        let mut j = i + 1;
        while digit(j) {
            j += 1;
        }
        if j > i + 1 {
            i = j;
        }
    }
    // So does an exponent, only when a (possibly signed) digit follows it.
    if matches!(chars.get(i), Some('e') | Some('E')) {
        let mut j = i + 1;
        if matches!(chars.get(j), Some('+') | Some('-')) {
            j += 1;
        }
        if digit(j) {
            while digit(j) {
                j += 1;
            }
            i = j;
        }
    }
    Some(i - start)
}

/// Numbers masked out, so two texts can be compared for **shape** rather than for digits.
pub fn skeleton(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        match number_len(&chars, at) {
            Some(length) => {
                out.push('#');
                at += length;
            }
            None => {
                out.push(chars[at]);
                at += 1;
            }
        }
    }
    out
}

/// The numbers of one text, in order: a distance can only be summed positionally, so the caller has to
/// know when two texts do not line up.
pub fn numbers(text: &str) -> Vec<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut found: Vec<f64> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match number_len(&chars, at) {
            Some(length) => {
                let token: String = chars[at..at + length].iter().collect();
                found.extend(token.parse::<f64>().ok());
                at += length;
            }
            None => at += 1,
        }
    }
    found
}

/// Why a row has no distance to speak of. Named separately because the rules treat them differently: an
/// answer that became an **error** the corpus never stated is a defect to revert, while a row whose
/// expectation is an error message is simply outside what a distance can mean.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Reason {
    /// the stored detail is not a `got … want …` pair (an `expected error` row, or an invalid case)
    ErrorExpectation,
    /// both sides hold the same kind of answer but the text around the numbers differs — a verbatim that
    /// became a computed value, a channel that appeared or vanished
    ShapeOfText,
}

/// One row's verdict. `Distance` carries both totals so the caller can print the movement; `Shape` says the
/// two answers are not the same *kind* of answer, which is a listing question and never a tally one.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Distance { old: f64, new: f64 },
    Shape(Reason),
}

/// The two texts of one harness detail: `got "<css>" want "<css>"`. Anything that does not have that exact
/// shape — an `expected error` line, a row the harness stored differently — yields `None`, and the caller
/// treats that as a change of kind rather than guessing at a distance.
fn got_and_want(detail: &str) -> Option<(&str, &str)> {
    let rest = detail.strip_prefix("got \"")?;
    let (got, tail) = rest.split_once("\" want \"")?;
    Some((got, tail.strip_suffix('"')?))
}

/// Judge one `changed` line: `<key> | a: <detail> | b: <detail>`, each detail written by the harness as
/// `got "<css>" want "<css>"` — or, for a row whose expectation is an error, as
/// `expected error "<msg>", got "<css>"`, which has no distance to speak of.
pub fn judge(line: &str) -> Verdict {
    let (key_and_rest, detail_b) = match line.split_once(" | b: ") {
        Some(parts) => parts,
        None => return Verdict::Shape(Reason::ErrorExpectation),
    };
    let (_, detail_a) = match key_and_rest.split_once(" | a: ") {
        Some(parts) => parts,
        None => return Verdict::Shape(Reason::ErrorExpectation),
    };
    let Some((got_a, want)) = got_and_want(detail_a) else {
        return Verdict::Shape(Reason::ErrorExpectation);
    };
    let Some((got_b, want_b)) = got_and_want(detail_b) else {
        return Verdict::Shape(Reason::ErrorExpectation);
    };
    // One archive expectation, two runs: if the stored expectations differ the row is not comparable.
    if want != want_b || skeleton(got_a) != skeleton(got_b) || skeleton(got_a) != skeleton(want) {
        return Verdict::Shape(Reason::ShapeOfText);
    }
    let (va, vb, vw) = (numbers(got_a), numbers(got_b), numbers(want));
    if vw.is_empty() || va.len() != vw.len() || vb.len() != vw.len() {
        return Verdict::Shape(Reason::ShapeOfText);
    }
    let total =
        |got: &[f64]| -> f64 { got.iter().zip(vw.iter()).map(|(g, w)| (g - w).abs()).sum() };
    Verdict::Distance {
        old: total(&va),
        new: total(&vb),
    }
}

/// The tally of a whole `changed` listing, plus the keys in the two buckets that a count must not absorb:
/// the rows that moved **away** from the archive, and the rows that changed **kind**.
#[derive(Debug, Default)]
pub struct Split {
    pub closer: usize,
    pub farther: usize,
    pub tie: usize,
    pub error_expectation: usize,
    pub shape_of_text: usize,
    pub farther_keys: Vec<String>,
    pub error_keys: Vec<String>,
    pub shape_keys: Vec<String>,
}

pub fn split_of(rows: &[String]) -> Split {
    let mut out = Split::default();
    rows.iter().for_each(|line| {
        let key = line.split(" | ").next().unwrap_or(line).to_string();
        match judge(line) {
            Verdict::Shape(reason) => match reason {
                Reason::ErrorExpectation => {
                    out.error_expectation += 1;
                    out.error_keys.push(key);
                }
                Reason::ShapeOfText => {
                    out.shape_of_text += 1;
                    out.shape_keys.push(key);
                }
            },
            Verdict::Distance { old, new } => match new.partial_cmp(&old) {
                Some(std::cmp::Ordering::Less) => out.closer += 1,
                Some(std::cmp::Ordering::Greater) => {
                    out.farther += 1;
                    out.farther_keys.push(key);
                }
                _ => out.tie += 1,
            },
        }
    });
    out
}
