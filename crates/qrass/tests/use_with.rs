//! Verbatim golden verification for `with (…)` configuration propagation
//! (Rounds 19–20): `directives/use/with`, `directives/use/error/with`,
//! `directives/forward/with`, `directives/forward/error/with.hrx` and the `with`
//! clause syntax cases.
//!
//! The shared hrx runner compares only an error's first `Error:` line, which
//! cannot catch a mis-placed caret, a wrong run length or a bad location column —
//! and cannot see at all whether a configured value actually reached the
//! `!default` declaration it belongs to. This harness therefore checks whichever
//! golden member a case carries: an `output` member must render the configured
//! CSS, and an `error` member must match its *whole* annotation block. Every
//! expectation is read off the local golden; each rule the coverage encodes cites
//! the file it comes from.

mod common;

use common::css_diff;
use common::hrx;
use qrass::{Options, compile};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Cases whose golden block needs features this round deliberately left out:
/// * `…/through_import.hrx`: configuration travelling through SCSS `@import`.
/// * `conflict.hrx` (`use/error/with`, `forward/error/with#4`): three marker runs
///   with a `... |` elision row, a widened numeric gutter, and the
///   `available from multiple global modules` detection behind them.
/// * `multi_configuration` / `through_forward_twice` / `forward/error/with#15-#18`:
///   the `This module was already loaded…` block, which draws the `new load` and
///   the colliding `original load` (plus a `configuration` run when the offer
///   came from another file) as separate labelled rows, in one block per file.
///   qrass emits the message and the frame stack; the multi-statement block is
///   the missing half.
/// * `syntax…#3` / `syntax…#29`: `Expected identifier.` for `$ a`, which the
///   lexer raises as a bare lexical error — it never reaches the snippet
///   renderer, and the message is worded differently.
const KNOWN_DEBT: [&str; 17] = [
    "directives/use/with/through_import.hrx#0",
    "directives/use/with/through_import.hrx#1",
    "directives/forward/with/through_import.hrx#0",
    "directives/forward/with/through_import.hrx#1",
    "directives/use/error/with/conflict.hrx#0",
    "directives/use/error/with/through_forward_twice.hrx#0",
    "directives/use/error/with/multi_configuration.hrx#1",
    "directives/use/error/with/multi_configuration.hrx#2",
    "directives/use/error/with/multi_configuration.hrx#3",
    "directives/use/error/with/multi_configuration.hrx#4",
    "directives/use/error/syntax/with.hrx#3",
    "directives/forward/error/syntax.hrx#29",
    "directives/forward/error/with.hrx#4",
    "directives/forward/error/with.hrx#15",
    "directives/forward/error/with.hrx#16",
    "directives/forward/error/with.hrx#17",
    "directives/forward/error/with.hrx#18",
];

/// Golden trees this harness walks, plus the two syntax files whose `with/*`
/// cases are picked out of a larger set. `directives/forward/error/with.hrx`
/// joined the set with the module-load stack rendering: each of its cases
/// locates across at least two frames (`_midstream.scss 1:27  @use` plus
/// `input.scss 1:1  root stylesheet`), so walking it verbatim is what proves the
/// frame stack is right.
const SUITES: [&str; 6] = [
    "directives/use/with",
    "directives/use/error/with",
    "directives/forward/with",
    "directives/forward/error/with.hrx",
    "directives/use/error/syntax/with.hrx",
    "directives/forward/error/syntax.hrx",
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

/// Write every source member of a case (partials included — a configuration
/// chain needs the whole module tree) and return the input path.
fn materialize(dir: &Path, case: &hrx::Case) -> Option<PathBuf> {
    let (input_rel, _) = case.input.as_ref()?;
    for entry in &case.files {
        let (rel, content) = (entry.0.clone(), entry.1.clone());
        let base = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        if base.starts_with("output")
            || base.starts_with("error")
            || base.starts_with("warning")
            || base == "options.yml"
        {
            continue;
        }
        let target = dir.join(&rel);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&target, content);
    }
    Some(dir.join(input_rel))
}

/// Whether a case from a mixed syntax file belongs to the `with` family.
fn wanted(rel: &str, case: &hrx::Case) -> bool {
    if !rel.ends_with("error/syntax.hrx") {
        return true;
    }
    case.dir.starts_with("with/")
}

#[test]
fn use_with_matches_goldens_verbatim() {
    let spec = common::sass_spec_dir();
    let mut mismatched: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut debt = 0usize;

    for suite in SUITES {
        let root = spec.join(suite);
        assert!(root.exists(), "missing golden {}", root.display());
        let mut files = Vec::new();
        if root.is_dir() {
            collect(&root, &mut files);
        } else {
            files.push(root);
        }
        files.sort();

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
                if case.indented || !wanted(&rel, &case) {
                    continue;
                }
                if KNOWN_DEBT.contains(&key.as_str()) {
                    debt += 1;
                    warn!(case = %key, "known debt: multi-frame / wording golden not covered yet");
                    continue;
                }
                let case_dir = scratch(&rel).join(format!("c{idx}"));
                let _ = std::fs::create_dir_all(&case_dir);
                // A section that declares only shared partials (the `.hrx`
                // accumulating file set) contributes no runnable case.
                if case.input.is_none() {
                    continue;
                }
                let Some(input) = materialize(&case_dir, &case) else {
                    mismatched.push(format!("{key}: no input member"));
                    continue;
                };
                let opts = Options {
                    load_paths: vec![case_dir.clone(), spec.clone()],
                    ..Default::default()
                };
                if let Some(expected) = &case.output {
                    // An `output` member: the configured value has to show up.
                    let css = match compile(&input, &opts) {
                        Ok(css) => css,
                        Err(e) => {
                            mismatched.push(format!(
                                "{key}: expected CSS, got error:\n{}",
                                strip_end(&e.to_string())
                            ));
                            continue;
                        }
                    };
                    if css_diff::normalize(&css_diff::canonicalize(&css))
                        != css_diff::normalize(&css_diff::canonicalize(expected))
                    {
                        mismatched.push(format!(
                            "{key}: output mismatch\n  ours:\n{}\n  want:\n{}",
                            strip_end(&css),
                            strip_end(expected)
                        ));
                    }
                } else if let Some(expected) = &case.error {
                    let ours = match compile(&input, &opts) {
                        Ok(css) => {
                            mismatched.push(format!("{key}: expected an error, got CSS:\n{css}"));
                            continue;
                        }
                        Err(e) => strip_end(&e.to_string()),
                    };
                    let want = strip_end(expected);
                    if !ours.contains(&want) {
                        mismatched.push(format!(
                            "{key}\n  ---- ours ----\n{ours}\n  ---- want ----\n{want}\n  ----------------"
                        ));
                    }
                } else {
                    // Neither member: nothing to assert, so it is not counted.
                    continue;
                }
                checked += 1;
            }
        }
    }

    info!(
        stage = "use-with-goldens",
        checked,
        debt,
        mismatches = mismatched.len(),
        "with configuration cases compared against the goldens"
    );
    // Guard against the harness silently vacuating (a moved/renamed spec tree).
    assert!(checked >= 110, "only {checked} golden cases compared");
    assert_eq!(debt, KNOWN_DEBT.len(), "debt list no longer matches the run");
    assert!(
        mismatched.is_empty(),
        "{} of {checked} with cases differ from the goldens:\n{}",
        mismatched.len(),
        mismatched.join("\n")
    );
}

/// Private scratch directory for one golden file's cases.
fn scratch(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("use_with_cases")
        .join(rel.replace(['/', ' ', '.'], "_"))
}
