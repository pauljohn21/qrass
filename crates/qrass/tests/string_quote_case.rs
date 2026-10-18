//! `string.to-upper-case` / `to-lower-case` quote-inheritance + ASCII-only case
//! contract, read off the sass-spec `core_functions/string/to_*.hrx` goldens.
//! Separate file so the already-over-budget `builtins.rs` does not grow.
//!
//! Two golden facts drive it: the result keeps the input's quoting (`"a"` ->
//! `"A"`, bareword `a` -> `A`), and only ASCII letters change case (`"äçðøþ"`
//! is returned unchanged). A non-string raises `$string: <v> is not a string.`.

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
fn case_functions_inherit_quoting() {
    // A quoted input stays quoted; a bareword stays unquoted.
    assert_eq!(value("string.to-upper-case(\"abc\")"), "\"ABC\"");
    assert_eq!(value("string.to-upper-case(aBc)"), "ABC");
    assert_eq!(value("string.to-lower-case(\"ABC\")"), "\"abc\"");
    assert_eq!(value("string.to-lower-case(aBc)"), "abc");
    // Digits / empty pass through, keeping the quote style.
    assert_eq!(value("string.to-upper-case(\"\")"), "\"\"");
    assert_eq!(value("string.to-upper-case(\"123\")"), "\"123\"");
}

#[test]
fn case_functions_are_ascii_only() {
    // The goldens note "Only ASCII characters have their case changed".
    assert_eq!(value("string.to-upper-case(\"äçðøþ\")"), "\"äçðøþ\"");
    assert_eq!(value("string.to-lower-case(\"ÄÇÐØÞ\")"), "\"ÄÇÐØÞ\"");
}

#[test]
fn case_functions_reject_non_strings() {
    let msg = eval_err(".a { p: string.to-upper-case(1); }");
    assert!(msg.contains("$string: 1 is not a string."), "{msg}");
    let msg = eval_err(".a { p: string.to-lower-case(1); }");
    assert!(msg.contains("$string: 1 is not a string."), "{msg}");
}
