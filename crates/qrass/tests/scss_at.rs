//! At-rule container tests (task 7.4): `@media`/`@supports` wrap their body into
//! a single merged block (sibling rules grouped, nested rules keep their selector
//! context), `@keyframes` wraps its stops, and `@at-root` re-parents wrapped rules
//! to the top level (dropping the surrounding selector nesting).

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

#[test]
fn media_groups_sibling_rules() {
    let out = css("@media screen { .a { color: red; } .b { color: blue; } }");
    // Exactly one `@media screen {` wrapper containing both rules.
    assert_eq!(out.matches("@media screen {").count(), 1, "{out}");
    assert!(out.contains(".a {") && out.contains(".b {"), "{out}");
}

#[test]
fn media_nested_in_rule_bubbles_selector() {
    let out = css(".c { @media print { .d { color: red; } } }");
    assert!(out.contains("@media print {"), "{out}");
    assert!(
        out.contains(".c .d {"),
        "selector context preserved under media\n---\n{out}"
    );
}

#[test]
fn supports_wraps_body() {
    let out = css("@supports (display: grid) { .g { display: grid; } }");
    assert!(
        out.starts_with("@supports (display:grid) {")
            || out.starts_with("@supports (display: grid) {"),
        "{out}"
    );
    assert!(out.contains("display: grid;"), "{out}");
}

#[test]
fn at_root_drops_nesting() {
    let out = css(".parent { color: black; @at-root { .child { color: red; } } }");
    assert!(out.contains(".parent {\n  color: black;\n}"), "{out}");
    assert!(
        out.contains(".child {\n  color: red;\n}"),
        "at-root rule is un-nested\n---\n{out}"
    );
    assert!(!out.contains(".parent .child"), "{out}");
}

#[test]
fn keyframes_wraps_stops() {
    let out = css("@keyframes spin { from { opacity: 0; } to { opacity: 1; } }");
    assert!(out.starts_with("@keyframes spin {"), "{out}");
    assert!(out.contains("from {") && out.contains("to {"), "{out}");
}

#[test]
fn media_with_interpolated_query() {
    let out = css("$bp: 600px; @media (max-width: #{$bp}) { .r { color: red; } }");
    assert!(
        out.contains("600px") && out.contains("@media") && out.contains(".r {"),
        "{out}"
    );
}
