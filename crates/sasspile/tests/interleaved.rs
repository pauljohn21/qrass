//! Document-order contract, straight from `sass-spec/css/style_rule.hrx`
//! `declaration/interleaved/**`: a block's declaration runs settle **before** a nested block that
//! follows them, and whatever a block still holds settles when it closes. The chain is serial up to
//! `Render`, so this ordering is produced by the stream, not restored by a sort.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input.to_string())
        .await
        .expect("interleaved compiles")
}

#[tokio::test]
async fn declarations_before_a_nested_rule_settle_first() {
    // spec: css/style_rule declaration/interleaved/before_style_rule
    assert_eq!(
        css(".a {\n  b: c;\n  .d {e: f}\n}").await,
        ".a {\n  b: c;\n}\n.a .d {\n  e: f;\n}\n"
    );
}

#[tokio::test]
async fn a_block_around_a_nested_rule_becomes_two_rules() {
    // spec: css/style_rule declaration/interleaved/around_style_rule
    assert_eq!(
        css(".a {\n  b: c;\n  .d {e: f}\n  g: h;\n}").await,
        ".a {\n  b: c;\n}\n.a .d {\n  e: f;\n}\n.a {\n  g: h;\n}\n"
    );
}

#[tokio::test]
async fn declarations_after_a_nested_rule_settle_with_the_closing_block() {
    // spec: css/style_rule declaration/interleaved/after_style_rule/higher_specificity
    assert_eq!(
        css(".a {\n  .b {c: d}\n  e: f;\n}").await,
        ".a .b {\n  c: d;\n}\n.a {\n  e: f;\n}\n"
    );
    // spec: css/style_rule declaration/interleaved/after_style_rule/same_specificity
    assert_eq!(
        css(".a {\n  :where(.b) {c: d}\n  e: f;\n}").await,
        ".a :where(.b) {\n  c: d;\n}\n.a {\n  e: f;\n}\n"
    );
}
