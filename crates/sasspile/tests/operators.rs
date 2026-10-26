//! Identifier concatenation (tasks 1.3b): what `+` and `-` do when both operands are unquoted
//! identifiers. Every expectation below is a section of `sass-spec/operators/{plus,minus}.hrx`, and the
//! four whitespace shapes plus the loud-comment shape are enumerated because the fixtures state each
//! one separately — Sass collapses them all to a single result.

use sasspile::compile;

async fn value(input: &str) -> String {
    let css = compile(input).await.unwrap_or_else(|e| {
        panic!("concatenation compiles: {input} failed with {}", e.message);
    });
    // `a {b: <value>;}` is the shape every fixture uses; return just the value.
    css.lines()
        .nth(1)
        .unwrap_or_default()
        .trim()
        .trim_end_matches(';')
        .strip_prefix("b:")
        .unwrap_or_else(|| panic!("unexpected output {css:?}"))
        .trim()
        .to_string()
}

/// `operators/plus.hrx::syntax/*` — the `+` disappears entirely, whatever surrounds it.
#[tokio::test]
async fn plus_between_identifiers_concatenates_and_vanishes() {
    assert_eq!(value("a {b: c+d}").await, "cd");
    assert_eq!(value("a {b: c + d}").await, "cd");
    assert_eq!(value("a {b: c +d}").await, "cd");
    assert_eq!(value("a {b: c+ d}").await, "cd");
    assert_eq!(value("a {b: c\n+d}").await, "cd");
    assert_eq!(value("a {b: c/**/+/**/d}").await, "cd");
    assert_eq!(value("a {b: c/**/+d}").await, "cd");
    assert_eq!(value("a {b: c+/**/d}").await, "cd");
}

/// `operators/minus.hrx::syntax/*` — `-` **stays** in the text and no space survives, and a
/// whole-parenthesised operand is unwrapped (`(c)-(d)` → `c-d`).
#[tokio::test]
async fn minus_between_identifiers_keeps_the_sign_and_drops_the_spaces() {
    assert_eq!(value("a {b: (c)-(d)}").await, "c-d");
    assert_eq!(value("a {b: c - d}").await, "c-d");
    assert_eq!(value("a {b: c -(d)}").await, "c-d");
    assert_eq!(value("a {b: (c)- d}").await, "c-d");
    assert_eq!(value("a {b: c/**/-/**/d}").await, "c-d");
}

/// The boundary that keeps this from swallowing shapes it must not read. Each case below still renders
/// exactly as the source said, because the concatenation fold refuses to fire:
/// * a **number** operand sends the run back to the arithmetic path, where the spacing law holds, so
///   `10px -5px` stays the two-item list it is today (the `-` is glued into the following lexeme, so no
///   operator token ever appears);
/// * `1px+2px` is not identifier concatenation either, and remains unevaluated until P1's unit work;
/// * a lone identifier has no operator at all;
/// * `c-d` is one lexeme (the lexer glues an identifier hyphen), never an operation;
/// * `(c)(d)` is two groups, not one balanced wrapper, so nothing is peeled.
#[tokio::test]
async fn numbers_and_lone_lexemes_keep_their_old_reading() {
    assert_eq!(value("a {b: 10px -5px}").await, "10px -5px");
    assert_eq!(value("a {b: 1px+2px}").await, "1px+2px");
    assert_eq!(value("a {b: c}").await, "c");
    assert_eq!(value("a {b: c-d}").await, "c-d");
    assert_eq!(value("a {b: (c)(d)}").await, "(c)(d)");
}
