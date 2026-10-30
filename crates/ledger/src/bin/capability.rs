//! `ledger capability <run>` — **which capability blocks which rows**, counted from the corpus instead of
//! guessed from the defect I happened to be touching.
//!
//! Why this tool exists: the failure distribution by *directory* (`core_functions/color` = 5,732 rows) says
//! where the red is, but not *why*, so a session keeps picking its next increment out of the last needle it
//! ran and re-litigating local fixes. This walks every failing key of one named run, pulls that case's own
//! `input.scss` out of the `.hrx` archive it came from, and buckets it by the feature predicates below.
//!
//! Two numbers per feature, because one without the other misleads: **rows** (the input mentions the feature
//! at all — what a session could unblock by building it, upper bound) and **sole** (the input mentions *only*
//! that feature — a lower bound on what is blocked by nothing else). Sorting by `rows` ranks the size of the
//! prize; sorting by `sole` ranks what actually gates a row today.
//!
//! The archive is the only oracle: an unresolved key is counted and reported rather than dropped silently.

use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One predicate and what it names. The strings are matched against the case's input text; `error golden`
/// against the ledger's own failure detail, because a row whose expectation is an error message is blocked by
/// the error work, not by the feature its input spells.
fn predicates() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "module: local @use",
            vec![
                "@use \"core_functions",
                "@use 'core_functions",
                "@use \"sass-spec",
            ],
        ),
        ("module: sass: @use", vec!["@use \"sass:", "@use 'sass:"]),
        ("module: @forward", vec!["@forward"]),
        ("module: @import", vec!["@import"]),
        ("mixin: @include", vec!["@include"]),
        ("mixin: @mixin def", vec!["@mixin "]),
        ("function: @function", vec!["@function"]),
        ("control: @each", vec!["@each"]),
        ("control: @while", vec!["@while"]),
        ("control: @if/@else", vec!["@if ", "@else"]),
        ("control: @extend", vec!["@extend"]),
        ("control: @media", vec!["@media"]),
        ("control: @supports", vec!["@supports"]),
        ("control: @content", vec!["@content"]),
        ("control: @at-root", vec!["@at-root"]),
        (
            "color: Color-4 spaces",
            vec![
                "color(",
                "to-space",
                "from-space",
                "to-gamut",
                "is-in-gamut",
                "$space:",
                "oklab(",
                "oklch(",
                "lab(",
                "lch(",
                "color.hwb",
            ],
        ),
        ("args: rest arglist", vec!["..."]),
        ("interp: #{", vec!["#{"]),
        ("calc: calc()", vec!["calc("]),
        ("ns: map.", vec!["map."]),
        ("ns: math.", vec!["math."]),
        ("ns: meta.", vec!["meta."]),
        ("ns: selector.", vec!["selector."]),
        ("ns: string.", vec!["string."]),
        ("ns: list.", vec!["list."]),
        ("ns: color.", vec!["color."]),
        ("ns: function.", vec!["function."]),
        ("ns: mixed.", vec!["mixed."]),
        ("value: custom property", vec!["--"]),
        ("value: url/var fallback", vec!["var("]),
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let usage = || tracing::info!("usage: ledger capability <run> [--db <path>]");
    let (db, run) = match argv.split_first() {
        // The run name is positional and `--db` takes its value in the next argument, matching how the
        // `ledger` binary itself is invoked (`--db` with a space, not `--db=`).
        Some((run, rest)) => {
            let explicit = rest
                .iter()
                .position(|arg| arg == "--db")
                .and_then(|index| rest.get(index + 1));
            if !rest.is_empty() && explicit.is_none() {
                usage();
                return Ok(());
            }
            (
                explicit.map_or_else(|| PathBuf::from(ledger::DB), PathBuf::from),
                run.clone(),
            )
        }
        None => {
            usage();
            return Ok(());
        }
    };
    let spec = Path::new("sass-spec/spec");
    let conn = Connection::open(&db)?;
    let rows = ledger::failing_any(&conn, &run, &[])?;
    let mut hits: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut sole: BTreeMap<&'static str, usize> = BTreeMap::new();
    let (mut unresolved, mut classified) = (0usize, 0usize);

    for row in &rows {
        let key = row.split(" | ").next().unwrap_or("").to_string();
        let detail = row.splitn(2, " | ").nth(1).unwrap_or("").to_lowercase();
        let Some(input) = input_of(&key, spec) else {
            unresolved += 1;
            continue;
        };
        classified += 1;
        let matched = predicates()
            .into_iter()
            .filter(|(name, needles)| {
                if *name == "error golden (P7)" {
                    detail.starts_with("expected error")
                } else {
                    needles.iter().any(|needle| input.contains(needle))
                }
            })
            .collect::<Vec<_>>();
        for (name, _) in &matched {
            *hits.entry(name).or_default() += 1;
        }
        if matched.len() == 1 {
            *sole.entry(matched[0].0).or_default() += 1;
        }
    }

    // The error expectation is its own bucket: a row that wants `Error: …` cannot be landed by implementing
    // the syntax its input spells, so it must not be counted as a syntax prize.
    let errors = rows
        .iter()
        .filter(|row| {
            row.splitn(2, " | ")
                .nth(1)
                .unwrap_or("")
                .to_lowercase()
                .starts_with("expected error")
        })
        .count();
    *hits.entry("error golden (P7)").or_default() = errors;

    let mut ranked = hits.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    tracing::info!(
        run = %run,
        failing = rows.len(),
        resolved = classified,
        unresolved = unresolved,
        "capability buckets"
    );
    for (name, count) in &ranked {
        tracing::info!(
            capability = %name,
            rows = count,
            sole = sole.get(name).copied().unwrap_or_default(),
            "rows / sole"
        );
    }
    Ok(())
}

/// The case's own input text: a key is `<path under spec>::<case name>`, and inside the archive the section
/// header is `<===> <case name>/input.scss`. Returns `None` when either half cannot be found, which the
/// caller reports as `unresolved` rather than dropping the row quietly.
fn input_of(key: &str, spec: &Path) -> Option<String> {
    let (path, case) = key.split_once("::")?;
    let file = spec.join(format!("{path}.hrx"));
    let text = std::fs::read_to_string(file).ok()?;
    let header = format!("<===> {case}/input.scss");
    let start = text.lines().position(|line| line == header)?;
    Some(
        text.lines()
            .skip(start + 1)
            .take_while(|line| !line.starts_with("<===>"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}
