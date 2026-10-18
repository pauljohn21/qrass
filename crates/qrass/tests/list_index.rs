//! `list.nth` / `list.set-nth` index-validation contract (dart's `$n:`-prefixed
//! messages), read off the `core_functions/list/{nth,set_nth}.hrx` goldens. Both
//! share `builtins/list.rs::list_index`. Separate file to keep builtins suites
//! within the 500-line budget.

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
fn nth_index_validation() {
    // A valid index (and a negative one) still resolves.
    assert_eq!(value("list.nth(c d, 1)"), "c");
    assert_eq!(value("list.nth(c d, -1)"), "d");
    let msg = eval_err(".a { p: list.nth(c d, e); }");
    assert!(msg.contains("$n: e is not a number."), "{msg}");
    let msg = eval_err(".a { p: list.nth(c d, 0); }");
    assert!(msg.contains("$n: List index may not be 0."), "{msg}");
    let msg = eval_err(".a { p: list.nth(c d, 3); }");
    assert!(
        msg.contains("$n: Invalid index 3 for a list with 2 elements."),
        "{msg}"
    );
    let msg = eval_err(".a { p: list.nth(c d, -3); }");
    assert!(
        msg.contains("$n: Invalid index -3 for a list with 2 elements."),
        "{msg}"
    );
}

#[test]
fn set_nth_shares_the_index_contract() {
    assert_eq!(value("list.set-nth(c d, 1, e)"), "e d");
    let msg = eval_err(".a { p: list.set-nth(c d, 3, e); }");
    assert!(
        msg.contains("$n: Invalid index 3 for a list with 2 elements."),
        "{msg}"
    );
    let msg = eval_err(".a { p: list.set-nth(c d, 0, e); }");
    assert!(msg.contains("$n: List index may not be 0."), "{msg}");
    let msg = eval_err(".a { p: list.set-nth(c d, e, f); }");
    assert!(msg.contains("$n: e is not a number."), "{msg}");
}
