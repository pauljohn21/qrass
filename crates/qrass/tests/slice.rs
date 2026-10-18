//! Round 30 — `string.slice` / legacy `str-slice`.
//!
//! These lock the convergence of the 41 sass-spec `core_functions/string/slice`
//! cases: Sass indexes by Unicode *codepoint*, bounds are 1-based and inclusive
//! at both ends, out-of-range indices clamp, an `end` landing before the start
//! (including `end-at: 0`) yields the empty string, and the result inherits the
//! source string's quote style. Arguments are strictly typed to the goldens'
//! dart-shaped errors.

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

fn eval_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.into(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn slice_inherits_the_source_quote_style() {
    // A quoted source slices to a quoted result; an unquoted one stays bare.
    assert_eq!(value(r#"string.slice("cde", 2)"#), "\"de\"");
    assert_eq!(value("string.slice(cdefgh, 3, 5)"), "efg");
}

#[test]
fn slice_bounds_are_one_based_inclusive_and_clamp() {
    assert_eq!(value(r#"string.slice("cde", 1, 1)"#), "\"c\"");
    assert_eq!(value(r#"string.slice("cde", 1, 2)"#), "\"cd\"");
    assert_eq!(value(r#"string.slice("cde", 1, 100)"#), "\"cde\"");
    // A negative index counts from the end; an out-of-range one clamps.
    assert_eq!(value(r#"string.slice("cde", -2)"#), "\"de\"");
    assert_eq!(value(r#"string.slice("cde", 1, -2)"#), "\"cd\"");
    // An end that lands before the start yields the empty string.
    assert_eq!(value(r#"string.slice("cde", 1, 0)"#), "\"\"");
    assert_eq!(value(r#"string.slice("cde", 1, -100)"#), "\"\"");
    assert_eq!(value(r#"string.slice("cde", 4)"#), "\"\"");
    // The empty string slices to the empty string for any index.
    assert_eq!(value(r#"string.slice("", 1, 2)"#), "\"\"");
}

#[test]
fn slice_indexes_by_codepoint_not_grapheme() {
    // An astral emoji is a single codepoint even though UTF-16 needs two units.
    assert_eq!(value(r#"string.slice("c👭d", 2, 2)"#), "\"👭\"");
}

#[test]
fn slice_argument_errors_match_dart_wording() {
    assert!(
        eval_err(r#".a { p: string.slice(1, 2); }"#).contains(r#"$string: 1 is not a string."#)
    );
    assert!(
        eval_err(r#".a { p: string.slice("cde", "f"); }"#)
            .contains(r#"$start-at: "f" is not a number."#)
    );
    assert!(
        eval_err(r#".a { p: string.slice("cde", 1, "f"); }"#)
            .contains(r#"$end-at: "f" is not a number."#)
    );
    // A decimal index errors without the `$param:` prefix, as dart phrases it.
    assert!(eval_err(r#".a { p: string.slice("", 0.5); }"#).contains("0.5 is not an int."));
    assert!(
        eval_err(r#".a { p: string.slice("", 1px); }"#)
            .contains(r#"$start-at: Expected 1px to have no units."#)
    );
}
