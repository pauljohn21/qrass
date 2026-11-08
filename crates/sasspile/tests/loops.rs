//! `@for` 遍历字面量整数区间（tasks 3.2，首批切片）。每条期望对应
//! `sass-spec/directives/for/for.hrx` 的一个段落；边界用例是读取器拒绝臆测的形状，
//! 它们保持今天的行为，而不是产出一个貌似合理却错误的展开。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `inclusive_forward/scss` + `inclusive_backward` + `exclusive_forward`：`through` 保留端点，
/// `to` 丢弃端点，降序区间向下走。循环自身不贡献选择器，也不分裂它所在的规则——
/// 迭代落入该规则的声明序列。
#[tokio::test]
async fn for_walks_a_literal_range_into_the_enclosing_rule() {
    assert_eq!(
        css("a {\n  @for $i from 1 through 5 {b: $i;}\n}").await,
        "a {\n  b: 1;\n  b: 2;\n  b: 3;\n  b: 4;\n  b: 5;\n}\n"
    );
    assert_eq!(
        css("a {\n  @for $i from 5 through 1 {b: $i;}\n}").await,
        "a {\n  b: 5;\n  b: 4;\n  b: 3;\n  b: 2;\n  b: 1;\n}\n"
    );
    assert_eq!(
        css("a {\n  @for $i from 1 to 5 {b: $i;}\n}").await,
        "a {\n  b: 1;\n  b: 2;\n  b: 3;\n  b: 4;\n}\n"
    );
    // `negative_to_positive` / `negative_to_negative`：前导负号是边界词法的一部分。
    assert_eq!(
        css("a { @for $i from -1 through 2 {b: $i;} }").await,
        "a {\n  b: -1;\n  b: 0;\n  b: 1;\n  b: 2;\n}\n"
    );
}

/// `empty/scss`：空区间不贡献任何内容，周围声明保持在同一规则内
/// （循环不是 CSS 子块，因此不能在它附近冲刷待落定的声明序列）。
#[tokio::test]
async fn an_empty_range_and_its_neighbours() {
    assert_eq!(
        css("a { b: 0; @for $i from 1 to 1 {b: $i;} d: 9; }").await,
        "a {\n  b: 0;\n  d: 9;\n}\n"
    );
}

/// 函数体中的两条声明**按迭代**交错——这正是重放绑定变量的顺序。
#[tokio::test]
async fn body_declarations_keep_their_order_within_each_iteration() {
    assert_eq!(
        css("a { @for $i from 1 to 3 { x: $i; y: $i; } }").await,
        "a {\n  x: 1;\n  y: 1;\n  x: 2;\n  y: 2;\n}\n"
    );
}

/// **引用** 绑定的边界会被读取。`for.hrx::to_scope` 写 `@for $i from 1 through $limit`，并在
/// 自身注释中声明在体内重新绑定 `$limit` 不会移动走步的终点，因此区间在头部一次性决定。这里在
/// 规则内产生的展开已由字面量拼写（`…::inclusive_forward/scss`）按值钉住，所以此处断言的是
/// **两种拼写的相等性**，而不是本编译器为 fixture 以不同方式书写的形状发明的输出串。
#[tokio::test]
async fn a_reference_bound_reads_the_same_range_as_the_literal_it_holds() {
    let literal = css("a { @for $i from 1 through 3 { b: $i; } }").await;
    let reference = css("$limit: 3;\na { @for $i from 1 through $limit { b: $i; } }").await;
    assert_eq!(literal, reference);
}

/// 持有块的函数体**每次迭代**落定一条规则：`directives/for/for.hrx::to_scope`。
/// 下面的输入与期望 CSS 均逐字摘抄自该段落，包括 `through $limit` 边界与体自身的
/// `$limit: 2`——该用例的注释声明走步在头部一次性决定，然而四次迭代也是引用边界
/// 确实被读取的唯一在域证据。
///
/// 比较前双方都丢弃空行。这是一致性测试台自身的基准
/// （`tests/hrx.rs` 的 `norm`，不以空行判断规则文本）；fixture 在规则*之间*打印的
/// 空行是本编译器尚未发出的格式律，此处不断言。
#[tokio::test]
async fn a_body_block_settles_one_rule_per_iteration() {
    let unrolled = |text: &str| {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let got = css(
        "$limit: 4;\n@for $i from 1 through $limit {\n  $limit: 2;\n  a {\n    limit: $limit;\n    i: $i;\n  }\n}",
    )
    .await;
    let want = "a {\n  limit: 2;\n  i: 1;\n}\n\na {\n  limit: 2;\n  i: 2;\n}\n\na {\n  limit: 2;\n  i: 3;\n}\n\na {\n  limit: 2;\n  i: 4;\n}\n";
    assert_eq!(unrolled(&got), unrolled(want));
}

/// 读取器仍不会跨越的边界：**非整数** 边界使头部无法被识别，因此不会臆造区间。
/// `1.5` 是 `error/from_float` 的用例，语料库要求
/// `Error: 1.5 is not an int.`——发出该消息是 P7 的工作，在那之前程序保持
/// 循环前的行为，而不是臆测区间或从分数边界里猜一个数。
#[tokio::test]
async fn a_fractional_bound_is_not_read_as_a_range() {
    let float = compile("a { @for $i from 1.5 through 2 { b: $i; } }")
        .await
        .expect_err("a float bound is not read as an integer range");
    assert_eq!(float.message, "Undefined variable.");
}
