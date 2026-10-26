//! hrx ledger harness — runs `sass-spec/*.hrx` cases through the SOLE public exit
//! `sasspile::compile` and emits a `STATUS\tkey[\tdetail]` snapshot for `ledger` A/B.
//! Not a pass/fail assert on individual cases (the compiler is deliberately incomplete);
//! it produces a stable per-case pass/fail set so `ledger diff <base> <run>` yields
//! `regress = 0` as features are wired in. Test scaffolding lives in `tests/` (product
//! code stays in `src/`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sasspile::compile;

/// The in-scope oracle directories. Excluded by policy: `non_conformant` (documents behaviour the
/// spec itself calls non-conformant), `libsass*` (legacy engine issues), and `helpers` (not tests).
///
/// `operators`, `parser` and `variables` were missing here until P1.3, although the change's own task
/// 1.6 cites `operators/**` as its golden source — 79 cases were never measured, which is how a
/// coverage gap turns into an unfalsifiable "regress=0". Adding them is a scope change: the total moves,
/// and the acceptance rule is that no previously-covered case may move from PASS to FAIL.
const SPEC_DIRS: [&str; 9] = [
    "core_functions",
    "directives",
    "values",
    "expressions",
    "css",
    "callable",
    "operators",
    "parser",
    "variables",
];

/// One parsed Hyrax case: keyed by its directory path inside the `.hrx`.
#[derive(Default)]
struct Case {
    input: Option<String>,
    output: Option<String>,
    /// the case's `error` section, verbatim (`None` when the case expects CSS)
    error: Option<String>,
    extra_inputs: usize,
    has_options: bool,
}

fn walk_hrx(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_hrx(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("hrx") {
                out.push(path);
            }
        }
    }
}

/// Split a `.hrx` into `(case_key, Case)`. A section is `<===> <path>` up to the next
/// `<===>`; the bare `<===>` + `====` separator carries no path and is ignored.
fn parse_hrx(text: &str) -> BTreeMap<String, Case> {
    let mut cases: BTreeMap<String, Case> = BTreeMap::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        match line.strip_prefix("<===>") {
            Some(rest) => {
                let path = rest.trim();
                i += 1;
                let mut body = String::new();
                while i < lines.len() && !lines[i].starts_with("<===>") {
                    body.push_str(lines[i]);
                    body.push('\n');
                    i += 1;
                }
                if path.is_empty() || path.starts_with("====") {
                    continue; // group separator
                }
                let (key, file) = split_case(path);
                let case = cases.entry(key).or_default();
                match file {
                    "input.scss" => {
                        if case.input.is_some() {
                            case.extra_inputs += 1;
                        }
                        case.input = Some(body);
                    }
                    "output.css" => case.output = Some(body),
                    "error" => case.error = Some(body),
                    "options.yml" => case.has_options = true,
                    _ => {
                        if file.ends_with(".scss") {
                            case.extra_inputs += 1;
                        }
                    }
                }
            }
            None => i += 1,
        }
    }
    cases
}

/// `unitless/to_unit/input.scss` → (`unitless/to_unit`, `input.scss`).
fn split_case(path: &str) -> (String, &str) {
    match path.rsplit_once('/') {
        Some((dir, file)) => (dir.to_string(), file),
        None => (String::new(), path),
    }
}

/// The ledger key for one case: its path **relative to `spec/`** (`.hrx` dropped) plus the case
/// directory inside the file.
///
/// It used to be `<basename>.hrx::<case>`, and that silently merged different cases: the corpus has
/// `css/plain/function.hrx`, `css/function.hrx`, `css/supports/syntax/function.hrx`, … all folding
/// onto `function.hrx::…`. `ledger` keys a run by case, so a merged pair lets one file's PASS hide
/// the other's FAIL — measured: `ledger diff` reported `regress=0 net=266` where a per-case set diff
/// was `+385 / -3`. Any conclusion about regressions therefore has to come from unique keys.
fn case_id(root: &Path, file: &Path, key: &str) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file).with_extension("");
    format!("{}::{}", rel.display(), key)
}

/// The oracle message for a failing case: the first non-empty line of its `error` section with the
/// leading `Error:` removed — the corpus states the message on that line and puts the source span
/// underneath (`css/comment.hrx::error/loud/interpolation/failure`).
///
/// A section that does not open with `Error:` (a `DEPRECATION WARNING` or `Warning:`) has no message
/// we can produce yet; P7 7.2 brings diagnostics, so such a case must count as a mismatch. This is what
/// makes an `error` case a real assertion: before it, the harness passed on **any** error at all, and
/// measured 385 of 385 flipping `FATAL` "gained" cases whose oracle was an unrelated message.
fn oracle_header(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    line.strip_prefix("Error:")
        .map(|message| message.trim().to_string())
}

/// Normalise CSS for comparison: trim each line, drop blank lines, join with `\n`.
fn norm(css: &str) -> String {
    css.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn hrx_snapshot() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sass-spec/spec");
    let _span = tracing::info_span!("hrx").entered();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let mut files = Vec::new();
    SPEC_DIRS
        .iter()
        .for_each(|d| walk_hrx(&root.join(d), &mut files));

    let mut rows: Vec<String> = Vec::new();
    let (mut total, mut pass, mut skipped) = (0usize, 0usize, 0usize);
    // Of the passes, how many were earned against an **empty** expected output. They are legitimately
    // correct (Sass really emits nothing for e.g. `@if true {}`), but they are also what a compiler that
    // produces nothing scores, so they must never be read as evidence that a feature works — measured at
    // 176 of 635 overall, and 39 of 39 in `directives/{{if,for,while}}` before loops/branches existed.
    let mut trivial_empty = 0usize;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut collision: Vec<String> = Vec::new();

    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (key, case) in parse_hrx(&text) {
            // Runnable only as a single-input, single-output (or single-error) case.
            if case.extra_inputs > 0 || case.has_options || case.input.is_none() {
                skipped += 1;
                continue;
            }
            if !(case.output.is_some() || case.error.is_some()) {
                skipped += 1;
                continue;
            }
            total += 1;
            let id = case_id(&root, file, &key);
            if !seen.insert(id.clone()) {
                collision.push(id.clone());
            }
            tracing::info!(stage = "hrx", id = %id, input_len = case.input.as_ref().map_or(0, |s| s.len()), "case start");
            let result = compile(case.input.unwrap()).await;
            let want_error = case.error.as_deref().and_then(oracle_header);
            match (result, &case.output, want_error) {
                (Ok(css), Some(expected), None) if norm(&css) == norm(expected) => {
                    pass += 1;
                    if norm(expected).is_empty() {
                        trivial_empty += 1;
                    }
                    rows.push(format!("PASS\t{id}"));
                }
                // An error case is satisfied only by the message the oracle states, never by any error.
                (Err(e), _, Some(want)) if e.message == want => {
                    pass += 1;
                    rows.push(format!("PASS\t{id}"));
                }
                (Err(e), _, Some(want)) => rows.push(format!(
                    "FAIL\t{id}\tgot error {:?} want {:?}",
                    e.message, want
                )),
                (Err(e), _, None) => {
                    rows.push(format!("FAIL\t{id}\tunexpected error {:?}", e.message))
                }
                (Ok(css), Some(expected), None) => rows.push(format!(
                    "FAIL\t{id}\tgot {:?} want {:?}",
                    norm(&css),
                    norm(expected)
                )),
                (Ok(css), _, Some(want)) => rows.push(format!(
                    "FAIL\t{id}\texpected error {:?}, got {:?}",
                    want,
                    norm(&css)
                )),
                (Ok(_), None, None) => rows.push(format!("FAIL\t{id}\tno expected output")),
            }
        }
    }

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hrx-snapshot.tsv");
    fs::create_dir_all(out.parent().unwrap()).ok();
    fs::write(&out, rows.join("\n")).expect("write snapshot");

    // Sanity: the harness found and ran cases. Individual failures are expected (the
    // compiler is incomplete); regress tracking is via `ledger diff`, not this assert.
    assert!(total > 0, "harness found no runnable cases under {root:?}");
    // The guard against the defect above: two cases sharing a key makes `ledger diff` able to hide a
    // regression, so the run is invalid rather than merely noisy.
    assert!(
        collision.is_empty(),
        "{} duplicate case keys, first few: {:?}",
        collision.len(),
        collision.iter().take(5).collect::<Vec<_>>()
    );
    tracing::info!(
        stage = "hrx",
        total,
        pass,
        // The number a round must actually move: passes earned against a non-empty expected output.
        pass_substantive = pass - trivial_empty,
        pass_empty_expectation = trivial_empty,
        skipped,
        pass_rate = format!("{:.1}%", 100.0 * pass as f64 / total as f64),
        snapshot = %out.display(),
        "hrx snapshot written"
    );
}
