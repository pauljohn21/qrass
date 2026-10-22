//! OpenSpec converge-sass-spec-to-100 M5: dart's bogus-combinators golden
//! matrix (`css/selector/combinator/*`) — which invalid combinator forms drop
//! their rule and which pass through verbatim (with only a warning).

use qrass::{Options, OutputStyle, compile_string};

fn out(scss: &str) -> String {
    compile_string(
        "/bc/in.scss".to_string(),
        scss.to_string(),
        &Options {
            style: OutputStyle::Expanded,
            load_paths: Vec::new(),
        },
    )
    .expect("compiles")
}

#[test]
fn top_level_leading_combinator_is_kept() {
    // `leading.hrx`: `+ a {b: c}` prints verbatim + warns; it is NOT dropped.
    let css = out("+ a {b: c}\n");
    assert!(css.contains("+ a {"), "{css}");
    let css = out("> a {b: c}\n");
    assert!(css.contains("> a {"), "{css}");
}

#[test]
fn adjacent_combinators_drop_the_rule() {
    // `middle` `multiple/*`: `a > + b` is omitted from the output entirely.
    let css = out("a > + b {c: d}\n");
    assert!(!css.contains("c: d"), "{css}");
    assert!(!css.contains("a > + b"), "{css}");
}

#[test]
fn trailing_combinator_drop() {
    // `trailing.hrx`: `a + {b: c}` is omitted.
    let css = out("a + {b: c}\na ~ {b: c}\n");
    assert!(!css.contains("b: c"), "{css}");
}

#[test]
fn combinator_at_paren_head_drops_except_has() {
    // `is.hrx` drops `:is(+ a)`/`:is(> a)`; `has.hrx` KEEPS `:has(+ a)` and
    // even `:has( > a)` (relative selectors are the spec exemption).
    let css = out(":is(+ a) {b: c}\n");
    assert!(!css.contains("b: c"), "{css}");
    let css = out(":has(+ a) {b: c}\n");
    assert!(css.contains("b: c"), "{css}");
    assert!(css.contains(":has(+ a)"), "{css}");
    let css = out("x:has( > a) {p: q}\n");
    assert!(css.contains("p: q"), "{css}");
}

#[test]
fn valid_combinator_selectors_are_untouched() {
    let css = out("a + b {c: d}\nx > y {c: d}\ne ~ f {c: d}\ng:is(h, i) {c: d}\n");
    assert_eq!(css.matches("c: d").count(), 4, "{css}");
    assert!(css.contains("g:is(h, i)"), "{css}");
}
