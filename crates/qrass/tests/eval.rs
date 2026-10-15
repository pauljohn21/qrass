//! Eval-stage tests (tasks 5.2 + 5.3): variable scoping (shadowing / `!default`
//! / `!global` writeback), ruleset nesting with `&`, unit-aware arithmetic and
//! conversion, selector interpolation, `@media` wrapping and undefined-variable
//! errors. Driven through the public `lex → parse → eval_all` surface; assertions
//! match the deterministic expanded formatting the eval fold emits.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

/// Compile source down to the eval chunk stream joined like render would.
fn css(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
}

fn eval_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.into(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

// ─── 5.2 Scoping: shadowing / !default / !global ──────────────────────────────

#[test]
fn nested_scope_shadows_outer() {
    let out = css("$x: 1; .a { $x: 2; color: $x; } .b { color: $x; }");
    assert!(
        out.contains(".a {\n  color: 2;\n}"),
        ".a uses shadowed 2\n---\n{out}"
    );
    assert!(
        out.contains(".b {\n  color: 1;\n}"),
        ".b sees outer 1 after leave\n---\n{out}"
    );
}

#[test]
fn loop_accumulator_updates_enclosing_local_scope() {
    // A plain `$n: $n + …` inside `@each`/`@for` must update the function-local `$n`
    // (dart-sass propagates within the local scope chain), not shadow it per-iteration.
    let out = css(
        "@function j($list) { $n: x; @each $i in $list { $n: $n + '-' + $i; } @return $n; } \
         .a { p: j((a, b, c)); }",
    );
    assert!(out.contains("p: x-a-b-c;"), "@each accumulator\n---\n{out}");

    let out = css(
        "@function k($hi) { $n: 0; @for $i from 1 through $hi { $n: $n + $i; } @return $n; } \
         .a { p: k(3); }",
    );
    assert!(out.contains("p: 6;"), "@for accumulator\n---\n{out}");
}

#[test]
fn global_flag_writes_back_past_scope() {
    let out = css("$x: 1; .a { $x: 9 !global; } .b { color: $x; }");
    assert!(!out.contains(".a {"), "empty ruleset emits nothing");
    assert!(
        out.contains(".b {\n  color: 9;\n}"),
        "!global propagated\n---\n{out}"
    );
}

#[test]
fn default_only_when_undefined() {
    let out = css("$x: 5 !default; $y: 1; $y: 2 !default; .a { color: $x; background: $y; }");
    assert!(
        out.contains(".a {\n  color: 5;\n  background: 1;\n}"),
        "!default sets $x, leaves already-defined $y\n---\n{out}"
    );
}

// ─── 5.3 Nesting, arithmetic, units, interpolation, media ─────────────────────

#[test]
fn nesting_and_ampersand() {
    let out = css(".a { color: red; .b { color: blue; } &:hover { color: green; } }");
    assert!(
        out.contains(".a {\n  color: red;\n}"),
        "parent own rule\n---\n{out}"
    );
    assert!(
        out.contains(".a .b {\n  color: blue;\n}"),
        "descendant\n---\n{out}"
    );
    assert!(
        out.contains(".a:hover {\n  color: green;\n}"),
        "& substitution\n---\n{out}"
    );
}

#[test]
fn selector_list_cross_product() {
    let out = css("h1, h2 { .x, .y { color: red; } }");
    assert!(out.contains("h1 .x"), "h1 × .x");
    assert!(out.contains("h1 .y"), "h1 × .y");
    assert!(out.contains("h2 .x"), "h2 × .x");
    assert!(out.contains("h2 .y"), "h2 × .y");
}

#[test]
fn multiplication_with_units() {
    let out = css("$x: 10px; .a { width: $x * 2; margin: (1 + 2) * 3px; }");
    assert!(out.contains("width: 20px;"), "10px*2\n---\n{out}");
    assert!(out.contains("margin: 9px;"), "(1+2)*3px\n---\n{out}");
}

#[test]
fn addition_across_convertible_units() {
    // 96px + 1in → 1in converts to 96px → 192px.
    let out = css(".a { width: 96px + 1in; }");
    assert!(
        out.contains("width: 192px;"),
        "px+in conversion\n---\n{out}"
    );
}

#[test]
fn selector_interpolation() {
    let out = css("$n: box; .#{$n} { color: red; }");
    assert!(
        out.contains(".box {\n  color: red;\n}"),
        "#{{n}} in selector\n---\n{out}"
    );
}

#[test]
fn value_interpolation() {
    let out = css("$s: 4; .a { padding: #{$s}px; }");
    assert!(out.contains("padding: 4px;"), "#{{s}}px value\n---\n{out}");
}

#[test]
fn color_and_number_serialization() {
    let out = css(".a { color: #0d6efd; opacity: 0.5; }");
    assert!(
        out.contains("color: #0d6efd;"),
        "hex color passthrough\n---\n{out}"
    );
    assert!(out.contains("opacity: 0.5;"), "unitless number\n---\n{out}");
}

#[test]
fn media_wraps_rules() {
    let out = css("@media screen { .a { color: red; } }");
    assert!(
        out.contains("@media screen {"),
        "media wrapper present\n---\n{out}"
    );
    assert!(
        out.contains("color: red;"),
        "inner declaration preserved\n---\n{out}"
    );
}

#[test]
fn null_value_drops_declaration() {
    let out = css(".a { color: null; width: 10px; }");
    assert!(
        !out.contains("color"),
        "null-valued declaration dropped\n---\n{out}"
    );
    assert!(out.contains("width: 10px;"), "sibling kept\n---\n{out}");
}

#[test]
fn comparison_in_arithmetic_context() {
    // Logical and/or short-circuit to an operand value used directly as a color.
    let out = css(".a { color: true or blue; }");
    assert!(
        out.contains("color: true;"),
        "`or` yields first truthy operand\n---\n{out}"
    );
}

#[test]
fn undefined_variable_errors() {
    let msg = eval_err(".a { color: $nope; }");
    assert!(
        msg.contains("Undefined variable"),
        "expected undefined-variable, got {msg}"
    );
}
