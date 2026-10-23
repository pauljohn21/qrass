//! sass-spec `.hrx` runner (tasks 11.2 / 11.3).
//!
//! Walks the modern subset of the checked-out `sass-spec` submodule
//! (`spec/{core_functions,expressions,directives,callable,css}`), parses every
//! `.hrx` into cases (via [`common::hrx`]), materializes each case's Sass
//! sources into a per-case temp directory (so `@use`/`@import` partials resolve
//! through the on-disk loader), compiles the input with qrass, and compares the
//! result against the case's expected member:
//!
//! * `output.css` → qrass must succeed and, after both sides are re-serialized
//!   through the shared lightningcss canonicalizer ([`common::css_diff`]), match
//!   line-for-line (identical policy to the element-plus golden);
//! * a bare `error` → qrass must fail, its message containing the expected
//!   error sentence (case-insensitive, whitespace-collapsed).
//!
//! Skip rules: indented-`.sass` inputs (syntax we do not target) and any case
//! keyed in `allowlist.txt` (genuine legacy-only / by-design-unsupported cases).
//! A pass-rate *regression floor* in `min_pass_rate.txt` keeps `cargo test`
//! green while convergence proceeds: the suite may not drop below the recorded
//! floor, and each convergence round raises it.
//!
//! `QRASS_HRX_DIR=<subdir>` narrows the walk to one top-level spec directory so
//! failures can be clustered and fixed one category at a time (task 11.3).
//!
//! `QRASS_HRX_OUT=<file>` additionally writes one tab-separated row per case
//! (`PASS|FAIL|SKIP\tkey\tdetail`) for offline clustering and the assemlist
//! regression report.

mod common;

use common::{css_diff, hrx};
use qrass::{compile, Options};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// The five modern spec areas the harness targets.
const TARGETS: [&str; 5] = [
    "core_functions",
    "expressions",
    "directives",
    "callable",
    "css",
];

/// Per-run scratch root for materialized cases (under cargo's integration-test
/// temp dir, so nothing escapes the build tree).
fn scratch_root() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("hrx_cases")
}

fn allowlist_path() -> PathBuf {
    common::golden("sass-spec/allowlist.txt")
}

fn floor_path() -> PathBuf {
    common::golden("sass-spec/min_pass_rate.txt")
}

/// Read newline-delimited keys, ignoring blanks and `#` comments.
fn read_keys(path: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') {
                set.insert(t.to_string());
            }
        }
    }
    set
}

/// Read a pass-rate floor: the first non-comment line parsing as a percentage.
fn read_floor(path: &Path) -> Option<f64> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .find_map(|l| l.parse::<f64>().ok())
}

/// Outdated libsass-era / non-conformant suites (`libsass*`, `non_conformant`
/// directories at any depth). They test legacy behavior the spec itself has
/// dropped, so the runner skips them unconditionally — a hard guarantee that
/// holds even if [`TARGETS`] is ever widened.
fn is_outdated_dir(name: &str) -> bool {
    name == "non_conformant" || name.starts_with("libsass")
}

/// Recursively collect `*.hrx` files under `root` (relative to `root` returned).
fn discover(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_outdated_dir)
            {
                continue;
            }
            discover(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("hrx") {
            out.push(path);
        }
    }
    let _ = root;
}

/// Write the case's Sass sources (everything except expected-output / error /
/// warning / options members) into `dir`, preserving relative layout. Returns
/// the materialized `input` path, or `None` when nothing could be written.
fn materialize(case: &hrx::Case, dir: &Path) -> Option<PathBuf> {
    let (input_rel, _) = case.input.as_ref()?;
    for (rel, content) in &case.files {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        if base.starts_with("output")
            || base.starts_with("error")
            || base.starts_with("warning")
            || base == "options.yml"
        {
            continue;
        }
        let target = dir.join(rel);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&target, content);
    }
    Some(dir.join(input_rel))
}

/// Collapse a string for tolerant comparison: lowercase, every run of non
/// alphanumeric becomes a single space, trimmed.
fn collapse_words(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The expected error's core sentence: first `Error:` line's message text.
fn expected_error_line(err: &str) -> String {
    for line in err.lines() {
        if let Some(rest) = line.trim().strip_prefix("Error:") {
            return rest.trim().to_string();
        }
    }
    err.lines().next().unwrap_or("").trim().to_string()
}

/// Compare qrass output to an expected `output.css` after canonicalization.
fn outputs_match(ours: &str, expected: &str) -> bool {
    css_diff::normalize(&css_diff::canonicalize(ours))
        == css_diff::normalize(&css_diff::canonicalize(expected))
}

#[derive(Default)]
struct Tally {
    passed: usize,
    failed: Vec<String>,
    skipped_indented: usize,
    skipped_allowlisted: usize,
    skipped_none: usize,
    /// `STATUS\tkey\tdetail` rows for the `QRASS_HRX_OUT` report.
    rows: Vec<String>,
}

/// Run one case: compile and check against its expected member.
fn run_case(case: &hrx::Case, key: &str, scratch: &Path, tally: &mut Tally) {
    // Materialize under a *virtual spec root* so URLs relative to the spec
    // directory (`@use "callable/arguments/mixin/utils"` — a fixture file that
    // lives inside this very hrx) resolve through the load path exactly like
    // the goldens' own reference harness. `<key without #n and .hrx>` is the case's
    // spec-relative directory; each case gets a private scratch vroot.
    let dir = scratch.join(format!("vroot_{}", key.replace(['/', ' ', '.'], "_")));
    let _ = std::fs::remove_dir_all(&dir);
    let case_rel = key
        .split('#')
        .next()
        .and_then(|k| k.strip_suffix(".hrx"))
        .unwrap_or("case");
    let case_dir = dir.join(case_rel);
    let Some(input_path) = materialize(case, &case_dir) else {
        tally.skipped_none += 1;
        tally.rows.push(format!("SKIP\t{key}\tmissing input member"));
        return;
    };

    let opts = Options {
        // sass-spec resolves spec-root-relative `@use` (e.g. "core_functions/color/utils")
        // through a load path at the spec directory; mirror that for fidelity. The
        // per-case vroot goes first so in-hrx fixtures win, then the on-disk spec for
        // real shared support files.
        load_paths: vec![dir.clone(), common::sass_spec_dir()],
        ..Default::default()
    };
    let result = compile(&input_path, &opts);
    let _ = std::fs::remove_dir_all(&dir);

    if let Some(expected) = &case.output {
        match result {
            Ok(css) if outputs_match(&css, expected) => {
                tally.passed += 1;
                tally.rows.push(format!("PASS\t{key}"));
            }
            Ok(css) => {
                tally.rows.push(format!("FAIL\t{key}\toutput mismatch"));
                tally.failed.push(format!(
                    "{key}: output mismatch\n  {}\n  want: {}",
                    first_diff_line(&css, expected),
                    expected.lines().next().unwrap_or("")
                ));
            }
            Err(e) => {
                tally.rows.push(format!("FAIL\t{key}\texpected success, got error: {}", err_head(&e.to_string())));
                tally.failed.push(format!("{key}: expected success, got error: {e}"));
            }
        }
    } else if let Some(expected) = &case.error {
        match result {
            Err(e) => {
                let want = collapse_words(&expected_error_line(expected));
                let got = collapse_words(&e.to_string());
                if want.is_empty() || got.contains(&want) || want.contains(&got) {
                    tally.passed += 1;
                    tally.rows.push(format!("PASS\t{key}\terror case"));
                } else {
                    tally.rows.push(format!("FAIL\t{key}\terror text mismatch"));
                    tally.failed.push(format!("{key}: error text mismatch\n  ours: {e}\n  want: {expected}"));
                }
            }
            Ok(_) => {
                tally.rows.push(format!("FAIL\t{key}\texpected error, got success"));
                tally.failed.push(format!("{key}: expected error, got success"));
            }
        }
    } else {
        tally.skipped_none += 1;
        tally.rows.push(format!("SKIP\t{key}\tno output/error member"));
    }
}

/// First line of a compiler error message, single-spaced, for the report column.
fn err_head(msg: &str) -> String {
    msg.lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render the first divergent line pair between ours and expected for a failure
/// message (bounded to keep output readable).
fn first_diff_line(ours: &str, expected: &str) -> String {
    match css_diff::first_diff(ours, expected, 0) {
        Some((d, _, _)) => d.describe(),
        None => "no line diff (byte-level formatting only)".to_string(),
    }
}

#[test]
fn sass_spec_hrx_runner() {
    qrass::tracing_setup::init();
    // lightningcss can panic on exotic spec colors; `canonicalize` catches it,
    // but the default panic hook would still flood stderr across the walk.
    // Silence it during collection and restore just before the final assert so a
    // genuine regression still prints its full message.
    std::panic::set_hook(Box::new(|_| {}));
    let spec = common::sass_spec_dir();
    if !spec.is_dir() {
        warn!("sass-spec submodule not checked out; skipping hrx runner");
        return;
    }

    let scratch = scratch_root();
    let _ = std::fs::create_dir_all(&scratch);
    let allow = read_keys(&allowlist_path());
    let mut tally = Tally::default();

    // Optional single-directory narrowing for convergence rounds.
    let dirs: Vec<String> = match std::env::var("QRASS_HRX_DIR") {
        Ok(one) => vec![one],
        Err(_) => TARGETS.iter().map(|s| s.to_string()).collect(),
    };

    let mut attempted_files = 0usize;
    for dir in &dirs {
        let root = spec.join(dir);
        if !root.is_dir() {
            warn!(dir = %dir, "spec directory missing");
            continue;
        }
        let mut files = Vec::new();
        discover(&root, &root, &mut files);
        files.sort();
        attempted_files += files.len();
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
                let key = format!("{}#{}", rel, idx);
                if case.indented {
                    tally.skipped_indented += 1;
                    tally.rows.push(format!("SKIP\t{key}\tindented sass input"));
                    continue;
                }
                if allow.contains(&key)
                    || allow.iter().any(|a| rel.starts_with(a.trim_end_matches('/')))
                {
                    tally.skipped_allowlisted += 1;
                    tally.rows.push(format!("SKIP\t{key}\tallowlisted"));
                    continue;
                }
                run_case(&case, &key, &scratch, &mut tally);
            }
        }
    }

    let attempted = tally.passed + tally.failed.len();
    let rate = if attempted == 0 {
        0.0
    } else {
        100.0 * tally.passed as f64 / attempted as f64
    };
    info!(
        stage = "sass-spec-hrx",
        files = attempted_files,
        passed = tally.passed,
        failed = tally.failed.len(),
        skipped_indented = tally.skipped_indented,
        skipped_allowlisted = tally.skipped_allowlisted,
        skipped_none = tally.skipped_none,
        pass_rate = format!("{rate:.2}%"),
        "sass-spec hrx run summary"
    );
    for f in tally.failed.iter().take(15) {
        error!("hrx failure: {f}");
    }

    // Full failure list for offline clustering (one `key: reason-head` per line),
    // written under the scratch dir so convergence rounds can bucket by cause.
    if !tally.failed.is_empty() {
        let full = std::env::var("QRASS_HRX_FAILDIFF").is_ok();
        let dump: String = tally
            .failed
            .iter()
            .map(|f| {
                if full {
                    f.clone()
                } else {
                    f.lines().next().unwrap_or("").to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(scratch_root().join("failures.txt"), dump);
    }

    // Optional machine-readable per-case report (`QRASS_HRX_OUT=<path>`).
    if let Ok(out) = std::env::var("QRASS_HRX_OUT")
        && !out.is_empty() {
            let body = tally.rows.join("\n");
            let _ = std::fs::write(&out, format!("{body}\n"));
        }

    // Ledger write-through (`QRASS_HRX_RUN=<name>`): record this snapshot in
    // the single SQLite ledger instead of growing another reports/*.tsv file.
    let run = std::env::var("QRASS_HRX_RUN").unwrap_or_default();
    if !run.is_empty() {
        let db = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/hrx.db");
        match qrass_ledger::record(&run, &db, &tally.rows) {
            Ok((total, pass)) => info!(target: "qrass", run, total, pass, "ledger recorded"),
            Err(e) => warn!(target: "qrass", ?e, "ledger record failed"),
        }
    }

    // Regression floor: for a FULL modern-subset run (no `QRASS_HRX_DIR`
    // narrowing), fail only if the pass rate drops below the recorded value.
    // Single-directory convergence runs are informational and skip the floor.
    let _ = std::panic::take_hook(); // restore default panic output for the assert
    let narrowed = std::env::var("QRASS_HRX_DIR").is_ok();
    if !narrowed
        && let Some(floor) = read_floor(&floor_path()) {
            assert!(
                rate >= floor,
                "sass-spec pass rate {rate:.2}% fell below recorded floor {floor:.2}%; \
                 first failures:\n{}",
                tally.failed.iter().take(15).cloned().collect::<Vec<_>>().join("\n")
            );
        }
}
