//! Golden-shaped behaviour for the first vertical slice. Formats match the
//! `sass-spec` expanded CSS contract (e.g. `css/plain` `& {a: b}` → `& {\n  a: b;\n}`).

use sasspile::compile_string;

#[test]
fn single_rule_ancestor_selector() {
    assert_eq!(compile_string("& {a: b}").unwrap(), "& {\n  a: b;\n}\n");
}

#[test]
fn multiple_decls() {
    assert_eq!(
        compile_string(".foo { a: b; c: d; }").unwrap(),
        ".foo {\n  a: b;\n  c: d;\n}\n"
    );
}

#[test]
fn multiple_rules_concatenate() {
    assert_eq!(
        compile_string("a { x: 1; }\nb { y: 2; }").unwrap(),
        "a {\n  x: 1;\n}\nb {\n  y: 2;\n}\n"
    );
}

#[test]
fn descendant_nesting() {
    assert_eq!(
        compile_string("a { b { c: d; } }").unwrap(),
        "a b {\n  c: d;\n}\n"
    );
}

#[test]
fn parent_reference_nesting() {
    assert_eq!(
        compile_string(".a { &:hover { color: red; } }").unwrap(),
        ".a:hover {\n  color: red;\n}\n"
    );
}

#[test]
fn parent_decls_precede_children() {
    assert_eq!(
        compile_string("a { x: 1; b { y: 2; } }").unwrap(),
        "a {\n  x: 1;\n}\na b {\n  y: 2;\n}\n"
    );
}

#[test]
fn deep_nesting() {
    assert_eq!(
        compile_string("a { b { c { d: e; } } }").unwrap(),
        "a b c {\n  d: e;\n}\n"
    );
}

#[test]
fn if_true_branch_emitted() {
    assert_eq!(
        compile_string("@if true { a { b: c; } }").unwrap(),
        "a {\n  b: c;\n}\n"
    );
}

#[test]
fn if_false_branch_dropped() {
    assert_eq!(compile_string("@if false { a { b: c; } }").unwrap(), "");
}

#[test]
fn if_else_picks_taken_branch() {
    assert_eq!(
        compile_string("@if false { x { y: 1; } } @else { z { w: 2; } }").unwrap(),
        "z {\n  w: 2;\n}\n"
    );
}

#[test]
fn if_variable_condition() {
    assert_eq!(
        compile_string("$f: false; @if $f { a { b: c; } } @else { d { e: f; } }").unwrap(),
        "d {\n  e: f;\n}\n"
    );
}

#[test]
fn global_variable_substitution() {
    assert_eq!(
        compile_string("$x: 3px; a { w: $x; }").unwrap(),
        "a {\n  w: 3px;\n}\n"
    );
}

#[test]
fn block_variable_not_emitted() {
    assert_eq!(
        compile_string("a { $x: red; color: $x; }").unwrap(),
        "a {\n  color: red;\n}\n"
    );
}

#[test]
fn variable_used_in_nested_child() {
    assert_eq!(
        compile_string(".c { $p: 1px; b { m: $p; } }").unwrap(),
        ".c b {\n  m: 1px;\n}\n"
    );
}
