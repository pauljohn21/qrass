//! hrx ledger harness — runs `sass-spec/*.hrx` cases through the SOLE public exit
//! `sasspile::compile` and emits a `STATUS\tkey[\tdetail]` snapshot for `ledger` A/B.
//! Not a pass/fail assert on individual cases (the compiler is deliberately incomplete);
//! it produces a stable per-case pass/fail set so `ledger diff <base> <run>` yields
//! `regress = 0` as features are wired in. Test scaffolding lives in `tests/` (product
//! code stays in `src/`).
//!
//! Two environment knobs shape what a run touches:
//! * `HRX_RUN=<name>` (+ optional `HRX_DB=<path>`) records the verdict set into the ledger;
//! * `HRX_CASE=<key substring>` narrows the run to the cases whose ledger key contains it — the way to
//!   look at **one** case's trace, because 11,533 spans are not a diagnostic view. It is mutually
//!   exclusive with `HRX_RUN`: a partial measurement written into the record would pose as a comparable
//!   run total (see the guard below).

#[path = "common/tracing_bridge.rs"]
mod tracing_bridge;

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
async fn hrx_run() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sass-spec/spec");
    let _span = tracing::info_span!("hrx").entered();
    tracing_bridge::init();
    // The single-case selector, read once. `Option<str>` so the loop below can ask the same question
    // without a special case: `None` measures everything.
    let only = std::env::var("HRX_CASE").ok();
    let selected = |id: &str| {
        only.as_ref()
            .is_none_or(|needle| id.contains(needle.as_str()))
    };
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
            let id = case_id(&root, file, &key);
            // A case outside `HRX_CASE` is not measured: it counts toward neither `total` nor the rows,
            // so a filtered run cannot be mistaken for a full one by anything reading the numbers.
            if !selected(&id) {
                continue;
            }
            total += 1;
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

    // Every failure as an event too. The record (`ledger failures <run>`) stays the durable read, but under
    // `trace` the got/want pair should show up where it happened, next to the spans that produced it —
    // which is how a case is diagnosed without re-deriving the oracle by hand.
    rows.iter().for_each(|row| {
        let (status, rest) = row.split_once('\t').unwrap_or(("?", ""));
        if status == "FAIL" {
            let (id, detail) = rest.split_once('\t').unwrap_or((rest, ""));
            tracing::warn!(stage = "hrx", id, verdict = detail, "case failed");
        }
    });

    // The record is the ledger database, never a file. `HRX_RUN=<name>` writes this run straight into
    // `reports/hrx.db` (`HRX_DB=<path>` aims it at another database, which is how the mandated fresh-empty
    // A/B re-check is done); without `HRX_RUN` the harness only measures and logs, leaving no artefact.
    // This replaces the old `target/hrx-snapshot.tsv` + `ledger import <file>` hop, which forced every A/B
    // to hold two files — and made me rebuild the “before” side with `git stash`, the maneuver that once
    // collided with `index.lock` and left a stale number in the transcript.
    //
    // A filtered run and a recorded run cannot coexist. `run.total` is what makes `ledger diff` mean
    // "the same population, twice"; storing a 1-case measurement under a run name would hand the next
    // A/B a number that looks comparable and is not, which is the exact failure mode this project has
    // been bitten by repeatedly. Diagnose with `HRX_CASE`, record with the full run.
    assert!(
        !(only.is_some() && std::env::var("HRX_RUN").is_ok()),
        "HRX_CASE selects a subset for diagnosis and must not be combined with HRX_RUN, which would \
         record a partial measurement as a comparable run (run.total would stop meaning the whole \
         population). Drop HRX_CASE to record, or drop HRX_RUN to trace."
    );
    let recorded = std::env::var("HRX_RUN").ok().map(|name| {
        let db = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(std::env::var("HRX_DB").unwrap_or_else(|_| "../../reports/hrx.db".into()));
        let (stored_total, stored_pass) = ledger::record(&name, &db, &rows)
            .unwrap_or_else(|e| panic!("ledger record {name}: {e}"));
        // The database is now the only record, so what it stored must equal what was measured: a silent
        // mismatch would make every later `ledger diff` compare the wrong sets.
        assert_eq!(
            stored_total,
            rows.len(),
            "ledger stored a different total for {name}"
        );
        assert_eq!(
            stored_pass, pass,
            "ledger stored a different pass count for {name}"
        );
        name
    });

    // Sanity: the harness found and ran cases. Individual failures are expected (the
    // compiler is incomplete); regress tracking is via `ledger diff`, not this assert. With `HRX_CASE`
    // this doubles as the positive control a selector needs: a typo would otherwise report "0 of 0"
    // and read as an empty, harmless run.
    assert!(
        total > 0,
        "harness found no runnable cases{} under {root:?}",
        only.as_ref()
            .map(|needle| format!(" matching HRX_CASE {needle:?}"))
            .unwrap_or_default()
    );
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
        // What a selector narrowed the population to, so a partial run cannot be read as a whole one.
        selector = only.as_deref().unwrap_or("<none>"),
        // The number a round must actually move: passes earned against a non-empty expected output.
        pass_substantive = pass - trivial_empty,
        pass_empty_expectation = trivial_empty,
        skipped,
        pass_rate = format!("{:.1}%", 100.0 * pass as f64 / total as f64),
        recorded_run = recorded.as_deref().unwrap_or("<not recorded>"),
        "hrx run measured"
    );
}
