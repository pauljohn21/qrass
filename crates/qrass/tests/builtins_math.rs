//! `math.*` builtin tests (split out of `builtins.rs` to keep each file within the
//! 500-line budget). Namespaced calls dispatch directly (no `@use`).

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

/// Evaluate `expr` as the value of declaration `p` and return its serialized CSS.
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
fn math_div_serialization_and_units() {
    assert_eq!(value("math.div(1, 3)"), "0.3333333333");
    assert_eq!(value("math.div(10px, 2)"), "5px");
}

#[test]
fn math_rounding() {
    assert_eq!(value("math.round(2.6)"), "3");
    assert_eq!(value("math.ceil(2.1)"), "3");
    assert_eq!(value("math.floor(2.9)"), "2");
    assert_eq!(value("math.abs(-4)"), "4");
}

#[test]
fn math_min_max_variadic() {
    assert_eq!(value("math.min(3, 1, 2)"), "1");
    assert_eq!(value("math.max(3, 1, 2)"), "3");
}

#[test]
fn math_percentage_and_power() {
    assert_eq!(value("math.percentage(0.25)"), "25%");
    assert_eq!(value("math.pow(2, 10)"), "1024");
    assert_eq!(value("math.sqrt(16)"), "4");
    assert_eq!(value("math.log(math.pow(2, 3), 2)"), "3");
}

#[test]
fn math_constants_resolve_without_use() {
    // `math.$pi` / `math.$e` come from the built-in module constant table.
    assert!(
        value("math.$pi").starts_with("3.141592653"),
        "{}",
        value("math.$pi")
    );
    assert!(
        value("math.$e").starts_with("2.71828"),
        "{}",
        value("math.$e")
    );
}

#[test]
fn math_trig_direct_returns_unitless_ratio() {
    // Angles accept a unit (deg/grad/rad/turn) or a unitless radian measure.
    assert_eq!(value("math.sin(1deg)"), "0.0174524064");
    assert_eq!(value("math.sin(1rad)"), "0.8414709848");
    assert_eq!(value("math.sin(1)"), "0.8414709848");
    assert_eq!(value("math.sin(1turn)"), "0");
    assert_eq!(value("math.cos(1rad)"), "0.5403023059");
    assert_eq!(value("math.tan(1rad)"), "1.5574077247");
}

#[test]
fn math_inverse_trig_returns_degrees() {
    assert_eq!(value("math.asin(0.5)"), "30deg");
    assert_eq!(value("math.asin(1)"), "90deg");
    assert_eq!(value("math.atan(1)"), "45deg");
    assert_eq!(value("math.atan2(1, 1)"), "45deg");
}

#[test]
fn math_non_finite_wraps_in_calc() {
    // A non-finite number is not a legal bare CSS number: the goldens emit it in
    // `calc()`. `math.div(1, 0)` is infinity, not an error.
    assert_eq!(value("math.div(1, 0)"), "calc(infinity)");
    assert_eq!(value("math.sin(math.div(1, 0))"), "calc(NaN)");
    assert_eq!(value("math.asin(2)"), "calc(NaN * 1deg)");
}

#[test]
fn math_hypot_unit_clamp_and_predicates() {
    assert_eq!(value("math.hypot(3, 4)"), "5");
    assert_eq!(value("math.hypot(3cm, 4mm * 10)"), "5cm");
    assert_eq!(value("math.clamp(0, 2, 1)"), "1");
    assert_eq!(value("math.is-unitless(1px)"), "false");
    assert_eq!(value("math.unit(1px)"), "\"px\"");
    assert_eq!(value("math.unit(1)"), "\"\"");
    assert_eq!(value("math.compatible(1px, 1in)"), "true");
    assert_eq!(value("math.compatible(1px, 1s)"), "false");
}

#[test]
fn math_hypot_errors() {
    // A non-number reads the value verbatim (a quoted string keeps its quotes).
    let msg = eval_err(".a { p: math.hypot(\"0\", 1px); }");
    assert!(msg.contains("\"0\" is not a number."), "{msg}");
    // Incompatible units name each argument against $numbers[1] (1-based).
    let msg = eval_err(".a { p: math.hypot(1deg, 1px, 1turn); }");
    assert!(
        msg.contains("$numbers[2]: 1px and $numbers[1]: 1deg have incompatible units."),
        "{msg}"
    );
    let msg = eval_err(".a { p: math.hypot(1deg, 1turn, 1px); }");
    assert!(
        msg.contains("$numbers[3]: 1px and $numbers[1]: 1deg have incompatible units."),
        "{msg}"
    );
    // One-unitless / one-not adds the parenthetical.
    let msg = eval_err(".a { p: math.hypot(0px, 1, 2); }");
    assert!(
        msg.contains(
            "$numbers[2]: 1 and $numbers[1]: 0px have incompatible units (one has units and the other doesn't)."
        ),
        "{msg}"
    );
    // Zero arguments raise dart's own wording (min lowered to 0 so the fn guards).
    let msg = eval_err(".a { p: math.hypot(); }");
    assert!(msg.contains("At least one argument must be passed."), "{msg}");
}
