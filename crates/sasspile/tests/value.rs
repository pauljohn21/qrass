//! Value-floor goldens — pin what P0 guarantees about declaration values: a rendered token
//! run is adopted as an owned `Value` and serialises **verbatim** at the single `to_css`
//! seam (no evaluation, so no CSS meaning may change). Everything is observed through the
//! sole `compile` exit; nothing internal to the crate is leaked `pub` for these tests.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input).await.expect("P0 value floor compiles")
}

#[tokio::test]
async fn raw_value_is_carried_verbatim() {
    assert_eq!(css("a { b: c; }").await, "a {\n  b: c;\n}\n");
    assert_eq!(css("a { color: #ff0000; }").await, "a {\n  color: #ff0000;\n}\n");
}

#[tokio::test]
async fn slash_stays_a_separator_not_division() {
    // P1 owns the value algebra; until then `/` must survive untouched (protects
    // `font: 12px/1.5`, `grid: 1/2`, `w: 1/2`).
    assert_eq!(css("a { font: 12px/1.5; }").await, "a {\n  font: 12px/1.5;\n}\n");
    assert_eq!(css("a { w: 1/2; }").await, "a {\n  w: 1/2;\n}\n");
}

#[tokio::test]
async fn value_whitespace_is_canonicalised() {
    assert_eq!(css("a {  b:   1px    2px  ; }").await, "a {\n  b: 1px 2px;\n}\n");
}

#[tokio::test]
async fn declaration_order_within_a_scope_is_source_order() {
    assert_eq!(css("a { c: 3; b: 2; }").await, "a {\n  c: 3;\n  b: 2;\n}\n");
}

#[tokio::test]
async fn nested_scope_values_stay_with_their_own_scope() {
    assert_eq!(
        css("a { b: 1; & c { d: 2; } }").await,
        "a {\n  b: 1;\n}\na c {\n  d: 2;\n}\n"
    );
}
