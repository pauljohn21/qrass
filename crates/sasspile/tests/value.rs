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

#[tokio::test]
async fn spaced_plus_minus_is_arithmetic() {
    assert_eq!(css("a { w: 10px + 5px; }").await, "a {\n  w: 15px;\n}\n");
    assert_eq!(css("a { w: 10px - 5px; }").await, "a {\n  w: 5px;\n}\n");
    assert_eq!(css("a { w: 1 + 2; }").await, "a {\n  w: 3;\n}\n");
}

#[tokio::test]
async fn unitless_operand_adopts_the_other_unit() {
    assert_eq!(css("a { w: 10px + 3; }").await, "a {\n  w: 13px;\n}\n");
    assert_eq!(css("a { w: 3 + 10px; }").await, "a {\n  w: 13px;\n}\n");
}

#[tokio::test]
async fn convertible_units_normalise_into_the_left_operand_unit() {
    assert_eq!(css("a { w: 1px + 1in; }").await, "a {\n  w: 97px;\n}\n");
    assert_eq!(css("a { w: 10s - 500ms; }").await, "a {\n  w: 9.5s;\n}\n");
}

#[tokio::test]
async fn unspaced_sign_is_a_list_item_not_subtraction() {
    // Sass reads `10px -5px` as a two-item space list, so nothing may be evaluated.
    assert_eq!(css("a { w: 10px -5px; }").await, "a {\n  w: 10px -5px;\n}\n");
}

#[tokio::test]
async fn incompatible_units_keep_the_text_instead_of_guessing() {
    // The Sass error travels once the stream carries `Result` (P1 next step); until then the
    // run must survive verbatim rather than invent a number.
    assert_eq!(css("a { w: 5px + 3s; }").await, "a {\n  w: 5px + 3s;\n}\n");
}

#[tokio::test]
async fn a_lone_number_value_is_still_verbatim_text() {
    // A single number is only a number once an expression consumes it; as a declaration
    // value it must survive with its exact source digits (`010` is not Sass's `10`).
    assert_eq!(css("a { w: 10px; }").await, "a {\n  w: 10px;\n}\n");
    assert_eq!(css("a { w: 010; }").await, "a {\n  w: 010;\n}\n");
    assert_eq!(
        css("a { b: rgb(18 52 none); }").await,
        "a {\n  b: rgb(18 52 none);\n}\n"
    );
}

#[tokio::test]
async fn bound_variables_resolve_and_emit_no_css_of_their_own() {
    assert_eq!(css("$w: 5px; a { width: $w; }").await, "a {\n  width: 5px;\n}\n");
    assert_eq!(
        css("$k: 2; .col-#{$k} { w: 1px; }").await,
        ".col-2 {\n  w: 1px;\n}\n"
    );
    assert_eq!(css("a { --d: 4px; width: var(--d); }").await, "a {\n  --d: 4px;\n  width: var(--d);\n}\n");
}
