//! Control-flow expansion tests (task 7.3): `@if`/`@else`, `@each` over lists and
//! maps (with `$k, $v` destructuring), `@for` with `to`/`through`, and `@while`
//! with a body-updated counter. Each verifies both statement-level expansion and
//! the eager-capture requirement (loop variables visible to inline declarations
//! and nested rules within the same iteration).

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn css(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
}

#[test]
fn if_else_picks_truthy_branch() {
    let out = css("$x: 2; .a { @if $x > 1 { color: red; } @else { color: blue; } }");
    assert_eq!(out, ".a {\n  color: red;\n}", "{out}");
}

#[test]
fn else_if_chain() {
    let out =
        css("$x: 0; .a { @if $x > 1 { p: big; } @else if $x == 0 { p: zero; } @else { p: neg; } }");
    assert!(out.contains("p: zero;"), "{out}");
    assert!(!out.contains("big") && !out.contains("neg"), "{out}");
}

#[test]
fn each_list_expands_rules() {
    let out = css("@each $c in red blue green { .#{$c} { color: $c; } }");
    assert!(out.contains(".red {\n  color: red;\n}"), "{out}");
    assert!(out.contains(".blue {\n  color: blue;\n}"), "{out}");
    assert!(out.contains(".green {\n  color: green;\n}"), "{out}");
}

#[test]
fn each_map_destructures_key_value() {
    let out = css("@each $k, $v in (a: 1, b: 2) { .#{$k} { width: $v; } }");
    assert!(out.contains(".a {\n  width: 1;\n}"), "{out}");
    assert!(out.contains(".b {\n  width: 2;\n}"), "{out}");
}

#[test]
fn each_inline_declaration_captures_index() {
    // A declaration directly under @each (no nested rule) folds into the parent.
    let out = css(".list { @each $i in 1 2 3 { content: #{$i}; } }");
    assert_eq!(
        out, ".list {\n  content: 1;\n  content: 2;\n  content: 3;\n}",
        "{out}"
    );
}

#[test]
fn for_to_is_exclusive() {
    let out = css("@for $i from 1 to 4 { .m#{$i} { width: $i; } }");
    assert!(
        out.contains(".m1 {") && out.contains(".m2 {") && out.contains(".m3 {"),
        "{out}"
    );
    assert!(!out.contains(".m4"), "{out}");
}

#[test]
fn for_through_is_inclusive() {
    let out = css("@for $i from 1 through 3 { .c#{$i} { z: $i; } }");
    assert!(
        out.contains(".c1 {") && out.contains(".c2 {") && out.contains(".c3 {"),
        "{out}"
    );
    assert!(!out.contains(".c4"), "{out}");
}

#[test]
fn while_counter_terminates() {
    let out = css("$i: 3; .w { @while $i > 0 { n: #{$i}; $i: $i - 1; } }");
    assert_eq!(out, ".w {\n  n: 3;\n  n: 2;\n  n: 1;\n}", "{out}");
}
