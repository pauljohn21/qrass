//! `color.same($color1, $color2)` contract tests (round 76,
//! `core_functions/color/same.hrx`). Rules are read off the golden table:
//! equality happens in XYZ after missing channels normalize to 0 (the order
//! the `false/different_space/both_none` golden documents in its own comment),
//! legacy colors join through their sRGB triple, and the arity/type errors
//! carry the `$color1` / `$color2` parameter names.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn same(args: &str) -> bool {
    let src = format!("@use \"sass:color\"; .a {{ p: color.same({args}); }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src, program)
        .expect("eval ok")
        .join("\n");
    let shown = out
        .split_once("p: ")
        .map(|x| x.1.split_once(";\n").map(|y| y.0).unwrap_or(x.1))
        .unwrap_or_default()
        .trim()
        .to_string();
    match shown.as_str() {
        "true" => true,
        "false" => false,
        other => panic!("expected bool, got {other:?}"),
    }
}

#[test]
fn same_true_family() {
    assert!(same(
        "lab(50.0000000000001 29.9999999999999 -20.0000000000001), \
         lab(50.0000000000001 29.9999999999999 -20.0000000000001)"
    ));
    assert!(same("oklch(50% 30% 120deg / 0.3), oklch(50% 30% 120deg / 0.3)"));
    assert!(same("color(srgb 2.3 -1 42), color(srgb 2.3 -1 42)"));
    // missing channels normalize to 0 before the comparison (`true/same_space`)
    assert!(same("color(display-p3 0.1 0.3 none), color(display-p3 0.1 0.3 0)"));
    // cross-space equality rides the XYZ paths (`true/different_space/one_none`)
    assert!(same(
        "color(rec2020 0.5 none 0.2), \
         oklab(39.853163697274695% 0.20545316630805804 0.044516505430218456)"
    ));
}

#[test]
fn same_false_family() {
    assert!(!same("#abcdef, #fedcba"));
    assert!(!same("color(srgb 0.1 0.2 0.3), color(srgb-linear 0.1 0.2 0.3)"));
    // none→0 happens *before* the conversion, so the XYZ literal with a
    // different first channel stays unequal (`false/different_space/both_none`)
    assert!(!same(
        "color(rec2020 0.5 none 0.2), color(xyz 0.174805932224126 none 0.058901333881161)"
    ));
    assert!(!same("$color1: red, $color2: green"));
}

#[test]
fn same_error_spellings() {
    let err = |expr: &str| {
        qrass::compile_string(
            "t.scss".to_string(),
            format!("@use \"sass:color\"; a {{b: {expr}}}"),
            &qrass::Options::default(),
        )
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
    };
    assert!(err("color.same(plum)").contains("Missing argument $color2."));
    assert!(
        err("color.same(red, green, blue)")
            .contains("Only 2 arguments allowed, but 3 were passed.")
    );
    assert!(err("color.same(1, red)").contains("$color1: 1 is not a color."));
    assert!(err("color.same(red, 1)").contains("$color2: 1 is not a color."));
}
