//! `list.join` `$separator` validation contract (dart's `$separator:`-prefixed
//! messages), from the `core_functions/list/join/error.hrx` goldens. Separate file
//! to keep builtins suites within the 500-line budget.

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
fn join_separator_must_be_a_valid_string() {
    let msg = eval_err(".a { p: list.join(c, d, $separator: 1); }");
    assert!(msg.contains("$separator: 1 is not a string."), "{msg}");
    // An unquoted bareword is a string, so it fails the keyword set, not the type.
    let msg = eval_err(".a { p: list.join(c, d, $separator: e); }");
    assert!(
        msg.contains("$separator: Must be \"space\", \"comma\", \"slash\", or \"auto\"."),
        "{msg}"
    );
}

#[test]
fn join_separator_gap_padding_from_named_is_not_validated() {
    // Only `$bracketed` given: fold_named pads the separator slot with Null, which
    // must NOT trip the separator validation (dart treats null as "auto"). The call
    // succeeds and joins with list1's separator.
    let out = css(".a { p: list.join(c d, e f, $bracketed: false); }");
    assert!(out.contains("p: c d e f;"), "{out}");
}
