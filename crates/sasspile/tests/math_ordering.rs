//! `math.clamp` 与 `math.hypot`，钉在 `core_functions/math/{clamp,hypot}.hrx`。每个输入与
//! 期望都逐字抄自 fixture 自己的 `input.scss` / `output.css`（下方拒答取 `Error:` 首行）。
//!
//! 两者除单位律外还有一处共同：拒答**同时**点名两个参数，顺序即比较到达它们的顺序 ——
//! `min`/`max`（`math.max.hrx`）两个都不点名，所以这一族的措辞无法从已登记函数推断。

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

/// `math.clamp` 先从最大值向内折、再从最小值向外折，且**胜者保留自己的单位**：
/// `clamp(180deg, 0.75turn, 360deg)` → `0.75turn`、`clamp(180deg, 1turn, 360deg)` → `360deg`、
/// `clamp(180deg, 0.5turn, 360deg)` → `180deg`（`clamp.hrx::preserves_units/{number,max,min}`
/// —— 两处平局都属于边界，这正是折叠写成先 `<` 后 `<=` 的原因）。边界互相矛盾的行也展示
/// 同样次序：`clamp(1, 2, 0)` → `1` 与 `clamp(1, 2, 1)` → `1`（`…::{min_greater_than_max,min_equals_max}`）。
#[tokio::test]
async fn clamp_folds_to_the_bounds_and_keeps_the_writing() {
    for (input, want) in [
        ("a {b: math.clamp(1, 0, 2)}", "1"),
        ("a {b: math.clamp(0, 1, 2)}", "1"),
        ("a {b: math.clamp(0, 2, 1)}", "1"),
        ("a {b: math.clamp(1, 2, 1)}", "1"),
        ("a {b: math.clamp(1, 2, 0)}", "1"),
        ("a {b: math.clamp(180deg, 0.5turn, 360deg)}", "180deg"),
        ("a {b: math.clamp(180deg, 0.75turn, 360deg)}", "0.75turn"),
        ("a {b: math.clamp(180deg, 1turn, 360deg)}", "360deg"),
        ("a {b: math.clamp($min: 0, $number: 1, $max: 2)}", "1"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.hypot` 先按公共尺度平方，再以**首个**实参的单位返回根：
/// `hypot(3, 4, 5, 6, 7)` → `11.6189500386`，而用厘米、毫米、**四分之一毫米**、英寸与像素
/// 得同样数字（`hypot.hrx::units`）—— 该行把长度因子从 `math.rs` 移进 `crate::unit` 里那唯一
/// 一张表，因为 `q` 只住在那儿。任一无穷实参把整个根带走（`…::infinite/*` → `calc(infinity)`）。
#[tokio::test]
async fn hypot_scales_every_argument_and_answers_in_the_first_unit() {
    for (input, want) in [
        ("a {b: math.hypot(3, 4, 5, 6, 7)}", "11.6189500386"),
        (
            "a {b: math.hypot(3cm, 4mm * 10, 5q * 40, math.div(6in, 2.54), 7px * math.div(96, 2.54))}",
            "11.6189500386cm",
        ),
        ("a {b: math.hypot(math.div(1, 0), 1, 1)}", "calc(infinity)"),
        ("a {b: math.hypot(1, math.div(1, 0), 1)}", "calc(infinity)"),
        ("a {b: math.hypot(1, 1, math.div(1, 0))}", "calc(infinity)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 拒答同时点名两个参数 —— `clamp` 点名它到达的那一对（第一次比较是
/// `$number: 1px and $min: 1deg have incompatible units.`，只边界矛盾时是
/// `$max: 1px and $min: 1deg …`，见 `clamp.hrx::error/incompatible_units/*`），而 `hypot`
/// 从 1 开始数实参（`$numbers[3]: 2 and $numbers[1]: 0px have incompatible units (one has units and the other doesn't).`，
/// 见 `hypot.hrx::error/some_unitless/*`）。`hypot` 的类型消息不点名，与 `min`/`max` 相同。
#[tokio::test]
async fn a_refusal_names_the_pair_it_compared() {
    for (input, want) in [
        (
            "a {b: math.clamp(\"0\", 1, 2)}",
            "$min: \"0\" is not a number.",
        ),
        (
            "a {b: math.clamp(1, \"0\", 2)}",
            "$number: \"0\" is not a number.",
        ),
        (
            "a {b: math.clamp(1, 2, \"0\")}",
            "$max: \"0\" is not a number.",
        ),
        (
            "a {b: math.clamp(1deg, 1px, 1turn)}",
            "$number: 1px and $min: 1deg have incompatible units.",
        ),
        (
            "a {b: math.clamp(1deg, 1turn, 1px)}",
            "$max: 1px and $min: 1deg have incompatible units.",
        ),
        (
            "a {b: math.clamp(1turn, 1deg, 1px)}",
            "$max: 1px and $min: 1turn have incompatible units.",
        ),
        (
            "a {b: math.clamp(0, 1px, 2px)}",
            "$number: 1px and $min: 0 have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.clamp(0px, 1, 2)}",
            "$number: 1 and $min: 0px have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.clamp(0px, 1px, 2)}",
            "$max: 2 and $min: 0px have incompatible units (one has units and the other doesn't).",
        ),
        ("a {b: math.clamp()}", "Missing argument $min."),
        ("a {b: math.clamp(0)}", "Missing argument $number."),
        ("a {b: math.clamp(0, 0)}", "Missing argument $max."),
        (
            "a {b: math.clamp(0, 0, 0, 0)}",
            "Only 3 arguments allowed, but 4 were passed.",
        ),
        (
            "a {b: math.hypot(1deg, 1px, 1turn)}",
            "$numbers[2]: 1px and $numbers[1]: 1deg have incompatible units.",
        ),
        (
            "a {b: math.hypot(1turn, 1deg, 1px)}",
            "$numbers[3]: 1px and $numbers[1]: 1turn have incompatible units.",
        ),
        (
            "a {b: math.hypot(0px, 1, 2px)}",
            "$numbers[2]: 1 and $numbers[1]: 0px have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.hypot(0, 1px, 2)}",
            "$numbers[2]: 1px and $numbers[1]: 0 have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.hypot(\"0\", 1px, 1px)}",
            "\"0\" is not a number.",
        ),
        (
            "a {b: math.hypot()}",
            "At least one argument must be passed.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}
