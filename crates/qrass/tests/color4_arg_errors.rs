//! Modern Color 4 constructor argument validation (task 11.3, color round W4a):
//! a concrete-but-wrong channel in `lab()/lch()/oklab()/oklch()/hsl()/hwb()`
//! raises dart's `$<channel>:` / `$channels:` messages, while symbolic `var()`
//! channels keep falling through to plain-CSS passthrough.

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

#[test]
fn color4_wrong_unit_channel_errors() {
    let msg = eval_err(".a { p: lab(1px 2 3); }");
    assert!(
        msg.contains("$lightness: Expected 1px to have unit \"%\" or no units."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(1% 2px 3); }");
    assert!(
        msg.contains("$a: Expected 2px to have unit \"%\" or no units."),
        "{msg}"
    );
}

#[test]
fn color4_non_number_channel_errors() {
    let msg = eval_err(".a { p: lab(c 2 3); }");
    assert!(
        msg.contains("$channels: Expected lightness channel to be a number, was c."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(1% 2 c); }");
    assert!(
        msg.contains("$channels: Expected b channel to be a number, was c."),
        "{msg}"
    );
}

#[test]
fn color4_symbolic_channel_still_passes_through() {
    // A var() channel must remain plain-CSS passthrough (no color error).
    let out = css(".a { p: lab(var(--x) 2 3); }");
    assert!(out.contains("lab(var(--x) 2 3)"), "{out}");
}
