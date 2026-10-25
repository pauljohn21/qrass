//! docsizer — read-only accounting for the "write rxrust from scratch" tutorial.
//!
//! Four questions, four subcommands, all answered from files rather than from memory:
//! - `inventory` — how many lines of the reference crate are *teachable*, and what was excluded
//!   and why (so the denominator is auditable);
//! - `plan` / `coverage` — does the lesson ledger own every teachable line exactly once;
//! - `words` — how much prose (non-code characters) the tutorial actually contains;
//! - `terms` — can a reader with no background follow it, i.e. is every piece of Rust jargon in
//!   the prose defined in the glossary *at or before* the lesson that uses it (word count and a
//!   green build cannot answer this; a user's "I understood none of it" can).

pub mod coverage;
pub mod inventory;
pub mod plan;
pub mod terms;
pub mod words;

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::inventory::{FileReport, classify, inventory as scan, rows};

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args.first().map(String::as_str) {
        Some("inventory") => inventory_cmd(args),
        Some("plan") => plan_cmd(args),
        Some("coverage") => coverage_cmd(args),
        Some("words") => words_cmd(args),
        Some("terms") => terms_cmd(args),
        _ => {
            info!("usage: docsizer inventory|plan|coverage|words|terms [--src P] [--out P] [--map P] [--ledger P] [--dir P] [--glossary P] [--min N] [--max-new N] [--target N] [--max N] [--strict]");
            Ok(())
        }
    }
}

/// `inventory --src <rxrust-src> [--out target/rxrust-inventory.tsv]`
fn inventory_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let src = need(args, "--src")?;
    let items = scan(&src)?;
    let summary = items.values().fold(
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize),
        |(files, lines, test, blank, uses, braces), report| {
            (
                files + 1,
                lines + report.lines,
                test + report.excl.test_module,
                blank + report.excl.blank,
                uses + report.excl.pure_use,
                braces + report.excl.brace_only,
            )
        },
    );
    let out = flag(args, "--out").unwrap_or_else(|| PathBuf::from("target/rxrust-inventory.tsv"));
    let body = rows(&items);
    write(&out, &body)?;
    info!(
        files = summary.0,
        lines = summary.1,
        teachable = body.len(),
        excl_test_module = summary.2,
        excl_blank = summary.3,
        excl_pure_use = summary.4,
        excl_brace_only = summary.5,
        out = %out.display(),
        "docsizer inventory"
    );
    Ok(())
}

/// `plan --src <rxrust-src> --map MAP.tsv [--out COVERAGE.tsv] [--target 300] [--max 600]`
///
/// The default `300` is what puts the reference crate inside the curriculum's 45–60 lesson band
/// (measured: 14,116 teachable lines → 58 lessons).
fn plan_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let items = scan(&need(args, "--src")?)?;
    let map_text = fs::read_to_string(flag(args, "--map").ok_or("--map is required")?)?;
    let target = num(args, "--target", 300);
    let max = num(args, "--max", 600);
    let segments = plan::plan(&items, &plan::parse_map(&map_text), target, max);
    let unmapped: Vec<&String> = items
        .keys()
        .filter(|path| !map_text.lines().any(|line| line.ends_with(path.as_str())))
        .collect();
    let out = flag(args, "--out").unwrap_or_else(|| PathBuf::from("docs/rxrust-from-scratch/COVERAGE.tsv"));
    write(&out, &plan::rows(&segments))?;
    info!(
        lessons = segments.iter().map(|s| s.lesson.as_str()).collect::<std::collections::HashSet<_>>().len(),
        spans = segments.len(),
        unmapped_files = unmapped.len(),
        out = %out.display(),
        "docsizer plan"
    );
    unmapped
        .iter()
        .for_each(|path| warn!(file = %path, "in the reference source but not assigned to a volume"));
    Ok(())
}

/// `coverage --src <rxrust-src> --ledger COVERAGE.tsv [--strict]`
fn coverage_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let items = scan(&need(args, "--src")?)?;
    let ledger = fs::read_to_string(flag(args, "--ledger").ok_or("--ledger is required")?)?;
    let report = coverage::check(&items, &coverage::parse_ledger(&ledger));
    let rate = 100.0 * report.assigned as f64 / report.teachable.max(1) as f64;
    info!(
        teachable = report.teachable,
        assigned = report.assigned,
        coverage = format!("{rate:.2}%"),
        unassigned = report.unassigned.len(),
        duplicates = report.duplicates.len(),
        "docsizer coverage"
    );
    report.unassigned.iter().take(40).for_each(|line| warn!(line = %line, "unassigned teachable line"));
    report.duplicates.iter().take(40).for_each(|line| warn!(line = %line, "line owned by more than one 主 row"));
    match strict(args) && !report.clean() {
        true => Err("coverage gate failed".into()),
        false => Ok(()),
    }
}

/// `words --dir docs/rxrust-from-scratch [--min 1000000] [--strict]`
fn words_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let dir = flag(args, "--dir").unwrap_or_else(|| PathBuf::from("docs/rxrust-from-scratch"));
    let counts = words::tally(&dir)?;
    let totals = counts.iter().fold((0usize, 0usize), |acc, count| (acc.0 + count.total, acc.1 + count.prose));
    let min = num(args, "--min", 1_000_000);
    info!(
        files = counts.len(),
        total_chars = totals.0,
        prose_chars_non_code = totals.1,
        required_min = min,
        dir = %dir.display(),
        "docsizer words"
    );
    counts.iter().for_each(|count| info!(file = %count.file, prose = count.prose, "docsizer words: file"));
    match strict(args) && totals.1 < min {
        true => Err("word gate failed".into()),
        false => Ok(()),
    }
}

/// `terms --dir docs/rxrust-from-scratch --glossary GLOSSARY.md [--max-new 2] [--strict]`
///
/// 这是“读者能不能跟上”的闸：不是量我写了多少，而是量我有没有拿未定义的行话去解释未知。
fn terms_cmd(args: &[String]) -> Result<(), Box<dyn Error>> {
    let dir = flag(args, "--dir").unwrap_or_else(|| PathBuf::from("docs/rxrust-from-scratch"));
    let glossary_path = flag(args, "--glossary").unwrap_or_else(|| dir.join("GLOSSARY.md"));
    let glossary = fs::read_to_string(glossary_path)?;
    let max_new = num(args, "--max-new", 2);
    let report = terms::check(&dir, &glossary, max_new)?;
    info!(
        lessons = report.lessons,
        defined_terms = report.defined_terms,
        max_new_per_lesson = max_new,
        undefined_uses = report.undefined.len(),
        over_budget = report.over_budget.len(),
        dir = %dir.display(),
        "docsizer terms"
    );
    report.undefined.iter().take(30).for_each(|hit| warn!(use_site = %hit, "行话在正文里用了，却没在同一课或更早定义"));
    report.over_budget.iter().take(10).for_each(|hit| warn!(lesson = %hit, "一节课引入的新术语超预算"));
    match strict(args) && !report.clean() {
        true => Err("readability gate failed".into()),
        false => Ok(()),
    }
}

fn flag(args: &[String], name: &str) -> Option<PathBuf> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(PathBuf::from)
}

fn need(args: &[String], name: &str) -> Result<PathBuf, Box<dyn Error>> {
    flag(args, name).ok_or(format!("{name} is required").into())
}

fn num(args: &[String], name: &str, default: usize) -> usize {
    flag(args, name).and_then(|p| p.to_string_lossy().parse::<usize>().ok()).unwrap_or(default)
}

fn strict(args: &[String]) -> bool {
    args.iter().any(|a| a == "--strict")
}

fn write(path: &Path, body: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(path, body.join("\n") + "\n")
}

/// Classify a single in-memory file — the seam the integration tests drive.
pub fn classify_text(path: &str, text: &str) -> FileReport {
    classify(path, text)
}

/// Inventory from an in-memory map — the seam the integration tests drive.
pub fn check_against(items: &BTreeMap<String, FileReport>, ledger: &str) -> coverage::Report {
    coverage::check(items, &coverage::parse_ledger(ledger))
}
