//! hrx ledger harness — runs `sass-spec/*.hrx` cases through the SOLE public exit
//! `sasspile::compile` and emits a `STATUS\tkey[\tdetail]` snapshot for `ledger` A/B.
//! Not a pass/fail assert on individual cases (the compiler is deliberately incomplete);
//! it produces a stable per-case pass/fail set so `ledger diff <base> <run>` yields
//! `regress = 0` as features are wired in. Test scaffolding lives in `tests/` (product
//! code stays in `src/`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sasspile::compile;

/// Directories we treat as the in-scope oracle (exclude non_conformant/* and libsass*).
const SPEC_DIRS: [&str; 6] = [
    "core_functions",
    "directives",
    "values",
    "expressions",
    "css",
    "callable",
];

/// One parsed Hyrax case: keyed by its directory path inside the `.hrx`.
#[derive(Default)]
struct Case {
    input: Option<String>,
    output: Option<String>,
    error: bool,
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
                    "error" => case.error = true,
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
    let _ = tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).try_init();
    let mut files = Vec::new();
    SPEC_DIRS
        .iter()
        .for_each(|d| walk_hrx(&root.join(d), &mut files));

    let mut rows: Vec<String> = Vec::new();
    let (mut total, mut pass, mut skipped) = (0usize, 0usize, 0usize);

    for file in &files {
        let Ok(text) = fs::read_to_string(file) else { continue };
        for (key, case) in parse_hrx(&text) {
            // Runnable only as a single-input, single-output (or single-error) case.
            if case.extra_inputs > 0 || case.has_options || case.input.is_none() {
                skipped += 1;
                continue;
            }
            if !(case.output.is_some() || case.error) {
                skipped += 1;
                continue;
            }
            total += 1;
            let id = format!("{}::{}", file.file_name().unwrap().to_string_lossy(), key);
            tracing::info!(stage = "hrx", id = %id, input_len = case.input.as_ref().map_or(0, |s| s.len()), "case start");
            let result = compile(case.input.unwrap()).await;
            match (result, &case.output, case.error) {
                (Ok(css), Some(expected), false) if norm(&css) == norm(expected) => {
                    pass += 1;
                    rows.push(format!("PASS\t{id}"));
                }
                (Err(_), _, true) => {
                    pass += 1;
                    rows.push(format!("PASS\t{id}"));
                }
                (Ok(_), _, true) => rows.push(format!("FAIL\t{id}\texpected error, got css")),
                (Err(e), _, false) => rows.push(format!("FAIL\t{id}\t{:?}", e.message)),
                (Ok(css), Some(expected), false) => rows.push(format!(
                    "FAIL\t{id}\tgot {:?} want {:?}",
                    norm(&css),
                    norm(expected)
                )),
                _ => rows.push(format!("FAIL\t{id}\tno expected output")),
            }
        }
    }

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hrx-snapshot.tsv");
    fs::create_dir_all(out.parent().unwrap()).ok();
    fs::write(&out, rows.join("\n")).expect("write snapshot");

    // Sanity: the harness found and ran cases. Individual failures are expected (the
    // compiler is incomplete); regress tracking is via `ledger diff`, not this assert.
    assert!(total > 0, "harness found no runnable cases under {root:?}");
    tracing::info!(
        stage = "hrx",
        total,
        pass,
        skipped,
        pass_rate = format!("{:.1}%", 100.0 * pass as f64 / total as f64),
        snapshot = %out.display(),
        "hrx snapshot written"
    );
}
