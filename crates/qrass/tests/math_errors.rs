//! `math.clamp($min, $number, $max)` argument-validation contract, read off the
//! sass-spec `core_functions/math/clamp.hrx` goldens. Kept in its own file so the
//! builtins suite stays under the 500-line budget while the math error surface
//! grows (round 40 added `hypot`, round 43 `clamp`).
//!
//! dart's `clamp` validates types first ($min, $number, $max in order), then the
//! unit compatibilities of $number and $max against $min (the base), with a
//! "(one has units and the other doesn't)" clause when exactly one side is
//! unitless. Arity errors already come from the 3/Some(3) registration.

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

fn eval_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.into(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

fn value(expr: &str) -> String {
    let out = css(&format!(".a {{ p: {expr}; }}"));
    let rest = out
        .split_once("p: ")
        .unwrap_or_else(|| panic!("no declaration emitted for {expr:?}: {out}"))
        .1;
    rest.split_once(";\n")
        .map(|x| x.0)
        .unwrap_or(rest)
        .trim()
        .to_string()
}

#[test]
fn clamp_success_still_holds() {
    assert_eq!(value("math.clamp(0, 2, 1)"), "1");
    assert_eq!(value("math.clamp(0, 1, 2)"), "1");
    // Cross-unit angular arguments reduce to $min's unit.
    assert_eq!(value("math.clamp(180deg, 0.5turn, 360deg)"), "180deg");
}

#[test]
fn clamp_type_errors_name_the_argument() {
    let msg = eval_err(".a { p: math.clamp(\"0\", 1, 2); }");
    assert!(msg.contains("$min: \"0\" is not a number."), "{msg}");
    let msg = eval_err(".a { p: math.clamp(1, \"0\", 2); }");
    assert!(msg.contains("$number: \"0\" is not a number."), "{msg}");
    let msg = eval_err(".a { p: math.clamp(1, 2, \"0\"); }");
    assert!(msg.contains("$max: \"0\" is not a number."), "{msg}");
}

#[test]
fn clamp_unit_incompatible_against_min() {
    // $number is checked against $min before $max.
    let msg = eval_err(".a { p: math.clamp(1deg, 1px, 1turn); }");
    assert!(
        msg.contains("$number: 1px and $min: 1deg have incompatible units."),
        "{msg}"
    );
    // A compatible $number lets the $max clash surface instead.
    let msg = eval_err(".a { p: math.clamp(1deg, 1turn, 1px); }");
    assert!(
        msg.contains("$max: 1px and $min: 1deg have incompatible units."),
        "{msg}"
    );
}

#[test]
fn clamp_unitless_mix_adds_the_clause() {
    let msg = eval_err(".a { p: math.clamp(0, 1px, 2px); }");
    assert!(
        msg.contains("$number: 1px and $min: 0 have incompatible units (one has units and the other doesn't)."),
        "{msg}"
    );
    // $number unitless vs $min with units.
    let msg = eval_err(".a { p: math.clamp(0px, 1, 2); }");
    assert!(
        msg.contains("$number: 1 and $min: 0px have incompatible units (one has units and the other doesn't)."),
        "{msg}"
    );
    // A compatible $number pushes the unitless clash to $max.
    let msg = eval_err(".a { p: math.clamp(0px, 1px, 2); }");
    assert!(
        msg.contains("$max: 2 and $min: 0px have incompatible units (one has units and the other doesn't)."),
        "{msg}"
    );
}

#[test]
fn clamp_arity_from_registration() {
    let msg = eval_err(".a { p: math.clamp(); }");
    assert!(msg.contains("Missing argument $min."), "{msg}");
    let msg = eval_err(".a { p: math.clamp(0); }");
    assert!(msg.contains("Missing argument $number."), "{msg}");
    let msg = eval_err(".a { p: math.clamp(0, 0); }");
    assert!(msg.contains("Missing argument $max."), "{msg}");
    let msg = eval_err(".a { p: math.clamp(0, 0, 0, 0); }");
    assert!(
        msg.contains("Only 3 arguments allowed, but 4 were passed."),
        "{msg}"
    );
}

// ─── math.atan2($y, $x): compatible-unit ratio + $y-referenced errors ──────────

#[test]
fn atan2_accepts_compatible_units() {
    // The operands need not be unitless — they are reduced to $y's unit first.
    assert_eq!(value("math.atan2(1cm, -10mm)"), "135deg");
    assert_eq!(value("math.atan2($y: 1, $x: -1)"), "135deg");
}

#[test]
fn atan2_errors() {
    let msg = eval_err(".a { p: math.atan2(\"0\", 0); }");
    assert!(msg.contains("$y: \"0\" is not a number."), "{msg}");
    let msg = eval_err(".a { p: math.atan2(0, \"0\"); }");
    assert!(msg.contains("$x: \"0\" is not a number."), "{msg}");
    // $x is reported against the $y reference.
    let msg = eval_err(".a { p: math.atan2(1px, 1deg); }");
    assert!(
        msg.contains("$x: 1deg and $y: 1px have incompatible units."),
        "{msg}"
    );
    let msg = eval_err(".a { p: math.atan2(1, 1px); }");
    assert!(
        msg.contains("$x: 1px and $y: 1 have incompatible units (one has units and the other doesn't)."),
        "{msg}"
    );
    // Arity (from the 2/Some(2) registration).
    let msg = eval_err(".a { p: math.atan2(); }");
    assert!(msg.contains("Missing argument $y."), "{msg}");
    let msg = eval_err(".a { p: math.atan2(0); }");
    assert!(msg.contains("Missing argument $x."), "{msg}");
    let msg = eval_err(".a { p: math.atan2(0, 0, 0); }");
    assert!(
        msg.contains("Only 2 arguments allowed, but 3 were passed."),
        "{msg}"
    );
}

// ─── math.min/max($numbers…): coerced comparison + bare-value errors ───────────

#[test]
fn min_max_compare_in_coerced_units() {
    // Same magnitude, different length units — the winner is decided after
    // reducing to a common unit, not on the raw magnitude.
    assert_eq!(value("math.min(1px, 1in, 1cm)"), "1px");
    assert_eq!(value("math.max(1px, 1in, 1cm)"), "1in");
    assert_eq!(value("math.max(6px, 2px, 10px)"), "10px");
    // Exactly one side unitless: the unit-bearing value is the larger.
    assert_eq!(value("math.min(2px, 1)"), "1");
    assert_eq!(value("math.max(2px, 1)"), "2px");
}

#[test]
fn min_max_errors() {
    let msg = eval_err(".a { p: math.min(c); }");
    assert!(msg.contains("c is not a number."), "{msg}");
    let msg = eval_err(".a { p: math.max(1, 2, c); }");
    assert!(msg.contains("c is not a number."), "{msg}");
    // Zero arguments self-report (arity min lowered to 0).
    let msg = eval_err(".a { p: math.min(); }");
    assert!(msg.contains("At least one argument must be passed."), "{msg}");
    // Two incompatible dimensioned operands error with the bare operand form.
    let msg = eval_err(".a { p: math.max(1px, 2s); }");
    assert!(
        msg.contains("1px and 2s have incompatible units."),
        "{msg}"
    );
}
