//! Deterministic iron-rule linter for `sasspile`.
//!
//! The recurring violations all stem from one root — the pipeline not being a single
//! owned-value stream. This tool encodes the high-signal structural bans as a rule
//! table and scans Rust sources, so a violation is caught by a machine (CI / test gate)
//! instead of relying on self-discipline. Findings are reported via `tracing`.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: &'static str,
    pub text: String,
}

/// Substring bans: each is a precise signature of a forbidden pattern (kept narrow to
/// avoid false positives on `@for`/`@else if` string literals and comments).
const SUBSTRING_BANS: &[(&str, &[&str])] = &[
    ("no-tree", &["Vec<Rule>", "pub children", ".children"]),
    ("no-relay-vec", &["-> Vec<Rule>"]),
    ("no-string-machine", &["fn subst", "stmt_text"]),
    (
        "no-recursive-descent",
        &["fn write_rule", "fn resolve_all", "fn resolve("],
    ),
];

/// Control-flow keywords banned when they START a (non-comment, non-`//`) line.
const LINE_START_BANS: &[&str] = &["for ", "while ", "loop "];

fn is_comment(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}

fn scan_line(file: &str, line_no: usize, raw: &str) -> Vec<Finding> {
    if is_comment(raw) {
        return Vec::new();
    }
    let matched = |needles: &[&str]| needles.iter().any(|n| raw.contains(n));
    SUBSTRING_BANS
        .iter()
        .filter(|(_, needles)| matched(needles))
        .map(|(rule, _)| Finding {
            file: file.to_string(),
            line: line_no,
            rule,
            text: raw.trim().to_string(),
        })
        .chain(if LINE_START_BANS.iter().any(|kw| raw.trim_start().starts_with(kw)) {
            vec![Finding {
                file: file.to_string(),
                line: line_no,
                rule: "no-imperative-loop",
                text: raw.trim().to_string(),
            }]
        } else {
            Vec::new()
        })
        .collect()
}

pub fn scan_file(path: &Path) -> Vec<Finding> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .enumerate()
        .flat_map(|(i, line)| scan_line(name, i + 1, line))
        .collect()
}

/// Recursively collect `.rs` files under `dir` (tooling may use recursion; the ban is
/// on the compiled pipeline walking a tree, not on a directory scan).
fn rust_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        entries.filter_map(|e| e.ok().map(|e| e.path())).for_each(|p| {
            if p.is_dir() {
                rust_files(&p, acc);
            } else if p.extension().is_some_and(|e| e == "rs") {
                acc.push(p);
            }
        });
    }
}

pub fn scan_dir(dir: &Path) -> Vec<Finding> {
    let mut files = Vec::new();
    rust_files(dir, &mut files);
    files
        .iter()
        .flat_map(|f| scan_file(f))
        .collect::<Vec<_>>()
}
