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
