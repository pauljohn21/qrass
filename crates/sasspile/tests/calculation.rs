//! 计算，tasks 1.4 的第一个切片（`crate::calculation`）。下面每条期望都是
//! `sass-spec/values/calculation/calc/no_operator.hrx` 的一节（带括号的形状取自 `calc/parens.hrx`），
//! 从 fixture 逐字引用，不是推断。
//!
//! 本文件后半是重要的一半：它钉住切片**拒绝**的那些形状，因为 1.4a 禁止猜它们 —— 同单位化简归
//! 1.4d，求值 `min`/`max`/`clamp` 与用户函数归更后的批次，而本簇曾有五条在律存在之前就已通过
//! （`ledger passed bool-units-746 "values/calculation/calc/no_operator"`），所以任何改写它们的
//! 东西都会是回归。

use sasspile::compile;

async fn value(input: &str) -> String {
    let css = compile(input).await.unwrap_or_else(|e| {
        panic!("calculation compiles: {input} failed with {}", e.message);
    });
    // 这里每个 fixture 都是一个块带一条 declaration；只回它的值，不管属性是什么（`b`、`c`、
    // `font` —— 最后一个是保护律守卫的 shorthand）。
    css.lines()
        .nth(1)
        .unwrap_or_default()
        .trim()
        .trim_end_matches(';')
        .split_once(':')
        .unwrap_or_else(|| panic!("unexpected output {css:?}"))
        .1
        .trim()
        .to_string()
}

/// `::case_insensitive`、`::extra_whitespace`、`syntax/extra_whitespace/number`、`::number/integer`
/// —— 单个值离开 calc，不管名字大小写或周围填充。
#[tokio::test]
async fn a_single_value_leaves_its_calc() {
    assert_eq!(value("a {b: CaLc(1px)}").await, "1px");
    assert_eq!(value("a {b: calc( 1px )}").await, "1px");
    assert_eq!(value("a {b: calc( 1 )}").await, "1");
    assert_eq!(value("a {b: calc(1px)}").await, "1px");
}

/// `syntax/extra_whitespace/parens` 与 `calc/parens::number` —— 剥掉冗余括号，然后值离开：
/// `calc( ( 1 ) )` → `1`、`calc((1px))` → `1px`。
#[tokio::test]
async fn redundant_parentheses_peel_to_the_value() {
    assert_eq!(value("a {b: calc( ( 1 ) )}").await, "1");
    assert_eq!(value("a {b: calc((1px))}").await, "1px");
}

/// `::number/*` 的数字形式：前导符号是字面量的一部分（`+1px` → `1px`、`-1px` → `-1px`），
/// `.2px` 按 Sass 的形式印（`0.2px`），而**指数记法也是数字**：`calc(1e2px)` → `100px`
/// （`values/calculation/calc/no_operator::number/exponent`）。这一行曾钉住逐字文本，因为
/// `number_lexeme` 把 `1e2px` 读成「单位是 `e2px` 的 1」；注释说修好后必须移到 `100px`，
/// 现在指数尾已是词素的一部分。
#[tokio::test]
async fn numbers_print_in_sass_form_after_collapsing() {
    assert_eq!(value("a {b: calc(+1px)}").await, "1px");
    assert_eq!(value("a {b: calc(-1px)}").await, "-1px");
    assert_eq!(value("a {b: calc(1e2px)}").await, "100px");
    assert_eq!(value("a {b: calc(.2px)}").await, "0.2px");
    assert_eq!(value("a {b: calc(1.2px)}").await, "1.2px");
}

/// `::calculation/calc/simplified` + `::calculation/{clamp,min,max}/preserved` —— 嵌套计算恰好只卸掉
/// **一重**壳，胜出的名字是内层那个。
#[tokio::test]
async fn a_nested_calculation_drops_one_shell() {
    assert_eq!(value("a {b: calc(calc(1px))}").await, "1px");
    assert_eq!(value("a {b: calc(calc(1px + 1%))}").await, "calc(1px + 1%)");
    assert_eq!(
        value("a {b: calc(clamp(1%, 2px, 3%))}").await,
        "clamp(1%, 2px, 3%)"
    );
    assert_eq!(value("a {b: calc(min(1%, 2px))}").await, "min(1%, 2px)");
    assert_eq!(value("a {b: calc(max(1%, 2px))}").await, "max(1%, 2px)");
    // `calc/parens::calculation` —— 剥括号与卸壳可复合。
    assert_eq!(
        value("a {b: calc((calc(1px + 1%)))}").await,
        "calc(1px + 1%)"
    );
}

/// `::variable/calculation` —— 绑定过的计算把它自己的身体带出外层壳，所以外壳不会叠加。
#[tokio::test]
async fn a_bound_variable_reads_through_the_environment() {
    assert_eq!(
        value("$a: calc(1px + 1%);\nb {c: calc($a)}").await,
        "calc(1px + 1%)"
    );
    assert_eq!(value("$a: foobar;\nb {c: calc($a)}").await, "calc(foobar)");
}

/// `$a: 1px; b {c: calc($a)}`（`::variable/number/simple_unit`）保持 `calc(1px)` —— 这不是本切片
/// 的律：绑定里**单独**的数字仍是 `Value::Raw`，因为 `exec::evaluated` 过滤掉 `Expr::Num`
/// （「裸数字还不是表达式」，P0′），所以没有可折叠的东西到达壳。撤掉那道过滤是另一项带自己 A/B
/// 的决定，它的效果就显现在这一行。
#[tokio::test]
async fn a_bound_plain_number_is_not_collapsible_yet() {
    assert_eq!(value("$a: 1px;\nb {c: calc($a)}").await, "calc(1px)");
}

/// 1.4d 所*授予*的：每一步都可计算时链条才会折叠。每条期望都逐字引自 fixture，不是推导 ——
/// `calc/operator/times::simplified`（`1px * 2` → `2px`）、`…/times::no_whitespace`
/// （`1px*2` → `2px`）、`…/minus::simplified`（`1px - 2px` → `-1px`），以及四条
/// `…/precedence::simplified/*` 行，它们定下结合性、`*` 比 `+` 绑得更紧。
#[tokio::test]
async fn chains_fold_while_every_step_is_computable() {
    for (input, want) in [
        ("a {b: calc(1px + 2px)}", "3px"),
        ("a {b: calc(1px - 2px)}", "-1px"),
        ("a {b: calc(1px * 2)}", "2px"),
        ("a {b: calc(1px*2)}", "2px"),
        (
            "a {b: calc(1px + 20px - 300px + 4000px - 50000px)}",
            "-46279px",
        ),
        ("a {b: calc(2 * 3 / 5 * 7 / 11)}", "0.7636363636"),
        ("a {b: calc(2 * 3 + 4 / 5 - 6)}", "0.8"),
        ("a {b: calc(2 * (3 + 4) / (5 - 6))}", "-14"),
        ("a {b: calc(1 / (2 * 3))}", "0.1666666667"),
        // 不兼容的步只停住自己（`operator/minus::preserved/number`、
        // `operator/units::percent/and_known`）。
        ("a {b: calc(1px + 1%)}", "calc(1px + 1%)"),
        ("a {b: calc(1px - 2%)}", "calc(1px - 2%)"),
        ("a {b: calc(1px + 2%)}", "calc(1px + 2%)"),
        // 语料库声明的左结合读法：`(1% + 1px) + 2px` 不折叠
        // （`calc/simplify.hrx` 自己的 README），右倾的读法同样不折叠。
        ("a {b: calc(1% + 1px + 2px)}", "calc(1% + 1px + 2px)"),
        // 复合单位只属于计算，所以壳留着
        // （`values/numbers/units/multiple::multiple_denominators`）。
        ("a {b: calc(1 / 1px / 1rad)}", "calc(1 / 1px / 1rad)"),
        // 而普通声明值里的斜杠仍是 CSS 简写需要的那个分隔符。
        ("a {b: 1/2}", "1/2"),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// 括号是从优先级重新推导出来的，绝不是复制来的：`operator/var` 与
/// `operator/precedence::preserved/*` 一起说明了一个分组何时存活。律的两侧是同一个表达式，
/// 一侧写成显式括号、一侧写成嵌套 `calc( … )` —— 两边打印完全相同，所以嵌套形式在此不是特例。
#[tokio::test]
async fn parentheses_follow_precedence_not_source() {
    for (input, want) in [
        // 代数允许的地方被丢掉。
        ("a {b: calc((1 + var(--c)))}", "calc(1 + var(--c))"),
        ("a {b: calc(1px + (2% - 3em))}", "calc(1px + 2% - 3em)"),
        (
            "a {b: calc(1px * (2 / var(--c)))}",
            "calc(1px * 2 / var(--c))",
        ),
        ("a {b: calc(1px + calc(2% - 3em))}", "calc(1px + 2% - 3em)"),
        (
            "a {b: calc(1px * calc(2 / var(--c)))}",
            "calc(1px * 2 / var(--c))",
        ),
        // 丢掉会改变值的地方被保留。
        (
            "a {b: calc(1px / (2 * var(--c)))}",
            "calc(1px / (2 * var(--c)))",
        ),
        (
            "a {b: calc(1px / calc(2 * var(--c)))}",
            "calc(1px / (2 * var(--c)))",
        ),
        (
            "a {b: calc(1px * (2 + var(--c)))}",
            "calc(1px * (2 + var(--c)))",
        ),
        (
            "a {b: calc(1px - (2% + var(--c)))}",
            "calc(1px - (2% + var(--c)))",
        ),
        (
            "a {b: calc(1px - (2% - var(--c)))}",
            "calc(1px - (2% - var(--c)))",
        ),
        // 包着不可建模操作数的分组，无论优先级怎么说都保留括号。
        ("a {b: calc(1 + (var(--c)))}", "calc(1 + (var(--c)))"),
        ("a {b: calc(1 + calc(var(--c)))}", "calc(1 + (var(--c)))"),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// 保持**不可读**的那部分，所以 1.4a 已正确印出的东西一个都不动：没有在操作数与操作符之间交替的
/// 一段会保留源码文本。`values/calculation/calc/space.hrx::var/before`
/// （`calc(var(--c) 1)` → `calc(var(--c) 1)`）是语料里声明「相邻操作数是空格分隔的列表而非乘积」的
/// fixture。
#[tokio::test]
async fn text_this_cannot_model_stays_as_written() {
    assert_eq!(value("a {b: calc(foobar)}").await, "calc(foobar)");
    assert_eq!(value("a {b: calc(var(--c))}").await, "calc(var(--c))");
    assert_eq!(value("a {b: calc(c())}").await, "calc(c())");
    assert_eq!(value("a {b: calc(var(--c) 1)}").await, "calc(var(--c) 1)");
    assert_eq!(value("a {b: calc(1 var(--c))}").await, "calc(1 var(--c))");
    // 紧贴数字的前导符号是字面量的一部分（`no_operator::number/leading_plus`）。
    assert_eq!(value("a {b: calc(+1px)}").await, "1px");
}

/// 本簇在任何计算规则存在之前就已经通过的五例 —— 语料要求它们的外壳，所以任何把「它们化简掉」的规则
/// 都会是无声回归。每条期望都是 fixture 自己的 `output.css`。
#[tokio::test]
async fn what_already_passed_still_passes() {
    // ::function/css —— 未知的被调者不是值的证据。
    assert_eq!(value("a {b: calc(c())}").await, "calc(c())");
    // ::var/bare 与 calc/parens::var/direct —— CSS `var()` 保留其壳，内边距归一化。
    assert_eq!(value("a {b: calc(var(--c))}").await, "calc(var(--c))");
    assert_eq!(value("a {b: calc((var(--c)))}").await, "calc((var(--c)))");
    // ::variable/unquoted_string —— 绑定过的标识符是内容，不是会离开壳的值。
    assert_eq!(value("$a: foobar;\nb {c: calc($a)}").await, "calc(foobar)");
    // ::interpolation/number —— calc 里的插值是一个非引号字符串（fixture 自己的注释），
    // 所以它可以被求值，但永不离开壳。
    assert_eq!(value("a {b: calc(#{1px + 2px})}").await, "calc(3px)");
    // ::interpolation/parens —— 并且它周围的括号存活。
    assert_eq!(value("a {b: calc((#{1px + 2px}))}").await, "calc((3px))");
    // calc/parens::identifier —— 冗余括号里单独的标识符，两重都保留。
    assert_eq!(value("a {b: calc((d))}").await, "calc((d))");
}

/// 在计算之外，这里没有任何东西改变：没有注册签名的 `min`/`max`/`clamp` 保留文本
/// （1.4a 拒绝求值它们），厂商前缀的 `-a-calc(` **不是**计算
/// （`css/functions/special/prefixed/lowercase::calc/script_like` 保留 `-a-calc($c)`），
/// 而保护律为之存在的 CSS 简写也不被动。
#[tokio::test]
async fn outside_a_calculation_nothing_is_newly_read() {
    assert_eq!(value("a {b: min(1px, 2px)}").await, "min(1px, 2px)");
    assert_eq!(value("a {b: -a-calc(1px)}").await, "-a-calc(1px)");
    assert_eq!(
        value("a {font: 12px/1.5 sans-serif}").await,
        "12px/1.5 sans-serif"
    );
    assert_eq!(value("a {b: 1/2}").await, "1/2");
    // URL 的斜杠是字面文本：`release_slashes` 只会读调用参数，
    // 而声明值仍然走保护律。
    assert_eq!(value("a {b: url(http://x/y)}").await, "url(http://x/y)");
}

/// 反三角函数名同样是计算壳：语料自己的 `simplification` 行保留被调者的拼写，
/// 折叠实参里可计算的部分，其余不可读的部分原样不动
/// （`values/calculation/{acos,asin,atan,atan2}.hrx::simplification`）。`atan2` 一次性展示律的两半
/// —— 第二个实参折叠成 `-1.75px`，而第一个把 `var(--c)` 留在折叠好的
/// `2px` 旁边。这里*不*声张的：同批文件的 `one`/`zero`/`overridden`/`case_insensitive` 行
/// 要求函数的**值**（`0deg`、`1`），那需要内建注册加用户函数优先级，
/// 仍是单独一批（tasks.md 1.7b）。
#[tokio::test]
async fn inverse_trig_shells_fold_what_they_can() {
    for (input, want) in [
        (
            "a { b: acos(3px - 1px + var(--c)); }",
            "acos(2px + var(--c))",
        ),
        (
            "a { b: asin(3px - 1px + var(--c)); }",
            "asin(2px + var(--c))",
        ),
        (
            "a { b: atan(3px - 1px + var(--c)); }",
            "atan(2px + var(--c))",
        ),
        (
            "a { b: atan2(3px - 1px + var(--c), -7px / 4em * 1em); }",
            "atan2(2px + var(--c), -1.75px)",
        ),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// 同一条律，每个文件一对字面量，覆盖其余九个 `simplification` 行点名要求的数字函数名：
/// `sqrt`/`exp`/`log`/`pow`/`sin`/`cos`/`tan`/`hypot`/`rem`
/// （`values/calculation/<name>.hrx::simplification`，本次会话读过）。`hypot` 的第二个实参
/// `-7px + 4em` **不**折叠 —— `em` 是相对上下文的，所以 `math.compatible(1px, 4em)` 为假 ——
/// 这与 `log` 里未动的 `var(--e)` 理由不同：那个实参作为值根本不可读。
/// `rem` 列入仅以 `a_multi_argument_shell_claims_no_value` 成立为前提：
/// 实参多于一个时壳声明无值，所以 `rem` 自己的行保留文本，而不是编造一个。
#[tokio::test]
async fn number_shells_fold_what_their_own_rows_ask() {
    for (input, want) in [
        (
            "a { b: sqrt(1px + 2px - var(--c)) }",
            "sqrt(3px - var(--c))",
        ),
        ("a { b: exp(1px + 2px - var(--c)) }", "exp(3px - var(--c))"),
        (
            "a {b: log(3px - 1px + var(--c), var(--e));}",
            "log(2px + var(--c), var(--e))",
        ),
        (
            "a {b: pow(3px - 1px + var(--c), 4px + 10px)}",
            "pow(2px + var(--c), 14px)",
        ),
        ("a { b: sin(3px - 1px + var(--c)); }", "sin(2px + var(--c))"),
        ("a { b: cos(3px - 1px + var(--c)); }", "cos(2px + var(--c))"),
        ("a { b: tan(3px - 1px + var(--c)); }", "tan(2px + var(--c))"),
        (
            "a { b: rem(3px - 1px + var(--c), -7px / 4em * 1em); }",
            "rem(2px + var(--c), -1.75px)",
        ),
        (
            "a { b: hypot(1px + 2px - var(--c), -7px + 4em) }",
            "hypot(3px - var(--c), -7px + 4em)",
        ),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// 有**多于一个实参的壳声明无值**：`min(10, 20)` 不是 `10`。`calculation::folded`
/// 用 `find_map` 走查各部分，所以多实参壳曾以第一个实参的身份被交出，随即参与算术
/// —— `math.div(1, min(10, 20))` 印出 `0.1`，`max(10, 20) * 1` 印出 `10`。
/// 我在语料里搜过把多实参计算放进算术的行（`math.div` 右侧一个壳、`/ calc(`），一条也没有，
/// 所以没有任何东西钉住那个编造的数字；本项目在无法求值时已经使用的形状是照写的文本
/// （1.4a 的不猜律），这两行断言的正是它。**单**实参路径也不是我的字面量声张的：
/// `calc/operator/units::multiplication` 只钉住 `calc(2px * 3px)` 怎么印，那边的守卫是 hrx 记录。
#[tokio::test]
async fn a_multi_argument_shell_claims_no_value() {
    assert_eq!(
        value("a {b: math.div(1, min(10, 20))}").await,
        "math.div(1, min(10, 20))"
    );
    assert_eq!(value("a {b: max(10, 20) * 1}").await, "max(10, 20) * 1");
}

/// calc 里未绑定的 `$x` 不得变成致命错误 —— fixture 族
/// `::variable/not_parsed_as_interpolation/*` 与 `css/functions/special_variable` 都保留文本，
/// 这也是 `expr::special_call` 记录的第二道守卫。
#[tokio::test]
async fn an_unbound_variable_inside_a_calc_is_not_fatal() {
    assert_eq!(value("a {b: calc($undefined)}").await, "calc($undefined)");
    assert_eq!(
        value("a {b: calc($undefined + 1px)}").await,
        "calc($undefined + 1px)"
    );
}

/// CSS 的计算**常量**，逐条逐字取自 `values/calculation/calc/constant.hrx`：
/// `pi/alone`（`calc(pi)` → `3.1415926536`）、`pi/case_insensitive`（`calc(pI)` 同）、
/// `e/math/simplified`（`calc(e * 2)` → `5.4365636569`）、
/// `pi/math/unsimplified`（`calc(pi * (1% + 1px))` → `calc(3.1415926536 * (1% + 1px))`）、
/// `infinity/case_insensitive`、`minus_infinity/case_insensitive`
/// （`calc(-iNfInItY)` → `calc(-infinity)`）与 `nan/case_insensitive`（`calc(nan)` → `calc(NaN)`）。
///
/// 最后三条还托住本批**选择**的边界，它在常量簇别处都不可见：非有限量级留在其壳内，
/// 于是 `color(srgb calc(infinity) 0 0)` 原样返回
/// （`core_functions/color/color/degenerate::arg1/positive_infinity`），
/// 而不是变成带裸 `infinity` 通道的颜色。这个选择正反两面都被计量，代价写在代码所在处
/// —— [`crate::calculation::collapsible`] 的文档点名了要求相反的 fixture 行
/// （`…::arg1/nan` → `color(srgb 0 0 0)`、
/// `hsl/four_args/out_of_gamut::hue/degenerate/positive_infinity` → `hsl(0, 100%, 50%)`、
/// `…::saturation/degenerate/nan` → `hsl(0, 0%, 50%)`），以及拒绝后留红的 23 条 clamp 形状行。
/// 本文件不复述那些数字。
#[tokio::test]
async fn calculation_constants_are_read_inside_the_shell() {
    assert_eq!(value("a {b: calc(pi)}").await, "3.1415926536");
    assert_eq!(value("a {b: calc(pI)}").await, "3.1415926536");
    assert_eq!(value("a {b: calc(e * 2)}").await, "5.4365636569");
    assert_eq!(
        value("a {b: calc(pi * (1% + 1px))}").await,
        "calc(3.1415926536 * (1% + 1px))"
    );
    assert_eq!(value("a {b: calc(InFiNiTy)}").await, "calc(infinity)");
    assert_eq!(value("a {b: calc(-iNfInItY)}").await, "calc(-infinity)");
    assert_eq!(value("a {b: calc(nan)}").await, "calc(NaN)");
    // 颜色构造函数从「壳原地不动」继承来的拒绝。
    assert_eq!(
        value("a {b: color(srgb calc(infinity) 0 0)}").await,
        "color(srgb calc(infinity) 0 0)"
    );
}
