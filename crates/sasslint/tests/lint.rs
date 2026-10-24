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

#[test]
fn mcp_protocol_handshake_and_review() {
    use serde_json::json;
    use sasslint::handle_message;

    let init = handle_message(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})).unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "sasslint");

    let list = handle_message(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    assert_eq!(list["result"]["tools"][0]["name"], "review");

    assert!(handle_message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());

    let dir = std::env::temp_dir().join("sasslint_mcp_probe");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.rs"), "pub children: Vec<Rule>\n").unwrap();
    let call = handle_message(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "review", "arguments": { "path": dir.to_str().unwrap() } }
    }))
    .unwrap();
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("no-tree"), "review tool missed violation: {text}");
    fs::remove_dir_all(&dir).ok();
}
