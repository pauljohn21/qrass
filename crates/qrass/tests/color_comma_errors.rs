//! Golden-pinned contract for the legacy comma color constructors' error
//! checks (`core_functions/color/{hsl,rgb,hwb}/error/four_args|five_args`) —
//! the pre-passthrough interceptor split out of `eval_call` in round 88.
//! Tests the AST-level shape rules: arity cap, quoted-string channel blame,
//! alpha unit demand, and the diverging `color.hwb` spellings.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

/// Compile `expr` through the full front end; return the CSS on success or
/// the error message on failure.
fn outcome(expr: &str) -> String {
    let src = format!("@use \"sass:color\"; .a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src, program) {
        Ok(out) => out.join("\n"),
        Err(e) => e.message,
    }
}

#[test]
fn arity_and_type_errors() {
    // `hsl/error/five_args`
    assert!(
        outcome("hsl(0, 100%, 50%, 0.5, 0)").contains("Only 4 arguments allowed, but 5 were passed."),
        "arity"
    );
    // `hsl/error/four_args` type family
    assert!(
        outcome("hsl(0, \"foo\", 50%, 0.5)").contains("$saturation: \"foo\" is not a number."),
        "saturation type"
    );
    assert!(
        outcome("hsl(0, 100%, \"foo\", 0.5)").contains("$lightness: \"foo\" is not a number."),
        "lightness type"
    );
    // `rgb/error/four_args`
    assert!(
        outcome("rgb(0, \"foo\", 0, 0.5)").contains("$green: \"foo\" is not a number."),
        "green type"
    );
    // alpha unit demand
    assert!(
        outcome("hsl(0, 0%, 0%, 0.5px)")
            .contains("$alpha: Expected 0.5px to have unit \"%\" or no units."),
        "alpha unit"
    );
}

#[test]
fn hwb_diverges_in_spelling() {
    // `hwb/error/four_args` — channel-name wording, percent-forced w/b
    assert!(
        outcome("color.hwb(0, \"foo\", 40%, 0.5)")
            .contains("Expected whiteness channel to be a number, was \"foo\"."),
        "hwb type"
    );
    assert!(
        outcome("color.hwb(0, 30, 40%, 0.5)").contains("$whiteness: Expected 30 to have unit \"%\"."),
        "hwb unitless w"
    );
    assert!(
        outcome("color.hwb(0, 30px, 40%, 0.5)")
            .contains("$whiteness: Expected 30px to have unit \"%\"."),
        "hwb wrong unit"
    );
}

#[test]
fn clean_and_symbolic_calls_stay_verbatim() {
    // The numeric comma forms keep the historical verbatim serialization.
    let out = outcome("hsl(120, 50%, 50%)");
    assert!(out.contains("hsl(120, 50%, 50%)"), "numeric hsl: {out}");
    // Symbolic slots (calc/var) are never judged by the interceptor.
    let out = outcome("hsl(0, calc(2%), 50%, 0.4)");
    assert!(!out.contains("not a number"), "calc slot: {out}");
    let out = outcome("rgb(0, var(--x), 0, 0.5)");
    assert!(!out.contains("not a number"), "var slot: {out}");
}
