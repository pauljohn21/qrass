//! 文档序契约，直接取自 `sass-spec/css/style_rule.hrx` 的
//! `declaration/interleaved/**`：一块的声明运行先于其后的嵌套块**沉淀**，而块仍持有的一切
//! 在它关闭时沉淀。链条直到 `Render` 都是串行的，所以这个次序由 stream 产生，
//! 而不是靠排序恢复。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input.to_string())
        .await
        .expect("interleaved compiles")
}

#[tokio::test]
async fn declarations_before_a_nested_rule_settle_first() {
    // 夹具: css/style_rule declaration/interleaved/before_style_rule
    assert_eq!(
        css(".a {\n  b: c;\n  .d {e: f}\n}").await,
        ".a {\n  b: c;\n}\n.a .d {\n  e: f;\n}\n"
    );
}

#[tokio::test]
async fn a_block_around_a_nested_rule_becomes_two_rules() {
    // 夹具: css/style_rule declaration/interleaved/around_style_rule
    assert_eq!(
        css(".a {\n  b: c;\n  .d {e: f}\n  g: h;\n}").await,
        ".a {\n  b: c;\n}\n.a .d {\n  e: f;\n}\n.a {\n  g: h;\n}\n"
    );
}

#[tokio::test]
async fn declarations_after_a_nested_rule_settle_with_the_closing_block() {
    // 夹具: css/style_rule declaration/interleaved/after_style_rule/higher_specificity
    assert_eq!(
        css(".a {\n  .b {c: d}\n  e: f;\n}").await,
        ".a .b {\n  c: d;\n}\n.a {\n  e: f;\n}\n"
    );
    // 夹具: css/style_rule declaration/interleaved/after_style_rule/same_specificity
    assert_eq!(
        css(".a {\n  :where(.b) {c: d}\n  e: f;\n}").await,
        ".a :where(.b) {\n  c: d;\n}\n.a {\n  e: f;\n}\n"
    );
}
