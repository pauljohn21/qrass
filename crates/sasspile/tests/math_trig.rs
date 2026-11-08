//! 三角 `math` 函数，逐行钉在
//! `core_functions/math/{sin,cos,tan,asin,acos,atan}.hrx` 与 `core_functions/math/atan2/*`。
//! 每个输入与期望都逐字抄自 fixture 自己的 `input.scss` / `output.css`。
//!
//! 两条律把这一族连在一起，每条都由成对的行陈述而非推断：
//! * 直接三函数的无单位实参是**弧度**（`math.cos(1)` 与 `math.cos(1rad)` 是同一个
//!   数），角度单位会换算（`1grad` 是 `0.9deg`），其余单位一律点名拒答；
//! * 反三函数以**度**作答，携带的 `-0.0` 保留符号（`math.sin(-0.0)` → `-0`、
//!   `math.atan2(-0.0, 0)` → `-0deg`），而*落到*零上的运算不保留（`math.round(-0.2)` →
//!   `0`，`core_functions/math/round.hrx::up/to_zero`；`hsl(-1 * 0, 100%, 50%)` → `hsl(0, 100%, 50%)`，
//!   `core_functions/color/hsl/four_args/in_gamut.hrx::negative_zero/hue`）—— 所以区分住在运算里，
//!   本文件钉住它的三角一侧。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

async fn failure(input: &str) -> String {
    match compile(input).await {
        Ok(css) => panic!("{input} was expected to fail, but it compiled to:{css}"),
        Err(error) => error.message,
    }
}

/// `sin`/`cos`/`tan` 收角度、给普通数字（`sin.hrx::{deg,grad,rad,turn}`、
/// `cos.hrx::{unitless,rad,deg,grad,turn}`、`tan.hrx::{deg,grad}`）。`math.cos(1)` → `0.5403023059`
/// 等于 `math.cos(1rad)` —— 无单位读法是弧度而非度 —— 且 `math.sin(1turn)` → `0`。
#[tokio::test]
async fn an_angle_in_a_number_out() {
    for (input, want) in [
        ("a {b: math.sin(1deg)}", "0.0174524064"),
        ("a {b: math.sin(1grad)}", "0.0157073173"),
        ("a {b: math.sin(1rad)}", "0.8414709848"),
        ("a {b: math.sin(1turn)}", "0"),
        ("a {b: math.cos(1deg)}", "0.9998476952"),
        ("a {b: math.cos(1grad)}", "0.9998766325"),
        ("a {b: math.cos(1rad)}", "0.5403023059"),
        ("a {b: math.cos(1)}", "0.5403023059"),
        ("a {b: math.cos(1turn)}", "1"),
        ("a {b: math.tan(1deg)}", "0.0174550649"),
        ("a {b: math.tan(1grad)}", "0.0157092553"),
        ("a {b: math.sin(0)}", "0"),
        ("a {b: math.tan(0)}", "0"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 反三函数要求无单位比值、以度作答：`math.atan(1)` → `45deg`、`math.asin(-0.5)` → `-30deg`、
/// `math.acos(0.5)` → `60deg`、`math.acos(1)` → `0deg`（`{asin,acos,atan}.hrx`）。定义域外的
/// 比值是 `NaN`，而语料把它写进 `calc()` 包装、单位以乘以 1 随行：`math.asin(2)` →
/// `calc(NaN * 1deg)`（`asin.hrx::greater_than_one`）、`math.asin(1.000000000001)` →
/// `calc(NaN * 1deg)`（`…::one_fuzzy`）—— 该打印形式与 `tests/math_exponents.rs` 里的无穷一起钉住。
#[tokio::test]
async fn a_ratio_in_a_degree_out() {
    for (input, want) in [
        ("a {b: math.atan(1)}", "45deg"),
        ("a {b: math.atan(-1)}", "-45deg"),
        ("a {b: math.atan(0)}", "0deg"),
        ("a {b: math.asin(0.5)}", "30deg"),
        ("a {b: math.asin(-0.5)}", "-30deg"),
        ("a {b: math.asin(1)}", "90deg"),
        ("a {b: math.acos(0.5)}", "60deg"),
        ("a {b: math.acos(-0.5)}", "120deg"),
        ("a {b: math.acos(1)}", "0deg"),
        ("a {b: math.asin(2)}", "calc(NaN * 1deg)"),
        ("a {b: math.asin(-2)}", "calc(NaN * 1deg)"),
        ("a {b: math.acos(2)}", "calc(NaN * 1deg)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 携带的负零经超越函数仍保留符号：`math.sin(-0.0)` → `-0`、`math.tan(-0.0)` → `-0`、
/// `math.atan(-0.0)` → `-0deg`、`math.asin(-0.0)` → `-0deg`（各文件的 `negative_zero` 行）。
/// 只是*截*到零的值会丢掉它：`math.sin(-0.000000000001)` → `0` 与 `math.tan(-0.000000000001)` → `0`
/// （`{sin,tan}.hrx::negative_zero_fuzzy`），因为 Sass 在第十位小数处截断
/// （`values/numbers/precision.hrx::very_small/*` 为算术陈述了该边界：`math.pow(10, -10)` →
/// `0.0000000001` 而 `2 * math.pow(10, -11)` → `0`）。`math.atan(-0.000000000001)` →
/// `-0.0000000001deg` 恰在两者之间 —— 它换算成的度数（`-5.7e-11`）在第十位仍有一个数字
/// （`atan.hrx::negative_zero_fuzzy`）。
#[tokio::test]
async fn a_carried_negative_zero_keeps_its_sign_a_cut_one_does_not() {
    for (input, want) in [
        ("a {b: math.sin(-0.0)}", "-0"),
        ("a {b: math.tan(-0.0)}", "-0"),
        ("a {b: math.atan(-0.0)}", "-0deg"),
        ("a {b: math.asin(-0.0)}", "-0deg"),
        ("a {b: math.sin(-0.000000000001)}", "0"),
        ("a {b: math.sin(0.000000000001)}", "0"),
        ("a {b: math.tan(-0.000000000001)}", "0"),
        ("a {b: math.tan(0.000000000001)}", "0"),
        ("a {b: math.atan(-0.000000000001)}", "-0.0000000001deg"),
        ("a {b: math.asin(0.000000000001)}", "0.0000000001deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.atan2($y, $x)`：无论实参怎么写，`atan2(1, -1)` → `135deg`
/// （`atan2/arguments.hrx::{named_args,…}`），而同一维度的两个单位先换算 ——
/// `atan2(1cm, -10mm)` 在长度意义上就是 `atan2(1, -1)`（`…::compatible_units`）。符号律来自
/// IEEE，与 fixture 所陈述的完全一致：`atan2(0, -1)` → `180deg`（`atan2/y_zero.hrx`）、
/// `atan2(-1, -1)` → `-135deg` 与 `atan2(1, 1)` → `45deg`（`atan2/y_non_zero_finite.hrx`），
/// 且 `atan2(-0.0, -1)` → `-180deg` 而 `atan2(-0.0, 0)` → `-0deg`（`atan2/y_zero.hrx`）。
///
/// 它的两行**不**断言，都因为缺一条本步之外的律：`atan2(1cm, -10mm)` → `135deg` 要长度因子精确
/// 而非语料的十位小数读数（本实现落在 `135.0000000003`），而 `atan2(1px, 1deg)` →
/// `$x: 1deg and $y: 1px have incompatible units.` 要维度表里根本有 `deg`
/// （`crate::builtins::math::dimension`），那是 `comparable.hrx` 从未要求的。两行保持红，
/// 而 `sin`/`tan` 需要的打印截断已在上方钉住。
#[tokio::test]
async fn atan2_needs_one_dimension_and_keeps_ieee_signs() {
    for (input, want) in [
        ("a {b: math.atan2($y: 1, $x: -1)}", "135deg"),
        ("a {b: math.atan2(1, 1)}", "45deg"),
        ("a {b: math.atan2(-1, -1)}", "-135deg"),
        ("a {b: math.atan2(-0.0, -1)}", "-180deg"),
        ("a {b: math.atan2(-0.0, 0)}", "-0deg"),
        ("a {b: math.atan2(0, -1)}", "180deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 单位拒答，用语料自己的措辞。直接三函数点名可接受的单位
/// （`$number: Expected 1px to have an angle unit (deg, grad, rad, turn).`，`cos.hrx::error/unit`），
/// 反三函数根本不要单位（`$number: Expected 1px to have no units.`，`asin.hrx::error/units`），
/// `atan2` 区分「两个维度」与*一侧无单位*，且总是先点名 `$x`，而非数字按其自己的参数名上报
/// （`$y: "0" is not a number.`）。
#[tokio::test]
async fn each_family_refuses_a_wrong_unit_by_name() {
    for (input, want) in [
        (
            "a {b: math.cos(1px)}",
            "$number: Expected 1px to have an angle unit (deg, grad, rad, turn).",
        ),
        (
            "a {b: math.sin(1px)}",
            "$number: Expected 1px to have an angle unit (deg, grad, rad, turn).",
        ),
        (
            "a {b: math.asin(1px)}",
            "$number: Expected 1px to have no units.",
        ),
        ("a {b: math.cos(\"0\")}", "$number: \"0\" is not a number."),
        ("a {b: math.cos()}", "Missing argument $number."),
        (
            "a {b: math.cos(0, 0)}",
            "Only 1 argument allowed, but 2 were passed.",
        ),
        (
            "a {b: math.atan2(1, 1px)}",
            "$x: 1px and $y: 1 have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.atan2(1px, 1)}",
            "$x: 1 and $y: 1px have incompatible units (one has units and the other doesn't).",
        ),
        ("a {b: math.atan2(\"0\", 0)}", "$y: \"0\" is not a number."),
        ("a {b: math.atan2(0, \"0\")}", "$x: \"0\" is not a number."),
        ("a {b: math.atan2()}", "Missing argument $y."),
        ("a {b: math.atan2(0)}", "Missing argument $x."),
        (
            "a {b: math.atan2(0, 0, 0)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}
