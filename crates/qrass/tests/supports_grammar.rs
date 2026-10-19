//! `@supports` condition grammar (round 71, `css/supports/error.hrx` +
//! `css/supports/syntax/*` goldens): the conservative validator claims only
//! the pinned invalid spellings; every legal shape — groups, `not (…)`
//! heads, functional conditions (`selector(…)`, interpolated names), lone
//! interpolations and operator chains — keeps today's passthrough.

use qrass::lexer::lex;
use qrass::parser::parse;

fn parse_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    match parse(0, "t.scss".into(), src.to_string(), tokens) {
        Ok(_) => panic!("expected parse error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

fn parses(src: &str) {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    parse(0, "t.scss".into(), src.to_string(), tokens)
        .unwrap_or_else(|e| panic!("{src:?} must parse: {e}"));
}

#[test]
fn invalid_supports_heads() {
    let want = "Expected @supports condition.";
    for src in [
        "@supports a {@b}",
        "@supports a: b {@c}",
        "@supports a#{b} {@c}",
        // NOTE: `#{a}b` (interpolation-then-letter) still passes through; its
        // lexer gluing keeps it outside this validator's reach (known FAIL).
        "@supports not a {@b}",
        "@supports (not a: b) {@c}",
        "@supports (a: b) and (not c: d) {@e}",
    ] {
        assert!(parse_err(src).contains(want), "{src}");
    }
}

#[test]
fn invalid_supports_operator_shapes() {
    // two groups without an operator, and a chain on the leading `not (…)`
    assert!(parse_err("@supports (a: b) (c: d) {@e}").contains("expected \"{\"."));
    assert!(
        parse_err("@supports not (a: b) and (c: d) {@e}").contains("expected \"{\"."),
        "and after leading not"
    );
}

#[test]
fn legal_supports_conditions_keep_passing_through() {
    for src in [
        "@supports (display: grid) {a {b: c}}",
        "@supports not (a: b) {c {d: e}}",
        "@supports (a: b) and (c: d) {e {f: g}}",
        "@supports (a) or not (b) {c {d: e}}",
        "@supports selector(:has(b)) {c {d: e}}",
        "@supports not a() {@b}",
        "@supports #{\"x\"}(b) {q: r}",
        "@supports #{\"(a: b)\"} {@c}",
        "@supports #{\"(a: b)\"} and (c: 1) {@d}",
        "@supports (c: 1) and #{\"(a: b)\"} {@d}",
    ] {
        parses(src);
    }
}
