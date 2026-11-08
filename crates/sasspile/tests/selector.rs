//! 结构化头部（任务 0′.7）：选择器是片段，`&` 是其中之一 —— 绝不是在一个渲染出的字符串里
//! 被搜索的字符。这里每条期望都是 `sass-spec` 夹具，逐个注明。

use sasspile::compile;

/// `css/selector/parent.hrx`:`&` 贴着父选择器的三种方式 —— 在根部独占、贴复合选择器（`&.b`）、
/// 贴后缀（`&b`）—— 外加一个完全没有 `&` 的后代情形。
#[tokio::test]
async fn parent_reference_is_spliced_from_pieces() {
    // `alone/first`:根级 `&` 没有可代入的父，而语料仍照印它。
    assert_eq!(
        compile("& {a: b}").await.expect("root &"),
        "& {\n  a: b;\n}\n"
    );
    // `compound`:无空格相连 —— 这正是修剪头部*边缘*空白所买到的，因为 `a` 与 `&.b` 在片段层拼接，
    // 而不是拼接后再规范化。
    assert_eq!(
        compile("a {\n  &.b {c: d}\n}").await.expect("compound"),
        "a.b {\n  c: d;\n}\n"
    );
    // `suffix`
    assert_eq!(
        compile("a {\n  &b {c: d}\n}").await.expect("suffix"),
        "ab {\n  c: d;\n}\n"
    );
    // descendant:没有 `&`，所以父以空格前置
    assert_eq!(
        compile("a {\n  .b {c: d}\n}").await.expect("descendant"),
        "a .b {\n  c: d;\n}\n"
    );
}

/// `css/selector/combinator/has::leading/single/parent` 与 `css/selector/placeholder::pseudoselectors/
/// where/nesting`：嵌在伪类函数实参里的 `&` 在原位拼接，其周围的空白属于复合选择器，不属于拼接。
#[tokio::test]
async fn parent_reference_inside_a_pseudo_class_splices_in_place() {
    assert_eq!(
        compile("a {\n  :has(~ &) {b: c}\n}")
            .await
            .expect(":has(~ &)"),
        ":has(~ a) {\n  b: c;\n}\n"
    );
    assert_eq!(
        compile("a {\n  :where(&) {\n    b: c;\n  }\n}")
            .await
            .expect(":where(&)"),
        ":where(a) {\n  b: c;\n}\n"
    );
}

/// `libsass-closed-issues/issue_1043`：一段整体内容为 `&` 的插值*就是*父选择器 ——
/// `.test` 里的 `.selector#{&}` 打印 `.test .selector.test`，而不是恰好含 `&` 的文本。
#[tokio::test]
async fn interpolation_of_only_the_parent_is_the_parent() {
    assert_eq!(
        compile(".test {\n  .selector#{&} {\n    color: blue;\n  }\n}")
            .await
            .expect("#{&}"),
        ".test .selector.test {\n  color: blue;\n}\n"
    );
}

/// 0′.7 在词素层的后果，是字符串形式无法表达的：终结一行静默注释的那个换行是空白，所以下一行
/// 绑定的 `$name` 仍可找到。从前它会作为 `Word(" ")` 发出，把 `$ids` 藏在假标识符后面，
/// 而其下的引用失败。
#[tokio::test]
async fn a_comment_does_not_disguise_the_next_binding() {
    assert_eq!(
        compile("// value.\n$ids: ();\na { b: $ids }")
            .await
            .expect("binding after a comment"),
        "a {\n  b: ();\n}\n"
    );
}
