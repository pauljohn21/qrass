//! End-to-end compile tests for the flat-stream core (rebuild §4a: nesting + variables +
//! interpolation + globals + at-rule transparency).

use sasspile::compile_string as cs;

#[test]
fn flat_rule() {
    assert_eq!(cs("a { b: c; }").unwrap(), "a {\n  b: c;\n}\n");
}

#[test]
fn nesting_combines_selectors() {
    assert_eq!(cs("a { b { c: d; } }").unwrap(), "a b {\n  c: d;\n}\n");
    assert_eq!(
        cs("a { color: red; &:hover { color: blue; } }").unwrap(),
        "a {\n  color: red;\n}\na:hover {\n  color: blue;\n}\n"
    );
}

#[test]
fn variables_bind_and_substitute() {
    assert_eq!(cs("$x: 3px; a { w: $x; }").unwrap(), "a {\n  w: 3px;\n}\n");
}

#[test]
fn interpolation_glues() {
    assert_eq!(
        cs("$k: 2; .col-#{$k} { w: 1px; }").unwrap(),
        ".col-2 {\n  w: 1px;\n}\n"
    );
    // undefined variable inside interpolation drops the declaration
    assert_eq!(cs("a { b: #{$nope}; }").unwrap(), "");
}

#[test]
fn multiple_rules_document_order() {
    assert_eq!(
        cs("a { x: 1; } b { y: 2; }").unwrap(),
        "a {\n  x: 1;\n}\nb {\n  y: 2;\n}\n"
    );
}
