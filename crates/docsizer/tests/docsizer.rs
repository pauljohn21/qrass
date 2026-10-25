//! Integration tests for `docsizer`: every gate the tutorial is graded on must itself be tested
//! on a fixture whose answer is known by hand.

use std::collections::BTreeMap;

use docsizer::inventory::Exclusions;
use docsizer::{plan, words};

/// 16 lines; the `#[cfg(test)]` item must be excluded as a unit, not line by line.
const FIXTURE: &str = concat!(
    "use std::fmt;\n", //  1 pure use
    "pub struct S;\n", //  2 teachable
    "impl S {\n",      //  3 teachable
    "    fn f(&self) {}\n", //  4 brace-only? no: it has text -> teachable
    "\n",              //  5 blank
    "    /// doc\n",   //  6 teachable (comments are teaching material)
    "    fn g(&self)\n", //  7 teachable
    "    {\n",         //  8 brace only
    "        1;\n",    //  9 teachable
    "    }\n",         // 10 brace only
    "}\n",             // 11 brace only
    "#[cfg(test)]\n",  // 12 test module header
    "mod tests {\n",   // 13 test module (waiting depth opens here)
    "    #[test]\n",   // 14 test
    "    fn t() {}\n", // 15 test
    "}\n",             // 16 test closes
);

#[test]
fn test_module_lines_are_excluded_as_a_block() {
    let report = docsizer::classify_text("s.rs", FIXTURE);
    let excl = Exclusions { test_module: 5, blank: 1, pure_use: 1, brace_only: 3 };
    assert_eq!(report.excl, excl, "每一类排除都要有名字，分母才可审计");
    assert_eq!(report.teachable, vec![2, 3, 4, 6, 7, 9]);
    assert_eq!(report.lines, 16);
}

#[test]
fn plan_then_coverage_reports_a_clean_ledger() {
    let report = docsizer::classify_text("s.rs", FIXTURE);
    let mut items = BTreeMap::new();
    items.insert("s.rs".to_string(), report);
    let map = plan::parse_map("V1\t10\ts.rs");
    let segments = plan::plan(&items, &map, 3, 600);
    let ledger = plan::rows(&segments).join("\n");

    let check = docsizer::check_against(&items, &ledger);
    assert!(check.clean(), "unassigned={:?} duplicates={:?}", check.unassigned, check.duplicates);
    assert_eq!(check.teachable, check.assigned);
    assert_eq!(segments.len(), 2, "6 teachable lines at target 3 => two spans");
    assert_eq!(
        segments.iter().map(|s| s.lesson.as_str()).collect::<Vec<_>>(),
        vec!["V1-01", "V1-02"],
        "overflowing the lesson budget opens the next lesson instead of merging"
    );
}

#[test]
fn unassigned_and_duplicate_ownership_are_both_caught() {
    let report = docsizer::classify_text("s.rs", FIXTURE);
    let mut items = BTreeMap::new();
    items.insert("s.rs".to_string(), report);

    let missing = docsizer::check_against(&items, "s.rs\t2-3\tV1-01\t主");
    assert_eq!(missing.unassigned, vec!["s.rs:4", "s.rs:6", "s.rs:7", "s.rs:9"]);

    let doubled = docsizer::check_against(&items, "s.rs\t2-9\tV1-01\t主\ns.rs\t2-9\tV1-02\t主");
    assert_eq!(doubled.unassigned, Vec::<String>::new());
    assert_eq!(doubled.duplicates.len(), 6, "every doubly owned line is reported");

    // A `复习` row revisits material without claiming ownership, so it never creates a duplicate.
    let revision = docsizer::check_against(&items, "s.rs\t2-9\tV1-01\t主\ns.rs\t2-9\tV2-01\t复习");
    assert!(revision.clean());
}

#[test]
fn code_and_tables_do_not_count_as_prose() {
    let doc = "# 标题\n\n讲解文字。\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
    let count = words::count_markdown("d.md", doc);
    // Counted: `# 标题` (3) + `讲解文字。` (5) + `| a | b |` (5) + `| 1 | 2 |` (5).
    // Not counted: blank lines, the ``` fence markers, the code inside them, and the
    // `|---|---|` table separator row.
    assert_eq!(count.prose, 3 + 5 + 5 + 5);
    assert!(count.total > count.prose, "代码块与表分隔行必须不计入正文");
}
