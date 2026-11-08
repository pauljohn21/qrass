//! 绑定上的 declaration 旗标（`variables/double_flag.hrx` 两件）：`!default` / `!global`
//! 描述的是*绑定*，所以它们绝不是值的一部分、也绝不进 CSS。第三个测试是把这条守成
//! 非通用「吃掉 `!`」律的边界。

use sasspile::compile;

/// `variables/double_flag.hrx::default` —— 重复的旗标被弃用但仍解析，且两份副本都
/// 从绑定值里消失。
#[tokio::test]
async fn default_flag_is_not_part_of_the_bound_value() {
    assert_eq!(
        compile("$a: b !default !default;\n\nc {d: $a}")
            .await
            .expect("double flag parses"),
        "c {\n  d: b;\n}\n"
    );
}

/// `variables/double_flag.hrx::global` —— `!global` 同理，其*作用域*含义归 P3.6：今天在
/// 扁平环境下可见结果完全相同，而 fixture 陈述的正是这段文本。
#[tokio::test]
async fn global_flag_is_not_part_of_the_bound_value() {
    assert_eq!(
        compile("$a: b;\nc {\n  $a: d !global !global;\n  e: $a\n}")
            .await
            .expect("double global flag parses"),
        "c {\n  e: d;\n}\n"
    );
}

/// 边界：旗标只从**绑定的**值里剥。自定义属性是原始 CSS 文本，所以 `!important` 甚至裸 `!`
/// 都按原样存活 —— 这就是 `css/custom_properties/exclamation.hrx` fixture，它在通过的
/// 基线里，必须保持如此。
#[tokio::test]
async fn a_flag_survives_in_a_custom_property_value() {
    assert_eq!(
        compile(".x {\n  --important: value !important;\n  --multiple: !important !important;\n}")
            .await
            .expect("custom property compiles"),
        ".x {\n  --important: value !important;\n  --multiple: !important !important;\n}\n"
    );
}
