//! Tests for the `sasslint` rule engine.

use std::fs;
use std::path::PathBuf;

use sasslint::{Finding, scan_dir, scan_file};

fn has(findings: &[Finding], rule: &str) -> bool {
    findings.iter().any(|f| f.rule == rule)
}

#[test]
fn detects_forbidden_patterns() {
    let path = std::env::temp_dir().join("sasslint_probe.rs");
    fs::write(
        &path,
        "struct Rule { pub children: Vec<Rule> }\nfn resolve_all(&self, r: Vec<Rule>) -> Vec<Rule> { }\nfn subst() { let _ = stmt_text(); }\nloop { let _ = 1; }\n",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(has(&f, "no-tree"), "tree/children not caught: {f:?}");
    assert!(has(&f, "no-relay-vec"), "relay Vec<Rule> not caught: {f:?}");
    assert!(has(&f, "no-string-machine"), "string machine not caught: {f:?}");
    assert!(has(&f, "no-recursive-descent"), "recursion not caught: {f:?}");
    assert!(has(&f, "no-imperative-loop"), "loop not caught: {f:?}");
    fs::remove_file(&path).ok();
}

#[test]
fn ignores_comments() {
    let path = std::env::temp_dir().join("sasslint_comment.rs");
    fs::write(&path, "// Vec<Rule> mentioned in a doc comment\nlet x = 1;\n").unwrap();
    assert!(scan_file(&path).is_empty(), "comment line should not be flagged");
    fs::remove_file(&path).ok();
}

/// CI gate: `sasspile/src` must be free of iron-rule violations. Currently `#[ignore]`d
/// because the pipeline still carries the tree/recursion/string-machine debt; remove the
/// `#[ignore]` once the flat-stream refactor lands, making a re-introduction fail CI.
#[test]
#[ignore = "pending flat-stream refactor of sasspile core"]
fn gate_sasspile_is_clean() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&root);
    assert!(findings.is_empty(), "sasspile has rule violations: {findings:#?}");
}
