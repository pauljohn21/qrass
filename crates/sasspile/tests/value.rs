//! 值地板金样 —— 钉住 P0 对声明值的保证：一条渲染出的 token 串被收为 owned `Value`，
//! 在唯一的 `to_css` 接缝处**逐字**序列化（不求值，所以任何 CSS 含义都不许改变）。
//! 一切唯经唯一的 `compile` 出口观察；没有任何 crate 内部件为这些测试泄为 `pub`。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input).await.expect("P0 value floor compiles")
}

#[tokio::test]
async fn raw_value_is_carried_verbatim() {
    assert_eq!(css("a { b: c; }").await, "a {\n  b: c;\n}\n");
    assert_eq!(
        css("a { color: #ff0000; }").await,
        "a {\n  color: #ff0000;\n}\n"
    );
}

#[tokio::test]
async fn slash_stays_a_separator_not_division() {
    // 值代数归 P1；在此之前 `/` 必须原样存活（保卫
    // `font: 12px/1.5`、`grid: 1/2`、`w: 1/2`）。
    assert_eq!(
        css("a { font: 12px/1.5; }").await,
        "a {\n  font: 12px/1.5;\n}\n"
    );
    assert_eq!(css("a { w: 1/2; }").await, "a {\n  w: 1/2;\n}\n");
}

#[tokio::test]
async fn value_whitespace_is_canonicalised() {
    assert_eq!(
        css("a {  b:   1px    2px  ; }").await,
        "a {\n  b: 1px 2px;\n}\n"
    );
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
    // Sass 把 `10px -5px` 读成两项空格列表，所以什么都不能求值。
    assert_eq!(
        css("a { w: 10px -5px; }").await,
        "a {\n  w: 10px -5px;\n}\n"
    );
}

#[tokio::test]
async fn incompatible_units_keep_the_text_instead_of_guessing() {
    // 等流携带 `Result` 后 Sass 的错误才会传递（P1 下一步）；在那之前该运行必须逐字存活，
    // 而不是编造一个数字。
    assert_eq!(css("a { w: 5px + 3s; }").await, "a {\n  w: 5px + 3s;\n}\n");
}

#[tokio::test]
async fn a_lone_number_value_is_still_verbatim_text() {
    // 单个数字只有在表达式消费它时才是数字；作为声明值它必须带着确切的源码数字存活
    // （`010` 不是 Sass 的 `10`）。
    assert_eq!(css("a { w: 10px; }").await, "a {\n  w: 10px;\n}\n");
    assert_eq!(css("a { w: 010; }").await, "a {\n  w: 010;\n}\n");
    assert_eq!(
        css("a { b: rgb(18 52 none); }").await,
        "a {\n  b: rgb(18 52 none);\n}\n"
    );
}

#[tokio::test]
async fn bound_variables_resolve_and_emit_no_css_of_their_own() {
    assert_eq!(
        css("$w: 5px; a { width: $w; }").await,
        "a {\n  width: 5px;\n}\n"
    );
    assert_eq!(
        css("$k: 2; .col-#{$k} { w: 1px; }").await,
        ".col-2 {\n  w: 1px;\n}\n"
    );
    assert_eq!(
        css("a { --d: 4px; width: var(--d); }").await,
        "a {\n  --d: 4px;\n  width: var(--d);\n}\n"
    );
}
