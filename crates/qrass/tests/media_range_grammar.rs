//! Media-range grammar contract (round 90, `css/media/range/error`
//! `invalid_binary_operator`): a parenthesised media feature may carry at most
//! two top-level comparators; a third is garbage inside the paren group for
//! dart's grammar and reports `expected ")".` — while the legal two-comparator
//! range form stays untouched.

use qrass::parser::parse;
use qrass::lexer::lex;

fn parse_result(src: &str) -> Result<(), String> {
    let tokens = lex(0, "t.scss".into(), src.to_string()).map_err(|e| e.message)?;
    parse(0, "t.scss".into(), src.to_string(), tokens)
        .map(|_| ())
        .map_err(|e| e.message)
}

#[test]
fn chained_comparators_are_rejected() {
    for src in [
        "@media (1 < width < 2 < 3) {a {b: c}}",
        "@media (1 <= width <= 2 <= 3) {a {b: c}}",
        "@media screen and (1 < a < 2 < 3) {a {b: c}}",
    ] {
        let err = parse_result(src).expect_err("chained range must fail");
        assert!(err.contains("expected \")\"."), "got: {err}");
    }
}

#[test]
fn legal_ranges_pass() {
    for src in [
        "@media (1 <= width <= 2) {a {b: c}}",
        "@media (width < 100px) {a {b: c}}",
        "@media (400px <= width <= 700px) and (height > 10px) {a {b: c}}",
    ] {
        parse_result(src).expect("legal media range");
    }
}
