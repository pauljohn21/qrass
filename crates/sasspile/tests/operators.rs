//! 标识符拼接（tasks 1.3b）：`+` 与 `-` 在两侧操作数都是非引号标识符时的行为。下面每条期望
//! 都是 `sass-spec/operators/{plus,minus}.hrx` 的一节，而四种空白形状加上响注释形状被逐一
//! 列举，是因为 fixture 分别陈述每一种 —— Sass 把它们全部折叠成同一个结果。

use sasspile::compile;

async fn value(input: &str) -> String {
    let css = compile(input).await.unwrap_or_else(|e| {
        panic!("concatenation compiles: {input} failed with {}", e.message);
    });
    // `a {b: <value>;}` 是每个 fixture 都用的形状；只回值。
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

/// `operators/plus.hrx::syntax/*` —— `+` 整个消失，无论周围是什么。
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

/// `operators/minus.hrx::syntax/*` —— `-` **留在**文本里且没有空格存活，而整段加括号的
/// 操作数被剥开（`(c)-(d)` → `c-d`）。
#[tokio::test]
async fn minus_between_identifiers_keeps_the_sign_and_drops_the_spaces() {
    assert_eq!(value("a {b: (c)-(d)}").await, "c-d");
    assert_eq!(value("a {b: c - d}").await, "c-d");
    assert_eq!(value("a {b: c -(d)}").await, "c-d");
    assert_eq!(value("a {b: (c)- d}").await, "c-d");
    assert_eq!(value("a {b: c/**/-/**/d}").await, "c-d");
}

/// 保证这一折叠不去吞它不该读的形状的边界。下面每种情形仍逐字渲染，因为拼接折叠拒绝触发：
/// * **数字**操作数把运行送回算术路径，那里空白律管着，所以 `10px -5px` 保持它今天的两项列表
///   （`-` 被粘进后一个词素，所以操作符 token 从不出现）；
/// * `1px+2px` 也不是标识符拼接，在 P1 的单位工作落地前保持未求值；
/// * 单独标识符根本没有操作符；
/// * `c-d` 是一个词素（词法器把标识符的连字符合上），从来不是运算；
/// * `(c)(d)` 是两个分组，不是单个平衡包装，所以没什么可剥。
#[tokio::test]
async fn numbers_and_lone_lexemes_keep_their_old_reading() {
    assert_eq!(value("a {b: 10px -5px}").await, "10px -5px");
    assert_eq!(value("a {b: 1px+2px}").await, "1px+2px");
    assert_eq!(value("a {b: c}").await, "c");
    assert_eq!(value("a {b: c-d}").await, "c-d");
    assert_eq!(value("a {b: (c)(d)}").await, "(c)(d)");
}
