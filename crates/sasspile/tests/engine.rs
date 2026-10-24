//! Isolated acceptance tests for the new flat-stream `engine` (not yet wired into
//! `compile_string`). They mirror the golden outputs pinned in `compile.rs`, proving the
//! engine reproduces behaviour before cutover.

use sasspile::engine::compile_engine as ce;

#[test]
fn flat_nesting_and_vars() {
    assert_eq!(ce("$x: 3px; a { w: $x; }").unwrap(), "a {\n  w: 3px;\n}\n");
    assert_eq!(
        ce("a { color: red; &:hover { color: blue; } b { c: d; } }").unwrap(),
        "a {\n  color: red;\n}\na:hover {\n  color: blue;\n}\na b {\n  c: d;\n}\n"
    );
    assert_eq!(ce("a { b { c { d: e; } } }").unwrap(), "a b c {\n  d: e;\n}\n");
}

#[test]
fn control_flow() {
    assert_eq!(ce("@if true { a { b: c; } }").unwrap(), "a {\n  b: c;\n}\n");
    assert_eq!(ce("@if false { a { b: c; } }").unwrap(), "");
    assert_eq!(
        ce("@if false { x { y: 1; } } @else { z { w: 2; } }").unwrap(),
        "z {\n  w: 2;\n}\n"
    );
    assert_eq!(
        ce("$f: false; @if $f { a { b: c; } } @else { d { e: f; } }").unwrap(),
        "d {\n  e: f;\n}\n"
    );
    assert_eq!(
        ce("@each $a in b, c { .#{$a} { d: $a; } }").unwrap(),
        ".b {\n  d: b;\n}\n.c {\n  d: c;\n}\n"
    );
    assert_eq!(
        ce("@for $i from 1 to 3 { .c#{$i} { w: $i; } }").unwrap(),
        ".c1 {\n  w: 1;\n}\n.c2 {\n  w: 2;\n}\n"
    );
}

#[test]
fn interp_and_arith() {
    assert_eq!(
        ce("$k: 2; .col-#{$k} { w: 10px + 5px; }").unwrap(),
        ".col-2 {\n  w: 15px;\n}\n"
    );
    // at-rules other than control-flow (@media/@function) stay ordinary blocks —
    // must match the current pipeline byte-for-byte
    let media = "@media screen { a { b: c; } }";
    assert_eq!(ce(media).unwrap(), sasspile::compile_string(media).unwrap());
}

#[test]
fn error_propagation() {
    assert!(ce("a { b: c;").is_err());
}
