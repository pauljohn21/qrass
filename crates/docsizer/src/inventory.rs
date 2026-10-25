//! Inventory — which lines of a Rust source tree are **teachable**, i.e. must be explained by
//! some lesson in the from-scratch tutorial.
//!
//! The classification is deliberately conservative and *auditable*: every excluded line is
//! counted under a named reason, so the "teachable" total the tutorial is graded against can be
//! re-derived and argued about, instead of being a magic number.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Why a line is not counted. Reported per file so exclusions can be reviewed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Exclusions {
    /// inside a `#[cfg(test)] mod … { … }` item
    pub test_module: usize,
    /// whitespace only
    pub blank: usize,
    /// a `use` / `pub use` statement
    pub pure_use: usize,
    /// only structural punctuation: `{ } ( ) , ;`
    pub brace_only: usize,
}

impl Exclusions {
    pub fn total(&self) -> usize {
        self.test_module + self.blank + self.pure_use + self.brace_only
    }
}

/// One source file's tally. `teachable` holds the 1-based line numbers that need explaining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub path: String,
    pub lines: usize,
    pub teachable: Vec<usize>,
    pub excl: Exclusions,
}

/// State of the `#[cfg(test)]` scan: `None` outside a test item, `Some(depth)` inside or waiting
/// for its opening brace (the attribute line itself carries no brace).
#[derive(Debug, Default)]
struct Scan {
    in_test: Option<i32>,
}

fn braces(line: &str) -> i32 {
    line.chars().fold(0, |acc, c| match c {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

fn reason_of(trimmed: &str) -> Option<Reason> {
    match trimmed {
        "" => Some(Reason::Blank),
        t if t.starts_with("use ") || t == "use" || t.starts_with("pub use ") => Some(Reason::Use),
        t if !t.is_empty() && t.chars().all(|c| matches!(c, '{' | '}' | '(' | ')' | ',' | ';')) => {
            Some(Reason::Braces)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum Reason {
    Blank,
    Use,
    Braces,
}

/// Classify one file's text into teachable line numbers plus exclusion counters.
pub fn classify(path: &str, text: &str) -> FileReport {
    let mut scan = Scan::default();
    let mut excl = Exclusions::default();
    let mut teachable = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    lines.iter().enumerate().for_each(|(index, line)| {
        let trimmed = line.trim();
        let depth = braces(line);
        let counted = match scan.in_test {
            // Already inside (or waiting for) a test item: consume until its braces balance again.
            Some(open) => {
                let closing = open + depth;
                scan.in_test = (closing > 0).then_some(closing);
                true
            }
            None => {
                let opens_test = trimmed.starts_with("#[cfg(") && trimmed.contains("test");
                // Waiting at depth 0 keeps the item's own `mod tests {` line inside the exclusion.
                scan.in_test = opens_test.then_some(0);
                opens_test
            }
        };

        match counted.then_some(Reason::Blank).or_else(|| reason_of(trimmed)) {
            Some(Reason::Blank) if counted => excl.test_module += 1,
            Some(Reason::Blank) => excl.blank += 1,
            Some(Reason::Use) => excl.pure_use += 1,
            Some(Reason::Braces) => excl.brace_only += 1,
            None => teachable.push(index + 1),
        }
    });

    FileReport { path: path.to_string(), lines: lines.len(), teachable, excl }
}

/// All `.rs` files under `dir`, as stable relative paths, sorted.
pub fn sources(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(dir, dir, &mut found)?;
    found.sort();
    Ok(found)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() || path.is_dir())
        .collect();
    entries.iter().try_fold((), |(), path| {
        match (path.is_dir(), path.extension().and_then(|e| e.to_str()) == Some("rs")) {
            (true, _) => walk(root, path, out),
            (false, true) => {
                out.push(path.strip_prefix(root).unwrap_or(path).to_path_buf());
                Ok(())
            }
            (false, false) => Ok(()),
        }
    })
}

/// Read and classify every source file under a root (`<root>/src` when a `src` dir exists).
pub fn inventory(root: &Path) -> io::Result<BTreeMap<String, FileReport>> {
    let base = match root.join("src").is_dir() {
        true => root.join("src"),
        false => root.to_path_buf(),
    };
    sources(&base)?.iter().try_fold(BTreeMap::new(), |mut acc, path| {
        let text = fs::read_to_string(base.join(path))?;
        let relative = path.to_string_lossy().to_string();
        acc.insert(relative.clone(), classify(&relative, &text));
        Ok(acc)
    })
}

/// TSV rows: `relative_path\tline_number` for each teachable line.
pub fn rows(items: &BTreeMap<String, FileReport>) -> Vec<String> {
    items
        .values()
        .flat_map(|report| report.teachable.iter().map(move |line| format!("{}\t{line}", report.path)))
        .collect()
}
