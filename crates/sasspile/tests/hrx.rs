//! sasspile hrx golden runner.
//!
//! Compiles every `sass-spec` `.hrx` case through [`sasspile::compile_string`] and
//! records a PASS/FAIL snapshot into the SQLite **ledger**, so A/B regressions fall
//! out of `ledger diff <a> <b>`. The only behavior authority is the golden
//! `output.css` in each hrx — never dart-sass source/docs.
//!
//! Skipped in ordinary `cargo test`; run a snapshot with:
//!   SASSPILE_HRX_RUN=<run-name> cargo test -p sasspile --test hrx -- --nocapture
//! Optionally narrow the tree with SASSPILE_HRX_DIR=<path> (default: sass-spec/spec).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::info;
use tracing_subscriber::EnvFilter;

use sasspile::compile_string;
use sasspile::engine::compile_engine;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Recursively collect `*.hrx` files under `dir`, skipping the out-of-scope
/// `non_conformant` and `libsass*` trees (the modern suite excludes them).
fn hrx_files(dir: &Path) -> Vec<PathBuf> {
    fn excluded(path: &Path) -> bool {
        path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == "non_conformant" || s.starts_with("libsass")
        })
    }
    let mut stack = vec![dir.to_path_buf()];
    let mut out = Vec::new();
    while let Some(d) = stack.pop() {
        if excluded(&d) {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "hrx") {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

/// Split an `.hrx` document into sections; each section is `(relpath, content)`
/// blocks. Sections are separated by a bare `<===>` line or a line of >=40 `=`.
fn parse_sections(text: &str) -> Vec<Vec<(String, String)>> {
    let mut sections = Vec::new();
    let mut cur: Vec<(String, String)> = Vec::new();
    let mut name: Option<String> = None;
    let mut buf = String::new();
    let close_block = |name: &mut Option<String>, buf: &mut String, cur: &mut Vec<(String, String)>| {
        if let Some(n) = name.take() {
            cur.push((n, std::mem::take(buf)));
        }
    };
    for line in text.split('\n') {
        let t = line.trim_end_matches('\r');
        let sep = t == "<===>" || (t.len() >= 40 && t.bytes().all(|b| b == b'='));
        if sep {
            close_block(&mut name, &mut buf, &mut cur);
            if !cur.is_empty() {
                sections.push(std::mem::take(&mut cur));
            }
        } else if let Some(rest) = t.strip_prefix("<===> ") {
            close_block(&mut name, &mut buf, &mut cur);
            name = Some(rest.trim().to_string());
        } else if name.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    close_block(&mut name, &mut buf, &mut cur);
    if !cur.is_empty() {
        sections.push(cur);
    }
    sections
}

#[test]
fn hrx_ab_baseline() {
    let Some(run) = std::env::var("SASSPILE_HRX_RUN").ok() else {
        return; // skipped unless a snapshot name is supplied
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .with_test_writer()
        .try_init()
        .ok();

    let root = repo_root();
    let spec = root.join("sass-spec/spec");
    let dir = std::env::var("SASSPILE_HRX_DIR").map(PathBuf::from).unwrap_or(spec);
    let engine = std::env::var("SASSPILE_ENGINE").is_ok();

    let mut rows: Vec<String> = Vec::new();
    for file in hrx_files(&dir) {
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&file).unwrap_or_default();
        for (i, section) in parse_sections(&text).into_iter().enumerate() {
            let map: HashMap<String, String> = section.into_iter().collect();
            // a case is any section that declares an input.scss (possibly section-prefixed)
            let Some((input_key, input)) = map.iter().find(|(k, _)| k.ends_with("input.scss")) else {
                continue;
            };
            let prefix = &input_key[..input_key.len() - "input.scss".len()];
            let want = map.get(&format!("{prefix}output.css"));
            let key = format!("{rel}#{i}");
            match (if engine { compile_engine(input.clone()) } else { compile_string(input.clone()) }, want) {
                (Ok(got), Some(want)) if got.trim_end() == want.trim_end() => {
                    rows.push(format!("PASS\t{key}"))
                }
                (Ok(got), Some(want)) => rows.push(format!(
                    "FAIL\t{}\tgot={:?} want={:?}",
                    key,
                    got.trim_end(),
                    want.trim_end()
                )),
                (Ok(_), None) => rows.push(format!("FAIL\t{key}\tno expected output.css")),
                (Err(e), _) => rows.push(format!("FAIL\t{}\t{}", key, e.message)),
            }
        }
    }

    let db = root.join(ledger::DB);
    fs::create_dir_all(db.parent().unwrap()).ok();
    let (total, pass) = ledger::record(&run, &db, &rows).expect("record ledger snapshot");
    info!(run = run.as_str(), total, pass, "sasspile hrx baseline recorded");
}
