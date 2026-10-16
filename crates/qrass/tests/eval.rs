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
    // (per the goldens, updates propagate within the local scope chain), not shadow it per-iteration.
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

// ─── Modern `if()` clause syntax (expressions/if/*.hrx goldens) ──────────────

#[test]
fn modern_if_css_clauses_render_back() {
    let out = css("a {b: if(css(1) and css(2): c; else: d)}");
    assert!(
        out.contains("b: if(css(1) and css(2): c; else: d);"),
        "css clauses must re-render\n---\n{out}"
    );
    // Structural whitespace normalizes (`;else` -> `; else`).
    let out = css("a {b: if(css(): c ;else: d)}");
    assert!(
        out.contains("b: if(css(): c; else: d);"),
        "whitespace normalized\n---\n{out}"
    );
}

#[test]
fn modern_if_sass_evaluates_with_short_circuit() {
    let out = css("a {b: if(sass(true): c; else: d)}");
    assert!(out.contains("b: c;"), "true arm\n---\n{out}");
    let out = css("a {b: if(sass(false): c; else: d)}");
    assert!(out.contains("b: d;"), "false falls to else\n---\n{out}");
    // A definitely-true first arm never evaluates later clauses.
    let out = css("a {b: if(sass(true): c; sass($undefined): d)}");
    assert!(out.contains("b: c;"), "later clause skipped\n---\n{out}");
    // No matching arm and no else: the declaration drops (null value).
    let out = css("a {b: if(sass(false): c); d: 1px}");
    assert!(
        !out.contains("b:") && out.contains("d: 1px;"),
        "null if() drops the declaration\n---\n{out}"
    );
}

#[test]
fn modern_if_mixed_simplification_drops_true_terms() {
    // sass(true) vanishes from the and-chain, css terms stay symbolic.
    let out = css("a {b: if(css(1) and sass(true) and css(2): c; else: d)}");
    assert!(
        out.contains("b: if(css(1) and css(2): c; else: d);"),
        "true term dropped\n---\n{out}"
    );
    // A false term decides the clause: fall through to the Sass else arm.
    let out = css("a {b: if(css(1) and sass(false) and css(2): c; else: d)}");
    assert!(out.contains("b: d;"), "false kills clause\n---\n{out}");
}

#[test]
fn modern_if_adjacent_var_terms_survive_verbatim() {
    let out = css("a {b: if(css(1) var(--and) css(2): c)}");
    assert!(
        out.contains("b: if(css(1) var(--and) css(2): c);"),
        "adjacency preserved\n---\n{out}"
    );
}

#[test]
fn modern_if_interpolates_only_hash_runs() {
    let out = css("a {b: if(css(#{1 + 2}px): c)}");
    assert!(
        out.contains("b: if(css(3px): c);"),
        "interpolation evaluated, text verbatim\n---\n{out}"
    );
}

#[test]
fn legacy_if_builtin_still_works() {
    // A top-level comma keeps the three-argument builtin call intact.
    let out = css("a {b: if(true, 1px, 2px)}");
    assert!(out.contains("b: 1px;"), "legacy if()\n---\n{out}");
    let out = css("a {b: if(false, 1px, 2px)}");
    assert!(out.contains("b: 2px;"), "legacy else arg\n---\n{out}");
}
