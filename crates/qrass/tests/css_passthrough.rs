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
