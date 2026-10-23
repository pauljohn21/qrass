//! OpenSpec refactor-rfp-strict task 1.1: the machine-checkable style gate.
//!
//! Scans `src/**/*.rs`, masks comments and string literals, and counts the
//! imperative constructs the RFP contract forbids (iteration loops, `else if`
//! chains, mutable accumulators). The counts are compared against
//! `tests/golden/style-baseline.txt`: per file, each count may only go DOWN
//! (new files must start clean unless exempted). Bless a new baseline with
//! `QRASS_STYLE_GATE_BLESS=1 cargo test --test style_gate`.
//!
//! The scanner itself is written in the contract's own style (fold +
//! combinators, no iteration loops).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

const BASELINE: &str = "tests/golden/style-baseline.txt";

/// The repo rule (rust.md): no source file may exceed this many lines.
const LINE_BUDGET: usize = 500;
const LINE_BASELINE: &str = "tests/golden/line-budget.txt";

/// One file's imperative-construct counts: (loops, else-if chains, `let mut`,
/// guard `return` statements). RFP contract R3/S2: a function's statement-level
/// early returns are folded to single-exit `match`/`Shape` dispatch, so this
/// count may only fall.
type Counts = (u32, u32, u32, u32);

/// Mask a source line to its code part: drop `//` comments and blank out the
/// contents of string/char literals (doc comments included in the drop).
fn code_part(line: &str) -> String {
    let (masked, _, str_open) = line.chars().fold(
        (String::new(), false, None::<char>),
        |(mut out, in_line_comment, str_open), c| match (str_open, in_line_comment, c) {
            (_, true, _) => (out, true, str_open), // inside // comment: swallow
            (Some(q), false, ch) if ch == q && !ends_with_backslash(&out) => {
                out.push(' ');
                (out, false, None)
            }
            (Some(_), false, _) => {
                out.push(' ');
                (out, false, str_open)
            }
            (None, false, '/') if out.ends_with('/') => {
                // second slash of a line comment: drop the tail
                out.pop();
                (out, true, None)
            }
            (None, false, ch @ ('"' | '\'')) => {
                out.push(' ');
                (out, false, Some(ch))
            }
            (_, false, c) => {
                out.push(c);
                (out, false, None)
            }
        },
    );
    let _ = str_open;
    masked
}

fn ends_with_backslash(s: &str) -> bool {
    s.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}

/// A forbidden keyword only counts at a statement boundary: the char before it
/// must be none / whitespace / `{` / `(`, excluding `impl … for …` type lines.
fn is_statement_hit(line: &str, at: usize) -> bool {
    let before_ok = at == 0
        || matches!(line.as_bytes()[at - 1], b' ' | b'\t' | b'{' | b'(' | b';' | b')');
    let impl_line = line.contains("impl ") || line.contains("<dyn ");
    before_ok && !impl_line
}

fn count_line(line: &str) -> Counts {
    let code = code_part(line);
    let kw = |needle: &str| {
        code.match_indices(needle)
            .filter(|(at, _)| is_statement_hit(&code, *at))
            .count() as u32
    };
    (
        kw("for ") + kw("while ") + kw("loop ") + kw("loop{"),
        kw("else if"),
        kw("let mut "),
        kw("return "),
    )
}

fn scan_file(path: &Path) -> Option<Counts> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.lines().map(count_line).fold((0, 0, 0, 0), merge))
}

const fn merge(a: Counts, b: Counts) -> Counts {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3)
}

/// Recursively collect `.rs` files (fold over directory listings).
fn rs_tree(dir: &Path, acc: Vec<PathBuf>) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .fold(acc, |mut a, p| {
            let next = if p.is_dir() {
                rs_tree(&p, a)
            } else {
                if p.extension().is_some_and(|x| x == "rs") {
                    a.push(p);
                }
                a
            };
            a = next;
            a
        })
}

fn scan_all(root: &Path) -> BTreeMap<String, Counts> {
    rs_tree(root, Vec::new())
        .into_iter()
        .filter_map(|p| {
            scan_file(&p).map(|c| {
                (
                    p.strip_prefix(root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    c,
                )
            })
        })
        .filter(|(_, c)| *c != (0, 0, 0, 0))
        .collect()
}

fn render(map: &BTreeMap<String, Counts>) -> String {
    map.iter()
        .map(|(f, c)| format!("{f}\t{}\t{}\t{}\t{}\n", c.0, c.1, c.2, c.3))
        .collect()
}

fn parse_baseline(text: &str) -> BTreeMap<String, Counts> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut cols = l.split('\t');
            Some((
                cols.next()?.to_string(),
                (
                    cols.next()?.parse().ok()?,
                    cols.next()?.parse().ok()?,
                    cols.next()?.parse().ok()?,
                    cols.next()?.parse().ok().unwrap_or(0),
                ),
            ))
        })
        .collect()
}

#[test]
fn style_gate_counts_only_decrease() {
    let current = scan_all(Path::new("src"));
    let baseline_path = Path::new(BASELINE);
    if env::var("QRASS_STYLE_GATE_BLESS").is_ok_and(|v| v == "1") {
        fs::write(baseline_path, format!("# style baseline: file\\tloops\\telseif\\tletmut\\treturns\n{}", render(&current)))
            .expect("bless baseline");
        return;
    }
    let baseline = parse_baseline(
        &fs::read_to_string(baseline_path).expect("baseline missing — bless one first"),
    );
    let regressions: Vec<(&String, Counts, Counts)> = current
        .iter()
        .filter(|(f, c)| {
            baseline
                .get(*f)
                .is_none_or(|b| c.0 > b.0 || c.1 > b.1 || c.2 > b.2 || c.3 > b.3)
        })
        .map(|(f, c)| (f, *c, baseline.get(f).copied().unwrap_or((0, 0, 0, 0))))
        .collect();
    assert!(
        regressions.is_empty(),
        "style gate: imperative constructs grew: {regressions:?}"
    );
}

/// Files currently over the line budget, keyed for a stable baseline.
fn oversize(root: &Path) -> BTreeMap<String, usize> {
    rs_tree(root, Vec::new())
        .into_iter()
        .filter_map(|p| {
            let lines = fs::read_to_string(&p).ok()?.lines().count();
            (lines > LINE_BUDGET).then(|| {
                (
                    p.strip_prefix(root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    lines,
                )
            })
        })
        .collect()
}

fn parse_line_budget(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut cols = l.split('\t');
            Some((cols.next()?.to_string(), cols.next()?.parse().ok()?))
        })
        .collect()
}

/// OpenSpec refactor-rfp-strict task 5b.1: the 500-line rule as a gate. Each
/// over-budget file may only SHRINK, and a file that crosses the budget for the
/// first time fails until it is split (or explicitly blessed).
#[test]
fn line_budget_only_shrinks() {
    let current = oversize(Path::new("src"));
    let baseline_path = Path::new(LINE_BASELINE);
    if env::var("QRASS_STYLE_GATE_BLESS").is_ok_and(|v| v == "1") {
        let body = current.iter().fold(String::new(), |mut acc, (f, n)| {
            acc.push_str(&format!("{f}\t{n}\n"));
            acc
        });
        fs::write(
            baseline_path,
            format!("# files over the {LINE_BUDGET}-line budget: file\tlines\n{body}"),
        )
        .expect("bless line budget");
        return;
    }
    let baseline = parse_line_budget(
        &fs::read_to_string(baseline_path).expect("line budget baseline missing — bless one first"),
    );
    let offenders: Vec<(&String, &usize)> = current
        .iter()
        .filter(|(f, n)| baseline.get(*f).is_none_or(|b| *n > b))
        .collect();
    assert!(
        offenders.is_empty(),
        "line budget: files grew past (or newly exceeded) the {LINE_BUDGET}-line rule: {offenders:?} — split them like the `parser_*` / `builtins/color_*` siblings"
    );
}
