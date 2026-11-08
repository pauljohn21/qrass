//! 内建调用主干（tasks 1.7a），钉在 `core_functions/math/div.hrx` 的金样上：实参拆分、`$label:` 绑定、
//! 除法的单位律，以及两条签名无关的 arity 消息。最后一个测试是边界 —— 一个我们没有签名的调用、
//! 或一次本编译器尚不能建模其值的运行，必须保留逐字文本，而不是被半求值。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `unitless/{whole,fraction}` + `unit/{numerator,same}`：同单位相消，无单位的除数保留
/// 被除数的单位，而单独的数字实参在调用内仍是一个表达式。
#[tokio::test]
async fn div_evaluates_number_arguments() {
    assert_eq!(css("a {b: math.div(6, 3)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.div(6, 5)}").await, "a {\n  b: 1.2;\n}\n");
    assert_eq!(css("a {b: math.div(6px, 3)}").await, "a {\n  b: 2px;\n}\n");
    assert_eq!(css("a {b: math.div(6px, 3px)}").await, "a {\n  b: 2;\n}\n");
}

/// `unit/compatible`：可换算的单位先换算再相消，而 `1in = 96px` 是语料唯一佐证的
/// 因子（`math.div(6in, 3px)` → `192`）。
#[tokio::test]
async fn div_converts_lengths_it_can_prove() {
    assert_eq!(
        css("a {b: math.div(6in, 3px)}").await,
        "a {\n  b: 192;\n}\n"
    );
}

/// `unitless/named`：`$label:` 实参按参数名绑定，写在哪儿都一样 —— 而标签本身
/// 绝不作为变量查表（1.2b 的拒答，现在经由签名解决）。
#[tokio::test]
async fn named_arguments_bind_by_label() {
    assert_eq!(
        css("a {b: math.div($number2: 3, $number1: 6)}").await,
        "a {\n  b: 2;\n}\n"
    );
    assert_eq!(
        css("a {b: math.div(6, $number2: 3)}").await,
        "a {\n  b: 2;\n}\n"
    );
}

/// `error/{too_few_args,too_many_args}`：消息由签名给出，而夹具比较 oracle 的 `Error:` 头，
/// 所以这些必须逐字匹配 —— 包括英文复数形式，语料显示两个计数各自独立地以复数
/// 形式措辞。
#[tokio::test]
async fn arity_diagnostics_are_the_oracle_words() {
    for (input, want) in [
        ("a {b: math.div(6)}", "Missing argument $number2."),
        (
            "a {b: math.div(6, 3, 1)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
        (
            "a {b: math.div(6, 3, 1, 2)}",
            "Only 2 arguments allowed, but 4 were passed.",
        ),
    ] {
        let err = compile(input)
            .await
            .expect_err("an arity mistake must terminate the stream as an error");
        assert_eq!(err.message, want, "for input {input}");
    }
}

/// `core_functions/math/{round,abs,ceil,floor}.hrx` —— 主干使其廉价的取整家族：每个一个
/// `Builtin`，同名参数，无单位结果精确匹配。`round` 在平点处远离零断开（`-5.6` → `-6`），
/// 而 Sass 精度内恰低于 `.5` 的值仍给出 `1`。
#[tokio::test]
async fn rounding_family_matches_goldens() {
    assert_eq!(css("a {b: math.round(2.2)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.round(-5.6)}").await, "a {\n  b: -6;\n}\n");
    assert_eq!(css("a {b: math.round(0.2)}").await, "a {\n  b: 0;\n}\n");
    assert_eq!(
        css("a {b: math.round(1.4999999999949998)}").await,
        "a {\n  b: 1;\n}\n"
    );
    assert_eq!(css("a {b: math.abs(-17)}").await, "a {\n  b: 17;\n}\n");
    assert_eq!(
        css("a {b: math.abs(-123.456)}").await,
        "a {\n  b: 123.456;\n}\n"
    );
    assert_eq!(css("a {b: math.ceil(1.2)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.floor(-1.2)}").await, "a {\n  b: -2;\n}\n");
}

/// 类型契约**逐函数**不同，而这么说的正是语料：`round` 要
/// `$number: c is not a number.`（值按 CSS 形式渲染 —— 裸标识符不加引号、字符串加引号），
/// 而 `div.hrx::non_numeric/numerator` 要的是一条弃用**警告**与真实输出。
/// 所以 `div` 在非数字上拒答，而不是编造一条 oracle 未声明的错误。
#[tokio::test]
async fn number_parameters_validate_their_own_way() {
    for (input, want) in [
        ("a {b: math.round(c)}", "$number: c is not a number."),
        ("a {b: math.ceil(\"0\")}", "$number: \"0\" is not a number."),
        ("a {b: math.abs()}", "Missing argument $number."),
    ] {
        let err = compile(input)
            .await
            .expect_err("a non-number argument to a number parameter is an error");
        assert_eq!(err.message, want, "for input {input}");
    }
    assert_eq!(
        css("a {b: math.div(c, 3)}").await,
        "a {\n  b: math.div(c, 3);\n}\n"
    );
}

/// 可变实参的这一对。`min`/`max` 按单位律比较并返回**原始**实参（绝不重写成公共单位的值）：
/// `max(2px, 1)` → `2px`（无单位实参可比较），`max(1px, 1in, 1cm)` → `1in`
/// （长度按 oracle 自己在 `libsass/units/conversion/size.hrx` 里声明的因子缩放）。
/// 它们的消息与固定 arity 签名不同：没有 `$parameter:` 前缀（`c is not a number.`），
/// 以及 `At least one argument must be passed.`。
#[tokio::test]
async fn variadic_extremes_compare_units_and_keep_the_original() {
    assert_eq!(css("a {b: math.max(1, 2)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.max(3, 1, 2)}").await, "a {\n  b: 3;\n}\n");
    assert_eq!(css("a {b: math.max(1)}").await, "a {\n  b: 1;\n}\n");
    assert_eq!(css("a {b: math.max(2px, 1)}").await, "a {\n  b: 2px;\n}\n");
    assert_eq!(
        css("a {b: math.max(1px, 1in, 1cm)}").await,
        "a {\n  b: 1in;\n}\n"
    );
    assert_eq!(
        css("a {b: math.min(1px, 1in, 1cm)}").await,
        "a {\n  b: 1px;\n}\n"
    );
    for (input, want) in [
        ("a {b: math.max()}", "At least one argument must be passed."),
        ("a {b: math.min(1, c)}", "c is not a number."),
        (
            "a {b: math.max(1px, 2s)}",
            "1px and 2s have incompatible units.",
        ),
    ] {
        let err = compile(input)
            .await
            .expect_err("a variadic arity/type/unit mistake is an error");
        assert_eq!(err.message, want, "for input {input}");
    }
}

/// 同一代码路径上的另两道守卫，都关于**不猜**：共用换算表之外的单位不是冲突的证据
/// （`max.hrx` 只把 `px` 与 `s` 对照），而带 `$label:` 的可变实参调用没有参数可绑，所以保留文本。
///
/// 未列出的例子曾是 `2q`；`hypot.hrx::units` 把 `q` 放进了表（它在那里对 `cm` 与 `in` 换算），
/// 所以现在按它本身就是的那段绝对长度比较 —— 而 `rem`/`ch` 代表语料尚未佐证的情形，
/// 因为没有 fixture 换算它们中的任何一个。
#[tokio::test]
async fn unknown_units_and_labels_on_a_variadic_decline() {
    assert_eq!(
        css("a {b: math.max(1px, 2rem)}").await,
        "a {\n  b: math.max(1px, 2rem);\n}\n"
    );
    assert_eq!(css("a {b: math.max(1px, 2q)}").await, "a {\n  b: 2q;\n}\n");
    assert_eq!(
        css("a {b: math.max($number1: 1, 2)}").await,
        "a {\n  b: math.max($number1: 1, 2);\n}\n"
    );
}

/// 返回布尔的这一族，它需要 `Value::Bool`（以及语料自己的名字拼法：文件是 `unitless.hrx`，
/// 函数是 `math.is-unitless`）。`math.compatible` 只对维度被佐证的单位作答 ——
/// `px` 对 `em` 是 `false`（`comparable.hrx::unit/to_different`），`px` 对 `q` 现在是 `true`，
/// 因为 `hypot.hrx::units` 换算 `q`；而没有任何 fixture 换算的单位（`ch`）拒答为逐字文本，
/// 而不是猜。
#[tokio::test]
async fn booleans_and_unit_strings() {
    assert_eq!(
        css("a {b: math.is-unitless(1px)}").await,
        "a {\n  b: false;\n}\n"
    );
    assert_eq!(css("a {b: math.unit(1px)}").await, "a {\n  b: \"px\";\n}\n");
    assert_eq!(css("a {b: math.unit(1)}").await, "a {\n  b: \"\";\n}\n");
    assert_eq!(
        css("a {b: math.compatible(1px, 2in)}").await,
        "a {\n  b: true;\n}\n"
    );
    assert_eq!(
        css("a {b: math.compatible(1px, 2em)}").await,
        "a {\n  b: false;\n}\n"
    );
    assert_eq!(
        css("a {b: math.compatible(1px, 2q)}").await,
        "a {\n  b: true;\n}\n"
    );
    assert_eq!(
        css("a {b: math.compatible(1px, 2ch)}").await,
        "a {\n  b: math.compatible(1px, 2ch);\n}\n"
    );
}

/// `math.percentage` 是 Sass 的**十位有效数字**输出变得可见的地方：`0.246 × 100` 必须
/// 打印 `24.6`，而不是 f64 的尾巴。`format_number` 的夹紧只在自然渲染长过十个有效数字时
/// 生效，所以短的值保留文本（且落地时没有任何当前通过的用例移动 —— 实测 `regress=0`）。
/// 实参带单位时的消息是语料规定的，即使只有一个也写作复数 “units”，与 fixture
/// 逐字一致。
#[tokio::test]
async fn percentage_scales_and_respects_precision() {
    for (input, want) in [
        ("a {b: math.percentage(42)}", "4200%"),
        ("a {b: math.percentage(0.246)}", "24.6%"),
        ("a {b: math.percentage(123.456)}", "12345.6%"),
        ("a {b: math.percentage(-0.4)}", "-40%"),
        ("a {b: math.percentage(0)}", "0%"),
        ("a {b: math.percentage($number: 1)}", "100%"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
    let err = compile("a {b: math.percentage(1%)}")
        .await
        .expect_err("percentage requires a unitless argument");
    assert_eq!(err.message, "$number: Expected 1% to have no units.");
}

/// 内建的结果是值，所以它也必须按 Sass 的拼法*拼写*：
/// `values/calculation/calc/constant::infinity/math/unsimplified/from_variable` 绑定
/// `$var: math.div(1, 0)` 并打印 `calc(infinity * (1% + 1px))` —— Rust 的 `inf` 读起来
/// 不像 CSS。`NaN` 本来就已一致，所以同簇的 `nan` fixture 无需特例就通过。
#[tokio::test]
async fn infinite_results_use_sass_spelling() {
    assert_eq!(
        css("$var: math.div(1, 0);\na {b: calc($var * (1% + 1px))}").await,
        "a {\n  b: calc(infinity * (1% + 1px));\n}\n"
    );
}

/// 两个方向上的边界。没有签名的被调者（`color.contrast`、`map.merge`）回落到源码文本 ——
/// 不编造数字、不编造错误 —— 单位是**复合**的结果也一样，因为复合单位只属于计算
/// （`Unit::is_plain`；语料从不单独打印一个，而唯一声明该拒绝的是一个被注释掉的
/// `libsass-closed-issues` 行）。
///
/// `color.hwb` 曾是本测试的例子，直到 `hwb` 有了签名；用一个现已实现的名字去证明回落，
/// 只会记录旧状态，所以例子换成 `color.contrast` —— 它仍未注册。`hwb` 现在产出什么由
/// `tests/hwb.rs` 逐 fixture 行钉住。
///
/// 现在*确实*会求值的是只含一次调用的运行：`math.div(…) * 1px` 是一个运算，而 fixture 正是
/// 这个形状 —— `core_functions/math/div.hrx::unit/denominator`（`math.div(6, 3px) * 1px` → `2`）、
/// `…::unit/incompatible`（`math.div(6in, 3s) * 1s` → `2in`）与
/// `…::unit/unknown`（`math.div(6c, 3d) * 1d` → `2c`）。
#[tokio::test]
async fn what_the_spine_does_not_know_stays_verbatim() {
    assert_eq!(
        css("a {b: color.contrast(red, blue)}").await,
        "a {\n  b: color.contrast(red, blue);\n}\n"
    );
    assert_eq!(
        css("a {b: math.div(6in, 3s)}").await,
        "a {\n  b: math.div(6in, 3s);\n}\n"
    );
    for (input, want) in [
        ("a {b: math.div(6, 3) * 1px}", "2px"),
        ("a {b: math.div(6, 3px) * 1px}", "2"),
        ("a {b: math.div(6in, 3s) * 1s}", "2in"),
        ("a {b: math.div(6c, 3d) * 1d}", "2c"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
