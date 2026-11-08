//! `math.pow`、`math.sqrt`、`math.log` 与**非有限数的打印形式**，钉在
//! `core_functions/math/pow/*.hrx`、`core_functions/math/{sqrt,log}.hrx` 以及
//! `{sin,cos,tan,asin,acos,atan,atan2}.hrx` 的 `NaN` 行上。每个输入与期望都逐字抄自
//! fixture 自己的 `input.scss` / `output.css`（拒答取 `Error:` 首行）。
//!
//! 这条打印律一次解锁了 179 行：非有限数没有普通 CSS 拼法，所以 Sass 把它写进 `calc()`，
//! 而单位**在**包装内、以乘以 1 的形式随行 —— `calc(NaN)`、`calc(infinity)`、
//! `calc(-infinity)`、`calc(NaN * 1deg)`、`calc(infinity * 1px)`。有限数保持普通形式，
//! 所以 `math.pow(2, 3)` → `8` 而不是 `calc(8)`。

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

/// `math.pow($base: 2, $exponent: 3)` → `8`（`pow/arguments.hrx::named_args`），而零/有符号情形走
/// IEEE：`pow(0, -2)` → `calc(infinity)`（`pow/base_positive_zero.hrx::with_exponent/negative_even_integer`），
/// `pow(-0.0, -3)` → `calc(-infinity)` 而 `pow(-0.0, -3.000000000001)` → `calc(infinity)` ——
/// 奇**整数**反转零的符号，非整数指数不能（`pow/base_negative_zero.hrx::with_exponent/{negative_odd_integer,negative_odd_integer_fuzzy}`）。
#[tokio::test]
async fn pow_follows_ieee_through_zero_and_the_infinities() {
    for (input, want) in [
        ("a {b: math.pow($base: 2, $exponent: 3)}", "8"),
        ("a {b: math.pow(0, -2)}", "calc(infinity)"),
        ("a {b: math.pow(-0.0, -3)}", "calc(-infinity)"),
        ("a {b: math.pow(-0.0, -3.000000000001)}", "calc(infinity)"),
        ("a {b: math.pow(0, -0.5)}", "calc(infinity)"),
        ("a {b: math.pow(1, math.div(-1, 0))}", "1"),
        (
            "a {b: math.pow(1.000000000001, math.div(1, 0))}",
            "calc(infinity)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.sqrt`：`-0.0` 保留符号（`sqrt.hrx::negative_zero` → `-0`），而负**数**是
/// `calc(NaN)`（`…::negative`，`-0.000000000001` 走 `…::negative_zero_fuzzy`），且模糊零
/// 虽小却真实：`math.sqrt(0.000000000001)` → `0.000001`（`…::zero_fuzzy`）。
#[tokio::test]
async fn sqrt_keeps_a_signed_zero_and_refuses_a_negative() {
    for (input, want) in [
        ("a {b: math.sqrt(-0.0)}", "-0"),
        ("a {b: math.sqrt(0)}", "0"),
        ("a {b: math.sqrt(-1)}", "calc(NaN)"),
        ("a {b: math.sqrt(-0.000000000001)}", "calc(NaN)"),
        ("a {b: math.sqrt(0.000000000001)}", "0.000001"),
        ("a {b: math.sqrt(2)}", "1.4142135624"),
        ("a {b: math.sqrt($number: 2)}", "1.4142135624"),
        ("a {b: math.sqrt(math.div(1, 0))}", "calc(infinity)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.log` 默认取自然对数，除非来了底数，而写出 `null` **就是**没有底数：
/// `math.log(2, null)` → `0.6931471806` 等于 `math.log(2)`（`log.hrx::base/null`），
/// `math.log(2, 10)` → `0.3010299957`（`…::base/positive`），而极端值经 `calc` 打印：
/// `math.log(0)` → `calc(-infinity)`（`…::base/zero`），`math.log(-1)` → `calc(NaN)`（`…::base/negative`）。
#[tokio::test]
async fn log_takes_an_optional_base_and_prints_its_extremes_in_calc() {
    for (input, want) in [
        ("a {b: math.log(2)}", "0.6931471806"),
        ("a {b: math.log($number: 2)}", "0.6931471806"),
        ("a {b: math.log(2, null)}", "0.6931471806"),
        ("a {b: math.log(2, 10)}", "0.3010299957"),
        ("a {b: math.log($number: 2, $base: 10)}", "0.3010299957"),
        ("a {b: math.log(0)}", "calc(-infinity)"),
        ("a {b: math.log(-1)}", "calc(NaN)"),
        ("a {b: math.log(0.000000000001)}", "-27.6310211159"),
        ("a {b: math.log(math.div(1, 0))}", "calc(infinity)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 拒答点名出错的参数，措辞与该域其余部分一致
/// （`pow/arguments.hrx::error/{base_type,exponent_type,base_has_units,exponent_has_units,zero_args,one_arg,too_many_args}`、
/// `sqrt.hrx::error/*`、`log.hrx::error/*` —— 含 `math.log` 可选参数的 `$base:`）。
#[tokio::test]
async fn every_argument_is_refused_under_its_own_name() {
    for (input, want) in [
        ("a {b: math.pow(\"0\", 0)}", "$base: \"0\" is not a number."),
        (
            "a {b: math.pow(0, \"0\")}",
            "$exponent: \"0\" is not a number.",
        ),
        (
            "a {b: math.pow(1px, 0)}",
            "$base: Expected 1px to have no units.",
        ),
        (
            "a {b: math.pow(0, 1px)}",
            "$exponent: Expected 1px to have no units.",
        ),
        ("a {b: math.pow()}", "Missing argument $base."),
        ("a {b: math.pow(0)}", "Missing argument $exponent."),
        (
            "a {b: math.pow(0, 0, 0)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
        (
            "a {b: math.sqrt(1px)}",
            "$number: Expected 1px to have no units.",
        ),
        ("a {b: math.sqrt(\"0\")}", "$number: \"0\" is not a number."),
        ("a {b: math.sqrt()}", "Missing argument $number."),
        (
            "a {b: math.sqrt(0, 0)}",
            "Only 1 argument allowed, but 2 were passed.",
        ),
        (
            "a {b: math.log(1px)}",
            "$number: Expected 1px to have no units.",
        ),
        (
            "a {b: math.log(1, 1px)}",
            "$base: Expected 1px to have no units.",
        ),
        ("a {b: math.log()}", "Missing argument $number."),
        (
            "a {b: math.log(0, 0, 0)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// 打印律本身，取自那些经参数而非域拒答到达它的函数：`math.div(1, 0)` 是无穷，而
/// 对无穷取 `sin`/`cos`/`tan` 是 `calc(NaN)`（`{sin,cos,tan}.hrx::{infinity,negative_infinity}`），
/// 而 `math.atan2(-1, 0)` 保留一个真实的度数答案（`atan2/y_infinite.hrx`）。
#[tokio::test]
async fn a_non_finite_number_prints_inside_calc() {
    for (input, want) in [
        ("a {b: math.sin(math.div(1, 0))}", "calc(NaN)"),
        ("a {b: math.sin(math.div(-1, 0))}", "calc(NaN)"),
        ("a {b: math.cos(math.div(1, 0))}", "calc(NaN)"),
        ("a {b: math.tan(math.div(-1, 0))}", "calc(NaN)"),
        ("a {b: math.atan(math.div(1, 0))}", "90deg"),
        ("a {b: math.atan2(-1, 0)}", "-90deg"),
        ("a {b: math.round(1.4999999999949998)}", "1"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
