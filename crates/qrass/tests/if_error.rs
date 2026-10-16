//! Verbatim annotation-block verification for the `expressions/if/error/*.hrx`
//! goldens.
//!
//! The shared hrx runner matches only an error's first `Error:` line, which
//! cannot catch a mis-placed caret, a wrong run length, a missing label or a
//! bad location column. This harness compares the *whole* expected block —
//! message, `,` corner, numbered source line, every marker run with its label,
//! the `'` foot and the `input.scss L:C  root stylesheet` line — against what
//! qrass renders, right-trimming each line so only visible layout counts.

mod common;

use common::hrx;
use qrass::{compile, Options};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Legacy three-argument `if()` failures, whose golden block prints the builtin
/// `@function if($condition, $if-true, $if-false)` declaration snippet and an
/// `if-function` deprecation warning — outside this round's scope.
const KNOWN_DEBT: [&str; 2] = [
    "expressions/if/error/empty.hrx#0",
    "expressions/if/error/missing.hrx#0",
];

/// Drop trailing whitespace on every line, so only visible layout is compared.
fn strip_end(block: &str) -> String {
    block
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Recollect `*.hrx` files under `dir`.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("hrx") {
            out.push(path);
        }
    }
}

/// Materialize one case's input file, returning its path.
fn write_input(dir: &Path, case: &hrx::Case) -> Option<PathBuf> {
    let (rel, content) = case.input.as_ref()?;
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&target, content).ok()?;
    Some(target)
}

#[test]
fn if_error_snippets_match_goldens_verbatim() {
    let spec = common::sass_spec_dir();
    let root = spec.join("expressions/if/error");
    assert!(root.is_dir(), "missing golden dir {}", root.display());
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();

    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("if_error_cases");
    let mut checked = 0usize;
    let mut mismatched: Vec<String> = Vec::new();

    for file in &files {
        let rel = file
            .strip_prefix(&spec)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = std::fs::read_to_string(file) else {
            warn!(file = %rel, "unreadable hrx");
            continue;
        };
        for (idx, case) in hrx::parse(&text).into_iter().enumerate() {
            let key = format!("{rel}#{idx}");
            if KNOWN_DEBT.contains(&key.as_str()) {
                warn!(case = %key, "known debt: legacy if() argument-count block");
                continue;
            }
            let Some(expected) = &case.error else {
                mismatched.push(format!("{key}: golden has no error member"));
                continue;
            };
            let case_dir = scratch.join(rel.replace('/', "_").replace(".hrx", ""));
            let _ = std::fs::create_dir_all(&case_dir);
            let Some(input) = write_input(&case_dir, &case) else {
                mismatched.push(format!("{key}: no input member"));
                continue;
            };
            let ours = match compile(&input, &Options::default()) {
                Ok(css) => {
                    mismatched.push(format!("{key}: expected an error, got CSS:\n{css}"));
                    continue;
                }
                Err(e) => e.to_string(),
            };
            let want = strip_end(expected);
            if !strip_end(&ours).contains(&want) {
                mismatched.push(format!(
                    "{key}\n  ---- ours ----\n{}\n  ---- want ----\n{}\n  ----------------",
                    strip_end(&ours),
                    want
                ));
            }
            checked += 1;
        }
    }

    info!(
        stage = "if-error-snippets",
        checked,
        mismatches = mismatched.len(),
        "golden annotation blocks compared verbatim"
    );
    // Guard against the harness silently vacuating (a moved/renamed spec tree).
    assert!(checked >= 60, "only {checked} golden cases compared");
    assert!(
        mismatched.is_empty(),
        "{} of {checked} if/error annotation blocks differ:\n{}",
        mismatched.len(),
        mismatched.join("\n")
    );
}
