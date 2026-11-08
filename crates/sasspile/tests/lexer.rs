//! P1 立定的注释规则的词法金样：`// …` 静默注释是**数据**（`Tok::Silent`），各上下文决定
//! 留或弃，而 URL 的 authority 绝不可读作注释。一切唯经唯一的 `compile` 出口观察。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .expect("silent-comment lexing compiles")
}

#[tokio::test]
async fn silent_comment_is_dropped_from_an_ordinary_value() {
    assert_eq!(css("a { b: c // note\n}").await, "a {\n  b: c;\n}\n");
    assert_eq!(css("a { b: c; // note\n}").await, "a {\n  b: c;\n}\n");
}

#[tokio::test]
async fn comment_line_before_a_rule_is_dropped_from_the_selector() {
    assert_eq!(css("// header\na { b: c; }").await, "a {\n  b: c;\n}\n");
}

#[tokio::test]
async fn custom_property_keeps_the_comment_text_verbatim() {
    // Sass 把自定义属性值解析为原始 CSS 文本，所以那里 `//` 不是注释，
    // 而 `;` 仍终止该 declaration（`sass-spec` oracle：
    // `css/custom_properties/trailing_comment`）。
    assert_eq!(
        css("a { --b: c // comment;\n}").await,
        "a {\n  --b: c // comment;\n}\n"
    );
}

#[tokio::test]
async fn url_authority_is_never_a_comment() {
    assert_eq!(
        css("a { background: url(http://example.com/x.png); }").await,
        "a {\n  background: url(http://example.com/x.png);\n}\n"
    );
    assert_eq!(
        css("a { background: url(//cdn.example.com/x.png); }").await,
        "a {\n  background: url(//cdn.example.com/x.png);\n}\n"
    );
}

#[tokio::test]
async fn lone_slash_stays_division_text() {
    assert_eq!(
        css("a { b: 1px / 2px; }").await,
        "a {\n  b: 1px / 2px;\n}\n"
    );
}

/// 引号字符串里的 `$` 是文本，绝不是变量引用的开始：Sass 不在字符串字面量内解析变量，
/// 而语料把 `":c(@#$)"` 这类字符串当普通实参传
/// （`core_functions/selector/extend/simple/pseudo/arg::class/equal`）。词法器跟踪
/// 引号之前，这个形状会对 Sass 接受的程序报 `Undefined variable`
/// （实测为 118 条在辖行）。
#[tokio::test]
async fn dollar_inside_a_quoted_string_is_text() {
    let out = compile(r#"a {b: selector.extend(":c(@#$)", ":c(@#$)", "e")}"#)
        .await
        .expect("a quoted `$` must not be looked up");
    assert!(
        out.contains("selector.extend("),
        "the run keeps its text until calls evaluate (P4), got {out}"
    );
    // 另一种引号形式行为相同，而转义绝不可提前闭合字符串。
    compile(r"a {b: 'cost: $5'}")
        .await
        .expect("single-quoted $");
    compile(r#"a {b: "escaped \" still one string: $x"}"#)
        .await
        .expect("an escaped quote does not end the literal");
}

/// 右花括号可以**从未被打开过就到来**，因为自定义属性的值是原始 CSS 文本：
/// `css/custom_properties/error.hrx::brackets/curly` 写的是 `--prop: };`，回答
/// `Error: unmatched "}".`。那个错误是待办的 P7 工作，所以这里不钉消息 —— 钉住的是
/// 数括号相位绝不可做的事：减穿零。结构化相位现在携带深度（它正是能指出 `@for` 体
/// 在哪结束的相位），而那个计数写成 `depth -= 1` 时，这个输入在 debug 溢出检查下
/// panic —— 这就是它用饱和的原因。
#[tokio::test]
async fn a_closing_brace_that_opened_nothing_does_not_take_the_process_down() {
    let out = compile(".curly {\n  --prop: };\n}")
        .await
        .expect("an unmatched `}` must not panic the delimiter fold");
    // 声明保留其原始文本，而未匹配的括号随之而去。这是本编译器的输出，
    // 实测此例为 `expected error "unmatched \"}\".", got ".curly {\n  --prop: ;\n}"`；
    // 归档自己的答案是那条错误，属 P7 待办工作，所以这里不钉它。
    assert!(
        out.contains("--prop"),
        "the custom property kept no text: {out}"
    );
}

/// **在循环体内被截断**的程序仍须产出它到达过的块。本相位切出 `@for` 块之前，每个 `{`
/// 都会立刻发出一个 `Event::Open`，于是外层块的待结运行可能随后一个块开启而落定；
/// 而未闭合的切口若留在累加器里，反而会静默丢掉那条规则 —— 而丢掉的规则在通过计数里
/// 不留痕迹。也没有任何在辖行走这个截断循环：`ledger failures <run> "directives/for/for::" --count`
/// 在 run `for-fan-out-final` 与 run `for-structure-cut` 都读出 **11** —— 每条红的 `@for` 行
/// 都因自己的理由红（带单位的或分数界、错误消息、`in_declaration`），没有一条是体不平衡。
/// （更宽的针 `directives/for` 读 47，因为其中 36 行是 `directives/forward`，与循环无关 ——
/// 写下的计数必须是针真正选中的那个。）所以守卫得在这里、由实现者写出，而不是从 fixture
/// 继承。未闭合的运行经 `structure::as_block` 交出，与已拒答的 header 走同一构造：
/// 不读区间，不跑体。
#[tokio::test]
async fn a_loop_the_input_never_closes_still_hands_on_the_blocks_it_reached() {
    let out = compile("a { b: 1; @for $i from 1 through 2 { c: $i")
        .await
        .expect("a truncated program must not panic");
    assert!(
        out.contains("b: 1"),
        "the rule the enclosing block had reached was lost: {out}"
    );
}
