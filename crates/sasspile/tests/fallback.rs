//! 错误策略契约：哪些失败走真正的 `Err` 通道，哪些是纯 CSS。
//!
//! P0′.3a 铺设了该通道（`ErrorKind`、来自 `Exec` 内核的 `Observer::error`、不再给失败改名的终结器）。
//! 把 `UndefinedVariable` 翻为致命需要 `FATAL` 上记录的两个语料守卫：块头只解析 `#{ … }`，而特殊调用里
//! 未绑定的 `$x` 是字面文本。这些测试钉住该边界 —— 未绑定引用必须失败，字面形状必须仍能编译。
//! 每条断言都注明陈述它的夹具：一个较早的 `progid:` 情形被丢弃，因为没有夹具支持它
//! （`libsass-closed-issues/issue_931.hrx` 只展示*已绑定*变量在 `#{ … }` 内解析）。

use sasspile::compile;

/// 语料规定的放行：选择器里的转义美元是文本，不是变量引用（`css/selector/escaping::dollar_char`）。
#[tokio::test]
async fn escaped_dollar_in_selector_is_not_a_variable() {
    let css = compile(r"a\\$b { c: d }").await.expect("escape compiles");
    assert!(css.contains(r"\$"), "escape must stay, got {css}");
}

/// 普通值里未绑定的变量必须成为 stream 的 `Err` 通道上的真错误 —— `directives/use/error/member/missing`
/// 与 `core_functions/meta/call.hrx::error/if_args` 都陈述 oracle 头 `Error: Undefined variable.`。
///
/// 该头不携带变量名：语料里名字只出现在头下画出的源码跨度里
/// （`css/comment.hrx::error/loud/interpolation/failure`），那由 P7 7.2 发出。
#[tokio::test]
async fn unbound_variable_travels_the_err_channel() {
    let err = compile("a { b: $undefined }")
        .await
        .expect_err("an unbound reference must fail the compile");
    assert_eq!(err.kind, sasspile::ErrorKind::UndefinedVariable);
    assert_eq!(err.message, "Undefined variable.");
}

/// 翻 `FATAL` 需要的第三个守卫：**纯 CSS at-rule 体**里的声明是 CSS 文本，所以未绑定的 `$x`
/// 不得中止 —— 而同一体仍解析 `#{ … }`，普通样式规则在同一名字上仍失败。
/// 每种形状都是 `css/function.hrx` 的夹具。
#[tokio::test]
async fn plain_css_at_rule_body_keeps_an_unbound_variable_as_text() {
    // `lowercase/`、`uppercase/result/sass_script`、`result/uppercase/sass_script`
    assert_eq!(
        compile("@function --a() {\n  result: $b;\n}")
            .await
            .expect("plain-CSS body is text"),
        "@function --a() {\n  result: $b;\n}\n"
    );
    assert_eq!(
        compile("@FUNCTION --a() {\n  result: $b;\n}")
            .await
            .expect("the at-rule name is case-insensitive"),
        "@FUNCTION --a() {\n  result: $b;\n}\n"
    );
    // `lowercase/result/interpolation`：该体里的插值仍是 SassScript。
    assert_eq!(
        compile("@function --a() {\n  result: #{1 + 1};\n}")
            .await
            .expect("#{ } evaluates inside a plain-CSS body"),
        "@function --a() {\n  result: 2;\n}\n"
    );
    // 边界：样式规则里同一引用是语料的致命情形。
    assert_eq!(
        compile("a {\n  result: $b;\n}")
            .await
            .expect_err("a style rule is not plain CSS")
            .message,
        "Undefined variable."
    );
}

/// 规则的另一半：同一未绑定名字在**特殊**调用里是纯 CSS 文本，不得使编译失败
/// （`css/functions/special/prefixed/lowercase::calc/script_like`）。
#[tokio::test]
async fn unbound_variable_inside_a_special_call_stays_text() {
    assert_eq!(
        compile("a {b: -a-calc($c)}")
            .await
            .expect("special args are CSS"),
        "a {\n  b: -a-calc($c);\n}\n"
    );
    // …而已绑定的仍解析（values/calculation/calc/no_operator::variable/unquoted_string）。
    assert_eq!(
        compile("$a: foobar;\nb {c: calc($a)}")
            .await
            .expect("bound var resolves"),
        "b {\n  c: calc(foobar);\n}\n"
    );
    // 名字匹配忽略大小写：语料以大写陈述同一形状
    // （`css/functions/special/prefixed/uppercase::expression/script_like`、`-C-EXPRESSION($d)`），
    // 同样不得失败。Sass 还会把打印的名字小写化，我们尚不做 —— 断言是不报错，而非精确文本。
    compile("a {b: -C-EXPRESSION($d)}")
        .await
        .expect("a prefixed special call is plain CSS whatever its case");
}
