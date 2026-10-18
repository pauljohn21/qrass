//! Plain-CSS function passthrough (task 10): calls Sass doesn't define (`var`,
//! `calc`, transforms, gradients, …) are emitted verbatim, matching the goldens, so
//! element-plus's CSS-variable usage survives. Inside a passthrough call Sass math
//! is *disabled* — binary/unary operators stay literal — while interpolation
//! (`#{…}`), variables and Sass function calls are still resolved.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

/// Compile `src` and return the serialized value of declaration `p`.
fn value(expr: &str) -> String {
    let src = format!(".a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src, program)
        .expect("eval ok")
        .join("\n");
    let rest = out
        .split_once("p: ")
        .unwrap_or_else(|| panic!("no declaration emitted for {expr:?}: {out}"))
        .1;
    rest.split_once(";\n").map(|x| x.0).unwrap_or(rest).trim().to_string()
}

#[test]
fn var_custom_property_passes_through() {
    assert_eq!(value("var(--el-color-primary)"), "var(--el-color-primary)");
}

#[test]
fn calc_operators_are_not_evaluated() {
    // Sass would reject `100% - 32px` (incompatible units); in calc it stays literal.
    assert_eq!(value("calc(100% - 32px)"), "calc(100% - 32px)");
    assert_eq!(value("calc(50% + 10px)"), "calc(50% + 10px)");
}

#[test]
fn calc_resolves_interpolation_and_keeps_operator() {
    let src = "$w: 6px; .a { p: calc(100% - #{$w}); }";
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n");
    assert!(out.contains("calc(100% - 6px)"), "{out}");
}

#[test]
fn calc_evaluates_sass_function_inside() {
    // getCssVar-style user function must resolve (not pass through verbatim), while
    // the surrounding calc operators stay literal.
    let src = "@function cssvar($n) { @return var(#{$n}); } .a { p: calc((0px - cssvar(--x)) / 2); }";
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n");
    assert!(out.contains("calc((0px - var(--x)) / 2)"), "{out}");
}

#[test]
fn transform_and_gradient_pass_through() {
    assert_eq!(value("translateY(4px)"), "translateY(4px)");
    assert_eq!(
        value("linear-gradient(90deg, red, blue)"),
        "linear-gradient(90deg, red, blue)"
    );
}

#[test]
fn space_separated_transform_list_passes_through() {
    // Two CSS functions separated by a space are a plain value list.
    assert_eq!(
        value("rotate(45deg) translateY(-4px)"),
        "rotate(45deg) translateY(-4px)"
    );
}

#[test]
fn rgb_space_alpha_syntax_is_computed_not_passed_through() {
    // Modern CSS color syntax: the goldens evaluate it to a Sass color and
    // serializes the alpha, rather than emitting `rgb(… / …%)` verbatim.
    assert_eq!(value("rgb(0 0 0 / 15%)"), "rgba(0, 0, 0, 0.15)");
    assert_eq!(value("rgb(0 0 0 / 6%)"), "rgba(0, 0, 0, 0.06)");
}

// ─── Special-function name normalization (round 32) ───────────────────────────
// dart lowercases every *special* plain-CSS function name and, only for `url`,
// also strips the leading vendor prefix. A `url($var)` argument instead makes
// dart treat it as an ordinary function: the name stays verbatim.

#[test]
fn special_calc_lowercases_and_keeps_vendor_prefix() {
    assert_eq!(value("-A-CALC(0)"), "-a-calc(0)");
    assert_eq!(value("-WEBKIT-CALC(1px + 2px)"), "-webkit-calc(1px + 2px)");
}

#[test]
fn special_url_lowercases_and_strips_vendor_prefix() {
    assert_eq!(value("-C-URL(0)"), "url(0)");
    assert_eq!(value("-WEBKIT-URL(x)"), "url(x)");
}

#[test]
fn url_with_variable_arg_stays_verbatim() {
    // A `$var` argument demotes `url` to an ordinary call: name kept verbatim,
    // argument resolved.
    let src = "$a: b; .a { p: -E-URL($a); }";
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n");
    assert!(out.contains("p: -E-URL(b)"), "{out}");
}

#[test]
fn non_special_call_keeps_source_spelling() {
    // Unknown functions are not in the special set: name stays exactly as typed.
    assert_eq!(value("-A-FOO(1 + 1)"), "-A-FOO(1 + 1)");
}

// ─── Unknown at-rule passthrough (round 37) ───────────────────────────────────
// Any `@name` Sass does not recognize is echoed verbatim (`css/unknown_directive`,
// `css/supports/syntax/*`): the value's tokens pass through untouched, only `#{…}`
// is evaluated, and a block body is grouped like a container.

/// Compile a whole sheet and return the trimmed CSS output.
fn sheet(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
        .trim()
        .to_string()
}

#[test]
fn unknown_atrule_top_level_is_echoed_verbatim() {
    assert_eq!(sheet("@a b"), "@a b;");
    assert_eq!(sheet("@a b;"), "@a b;");
    // An empty-value directive emits just `@name;`.
    assert_eq!(sheet("@a;"), "@a;");
}

#[test]
fn unknown_atrule_nested_in_rule_interleaves() {
    assert_eq!(sheet("a { @b c; }"), "a {\n  @b c;\n}");
}

#[test]
fn unknown_atrule_block_body_is_grouped() {
    assert_eq!(sheet("@a { b { color: red } }"), "@a {\n  b {\n    color: red;\n  }\n}");
}

#[test]
fn unknown_atrule_value_evaluates_only_interpolation() {
    let src = "$x: Z; @a foo-#{$x} bar;";
    assert_eq!(sheet(src), "@a foo-Z bar;");
}

