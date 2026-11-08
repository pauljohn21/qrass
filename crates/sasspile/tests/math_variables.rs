//! `math` 模块自己的常数 —— `math.$pi`、`math.$max-number` —— 钉在
//! `core_functions/math/variables.hrx` 上。输入与期望逐字抄自该文件。
//!
//! 本切片读到的是：`module.$name` 引用是**一个词素**（`crate::lexer::Tok::MemberVar`），
//! 因为`.`结尾的词旁边跟一个 `$name` 时，下游无法重新接合 —— `math . $pi`、`5.$pi` 与
//! `math.$pi` 在 `expr` 看来都一个样，而在那儿重扫渲染文本正是本项目已量到四次的错误。
//!
//! 仍未做：本 fixture 无 —— `variables.hrx` 的 7 条取值行与其 7 条赋值行都在下方钉住；
//! 后一组正是模块表只读律的用途。

use sasspile::compile;

async fn failure(input: &str) -> String {
    match compile(input).await {
        Ok(css) => panic!("{input} was expected to fail, but it compiled to:{css}"),
        Err(error) => error.message,
    }
}

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// 不经任何算术就能打印的常数：`math.$max-safe-integer` → `9007199254740991` 与
/// `math.$min-safe-integer` → `-9007199254740991`（`variables.hrx::{max_safe_integer,min_safe_integer}`）。
/// 该文件第三行同类 `math.$max-number` 留给 hrx 跑而不在此抄写：它的答案是最大 double 的 309 位数字，
/// 而重敲一个数字串正是本会话两次凭空造出 fixture 的方式 —— 夹具是拿文件本身做比的。
#[tokio::test]
async fn the_safe_integer_constants_print_whole() {
    for (input, want) in [
        ("a {b: math.$max-safe-integer}", "9007199254740991"),
        ("a {b: math.$min-safe-integer}", "-9007199254740991"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 常数乘**指数字面量**是算术而非插值：
/// `math.$pi * 1e15` → `3141592653589793`、`math.$e * 1e15` → `2718281828459045`、
/// `math.$epsilon * 1e31` → `2220446049250313`、
/// `math.$min-number * 1e300 * 1e39` → `4940656458412465`（`variables.hrx::{pi,e,epsilon,min_number}`）。
/// 这些行交会两条律：`1e15` 是一个数而不是「1 带单位 `e15`」
/// （`operations::number_lexeme`），且持模块常数的运行先到达
/// [`crate::operations::arithmetic`] 再到插值分支 —— 同一件事也使
/// `math.tan(0.5rad * math.$pi)` → `16331239353195370` 能折叠其实参
/// （`core_functions/math/tan.hrx::{asymptote,negative_asymptote}/radian`）。
/// `calc(1e2px)` → `100px` 是从 `calc` 一侧看到的同一条词素律
/// （`values/calculation/calc/no_operator::number/exponent`）。
#[tokio::test]
async fn a_constant_times_an_exponent_literal_folds() {
    for (input, want) in [
        ("a {b: math.$pi * 1e15}", "3141592653589793"),
        ("a {b: math.$e * 1e15}", "2718281828459045"),
        ("a {b: math.$epsilon * 1e31}", "2220446049250313"),
        ("a {b: math.$min-number * 1e300 * 1e39}", "4940656458412465"),
        ("a {b: calc(1e2px)}", "100px"),
        ("a {b: math.tan(0.5rad * math.$pi)}", "16331239353195370"),
        ("a {b: math.tan(-0.5rad * math.$pi)}", "-16331239353195370"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 内建模块的常数只读：`math.$pi: 0;` → `Error: Cannot modify built-in variable.`
/// （`variables.hrx::error/assignment/*`，每个常数一行）。这条拒答要问模块表，所以表不拥有的
/// 名字仍是普通属性 —— 没有任何 fixture 给 `upstream.$a: 0;` 这条消息。
#[tokio::test]
async fn a_built_in_module_member_refuses_assignment() {
    for name in [
        "$pi",
        "$e",
        "$epsilon",
        "$max-safe-integer",
        "$min-safe-integer",
        "$max-number",
        "$min-number",
    ] {
        assert_eq!(
            failure(&format!("math.{name}: 0;")).await,
            "Cannot modify built-in variable.",
            "for math.{name}"
        );
    }
}

/// 没有内建成员的命名空间名字 —— 无论从文件载入的模块（`upstream.$a`，P5）还是凭空造的 ——
/// 都被安静地拒答：该运行保留文本，因为 `Undefined variable.` 不是语料对它所陈述的，
/// 而答 `0` 会是一个没人要的值。
#[tokio::test]
async fn a_name_with_no_built_in_member_keeps_its_text() {
    for call in ["math.$nope", "upstream.$a", "utils.$empty-map"] {
        let input = format!("a {{b: {call}}}");
        assert_eq!(
            css(&input).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unresolvable module member must survive verbatim"
        );
    }
}
