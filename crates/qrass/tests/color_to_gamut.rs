//! `color.to-gamut` contract tests (round 82, `core_functions/color/
//! to_gamut/*`). Rules read off the golden tables: `clip` clamps the color
//! into `$space`'s range and re-emits in the source space; in-gamut colors
//! pass through unchanged for every method; and the error family follows the
//! `to_gamut/error.hrx` spellings (unquoted-string requirements, unknown
//! space/method, the mandatory `$method` paragraph, arity).

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn value(expr: &str) -> String {
    let src = format!("@use \"sass:color\"; .a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src, program)
        .expect("eval ok")
        .join("\n");
    out.split_once("p: ")
        .map(|x| x.1.split_once(";\n").map(|y| y.0).unwrap_or(x.1))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn err(expr: &str) -> String {
    let src = format!("@use \"sass:color\"; .a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src, program)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn clip_family() {
    assert_eq!(
        value("color.to-gamut(color(srgb 1.5 0.5 0.8), $method: clip)"),
        "color(srgb 1 0.5 0.8)"
    );
    // cross-space clip re-emits the source space (`to_gamut/space` golden)
    assert_eq!(
        value("color.to-gamut(oklch(60% 0.15 240deg), $space: rgb, $method: clip)"),
        "oklch(60.4068474766% 0.1443906186 242.3895389183deg)"
    );
    // in-gamut passes through for every method
    assert_eq!(
        value("color.to-gamut(color(srgb 0.2 0.5 0.8), $method: local-minde)"),
        "color(srgb 0.2 0.5 0.8)"
    );
}

#[test]
fn error_spellings() {
    assert!(err("color.to-gamut(pink, $space: red, $method: clip)")
        .contains("$space: red is not a string."));
    assert!(err("color.to-gamut(pink, $space: c, $method: clip)")
        .contains("Unknown color space \"c\"."));
    assert!(
        err("color.to-gamut(pink, $space: \"rgb\", $method: clip)")
            .contains("$space: Expected \"rgb\" to be an unquoted string.")
    );
    assert!(err("color.to-gamut(pink, $method: 1)")
        .contains("$method: 1 is not a string."));
    assert!(err("color.to-gamut(pink, $method: c)")
        .contains("Unknown gamut map method \"c\"."));
    assert!(
        err("color.to-gamut(pink, $method: \"clip\")")
            .contains("$method: Expected \"clip\" to be an unquoted string.")
    );
    assert!(err("color.to-gamut(pink)").contains(
        "$method: color.to-gamut() requires a $method argument for forwards-compatibility"
    ));
    assert!(err("color.to-gamut()").contains("Missing argument $color."));
    assert!(err("color.to-gamut(red, rgb, clip, c)")
        .contains("Only 3 arguments allowed, but 4 were passed."));
}
