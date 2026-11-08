//! 可调用声明不在其书写处执行（任务 4.0，即 1.2d 识别的 P4 前提）：`@mixin`/`@function` 体不产生
//! 任何输出、也不被求值，直到被调用。此前，体自身的参数会在只有调用才提供的环境里被查找，于是
//! 整个程序带着一个 Sass 从不报告的 `Undefined variable` 中止（实测 29 例）。

use sasspile::compile;

/// `core_functions/meta/get_mixin/same_module.hrx::dash_insensitive/dash_to_underscore` —— 逐字的
/// 夹具输入。Sass 对这个程序毫无问题；断言点是 `$v` 在这里不被解析（因而编译不失败）。它仍缺
/// `meta.get-mixin`/`@include`，所以*输出*还不是夹具的 gold CSS —— 那是 P4.2/P5.1，不是本次改动。
#[tokio::test]
async fn a_mixin_body_is_not_evaluated_at_its_declaration() {
    let css = compile("@use \"sass:meta\";\n@mixin add_two($v) {b: $v + 2}\n\na {@include meta.apply(meta.get-mixin(add-two), 10)}")
        .await
        .expect("declaring a mixin must not fail the compile");
    assert!(
        !css.contains("$v + 2"),
        "the mixin body must not be emitted as CSS, got {css}"
    );
}

/// `@function` 声明同样不贡献任何规则，而其周围的规则照常输出 —— 跳过的是体，不是文件。
#[tokio::test]
async fn a_function_declaration_emits_no_rule_but_the_file_continues() {
    assert_eq!(
        compile("@function a() { @return 1 }\nb { c: d }")
            .await
            .expect("a function declaration compiles"),
        "b {\n  c: d;\n}\n"
    );
}

/// 使本改动不会删掉可用输出的边界：`@function --a()` 是一条**纯 CSS** at-rule
/// （`css/function.hrx::lowercase/result/sass_script`），它的体*会*被输出、且不求值。把它当作
/// Sass 声明处理，会吞掉一条今天仍能通过的规则。
#[tokio::test]
async fn a_plain_css_function_at_rule_still_emits_its_body() {
    assert_eq!(
        compile("@function --a() {\n  result: $b;\n}")
            .await
            .expect("plain-CSS at-rule body"),
        "@function --a() {\n  result: $b;\n}\n"
    );
}

/// 这条规则由上面两条推断而来、而非自己的夹具，所以被有意钉住：写在样式规则*内部*的 `@mixin`
/// 不是 CSS 子块，因而它既不沉淀也不冲刷外层规则 —— `b: c` 与 `d: e` 仍是同一条规则
/// （冲刷待处理序列的是开启一个*块*，而它在输出里不开启任何块）。
#[tokio::test]
async fn a_nested_declaration_does_not_split_the_enclosing_rule() {
    assert_eq!(
        compile("a { b: c; @mixin m { x: $v } d: e }")
            .await
            .expect("a mixin inside a rule compiles"),
        "a {\n  b: c;\n  d: e;\n}\n"
    );
}
