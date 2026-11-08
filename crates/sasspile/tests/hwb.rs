//! `hwb` / `color.hwb`，钉在 `core_functions/color/hwb/**` 与 `channel/hwb.hrx` 上。
//! 每个输入与每条期望都是该 fixture 自身的 `input.scss` / `output.css` 行。
//!
//! 这里有两条不同于 `rgb`/`hsl` 任何行为的定律，且都来自行而非推理：
//! * `hwb` 颜色**打印为 hsl**（`hwb(0deg, 30%, 40%)` → `hsl(0, 33.3333333333%, 45%)`），
//!   却保持白度与黑度可读（`color.channel(hwb(10deg 70% 50%), "whiteness")` → `58.3333333333%`，
//!   即 `70/120`——溢出在**创建时**归一化）；
//! * 字节命名到关键字的整字节 `hwb` 颜色打印**该关键字**（`red`），而 `rgb` 调用不这样做
//!   （`rgb(0, 0, 0)` 打印自身）；语料库未说明原因，所以此处不做任何推广。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `hwb/three_args/units.hrx`：hue 遵循 `hsl` 的角度定律，而**非**角度的单位
/// 被视为普通数字而非被拒绝（`hwb/three_args/named.hrx` 绑定标签）。
#[tokio::test]
async fn hwb_prints_as_hsl_and_folds_its_hue() {
    for (input, want) in [
        ("a {b: hwb(0deg, 30%, 40%)}", "hsl(0, 33.3333333333%, 45%)"),
        (
            "a {b: hwb(1rad, 30%, 40%)}",
            "hsl(57.2957795131, 33.3333333333%, 45%)",
        ),
        ("a {b: hwb(1in, 30%, 40%)}", "hsl(1, 33.3333333333%, 45%)"),
        (
            "a {b: color.hwb($hue: 0, $whiteness: 30%, $blackness: 40%)}",
            "hsl(0, 33.3333333333%, 45%)",
        ),
        // 命名空间拼写，以及 alpha：为 1 时返回三参数形式，为 0 时不返回
        // （`hwb/global.hrx::alpha/in_gamut/{opaque,transparent}`）。
        (
            "a {b: color.hwb(0, 30%, 40%)}",
            "hsl(0, 33.3333333333%, 45%)",
        ),
        (
            "a {b: hwb(180 30% 40% / 1)}",
            "hsl(180, 33.3333333333%, 45%)",
        ),
        (
            "a {b: hwb(180 30% 40% / 0)}",
            "hsla(180, 33.3333333333%, 45%, 0)",
        ),
        (
            "a {b: hwb(180 30% 40% / 0.5)}",
            "hsla(180, 33.3333333333%, 45%, 0.5)",
        ),
        (
            "a {b: hwb($channels: 180 30% 40% / 0.4)}",
            "hsla(180, 33.3333333333%, 45%, 0.4)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 白度与黑度不被钳制，但溢出颜色的组合**按其和归一化**——`hwb(0 101% 40%)` 给出
/// `1.01 / 1.41`，负白度则被留下把饱和度推过 100%
/// （`hwb/one_arg.hrx::{whiteness/above_max,whiteness/below_min}`，与逗号拼写的
/// `hwb/four_args` 行数字相同）。
#[tokio::test]
async fn overflowing_whiteness_and_blackness_normalise() {
    for (input, want) in [
        ("a {b: hwb(0 101% 40%)}", "hsl(0, 0%, 71.6312056738%)"),
        ("a {b: hwb(0 -1% 40%)}", "hsl(0, 103.3898305085%, 29.5%)"),
        (
            "a {b: hwb(0, 101%, 40%, 0.5)}",
            "hsla(0, 0%, 71.6312056738%, 0.5)",
        ),
        (
            "a {b: hwb(0, -1%, 40%, 0.5)}",
            "hsla(0, 103.3898305085%, 29.5%, 0.5)",
        ),
        // 当通道被读回时归一化可见（`channel/hwb.hrx::local/normalized`）。
        (
            "a {b: color.channel(hwb(10deg 70% 50%), \"whiteness\")}",
            "58.3333333333%",
        ),
        (
            "a {b: color.channel(hwb(10deg 70% 50%), \"blackness\")}",
            "41.6666666667%",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hwb/four_args::alpha/percent/max`：`color.hwb(0, 0%, 0%, 100%)` 是纯红并打印
/// **`red`**——而 `rgb(0, 0, 0)` 打印自身（`rgb/three_args/unitless::numbers`）。非关键字的
/// `hwb` 颜色保持 hsl 形式，而经轴操作移动过的 `hwb` 颜色在结果为灰色时失去 hue
/// （`grayscale.hrx::legacy/powerless/different`），这就是该路径返回 `hsl` 颜色的原因。
#[tokio::test]
async fn a_hwb_colour_prints_its_keyword_and_loses_hue_when_greyed() {
    for (input, want) in [
        ("a {b: color.hwb(0, 0%, 0%, 100%)}", "red"),
        ("a {b: hwb(0deg, 30%, 40%)}", "hsl(0, 33.3333333333%, 45%)"),
        (
            "a {b: color.grayscale(hwb(120deg 10% 20%))}",
            "hsl(0, 0%, 45%)",
        ),
        // `hwb` 颜色回答自身的通道，`hue` 是存储的角度。
        ("a {b: color.channel(hwb(10 30% 40%), \"hue\")}", "10deg"),
        (
            "a {b: color.channel(pink, \"whiteness\", $space: hwb)}",
            "75.2941176471%",
        ),
        (
            "a {b: color.channel(pink, \"blackness\", $space: hwb)}",
            "0%",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `var(` 实参使整个调用不可读，每个这样的行保持其文本——包括内部空格
/// （`hwb/one_arg.hrx::{hue/var,whiteness/var,blackness/var/no_alpha}`）。
#[tokio::test]
async fn an_unreadable_hwb_argument_keeps_the_run_verbatim() {
    for call in [
        "hwb(var(--c) 30% 40%)",
        "hwb(0 var(--c) 40%)",
        "hwb(0 30% var(--c))",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unreadable hwb call must not be answered: {call}"
        );
    }
}
