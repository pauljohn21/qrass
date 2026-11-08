//! `color.adjust`，逐行钉在 `core_functions/color/adjust/{rgb,hsl,hwb,units,no_channels}.hrx` 与命名空间
//! 孪件 `core_functions/modules/color/adjust.hrx` 上。下面每条输入与期望都抄自 fixture 自己的
//! `input.scss` / `output.css`，含 case 名。
//!
//! 本文件的主旨是：**`adjust` 移动通道而不是设置它**，而它所撞的界不止一条规则：字节停在
//! `0–255`，hsl 轴完全不取 legacy 夹紧（这正是本函数与 `saturate` 的分界），hwb 那一对在加法之后
//! 归一化，而一个什么通道都不点名的调用是 no-op。每一条都有自己的行，故分开写，而不是折成调用者
//! 得记的一条夹紧。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// 字节通道按参数移动，且不能越过两端：`red/above_max` 是 `color.adjust(#abcdef, $red: 200)` →
/// `#ffcdef`（171 + 200 = 371，夹到 `ff`，另两个不动），`red/below_min` 是同调用取 `-200` →
/// `#00cdef`。`all` 与 `named` 是对三个通道的一次调用——一次作位置参数、一次把颜色标名——两者
/// 都得 `#0c1830`，即从 black 起 `12/24/48` 的偏移；`alpha_arg` 在其上加 `$alpha: -0.3` 并印
/// `rgba(12, 24, 48, 0.7)`。
#[tokio::test]
async fn bytes_move_and_stop_at_the_ends() {
    for (input, want) in [
        ("a {b: color.adjust(black, $red: 255)}", "red"),
        ("a {b: color.adjust(red, $red: -100)}", "#9b0000"),
        ("a {b: color.adjust(#abcdef, $red: 200)}", "#ffcdef"),
        ("a {b: color.adjust(#abcdef, $red: -200)}", "#00cdef"),
        ("a {b: color.adjust(#abcdef, $green: 200)}", "#abffef"),
        ("a {b: color.adjust(#fedcba, $blue: -200)}", "#fedc00"),
        (
            "a {b: color.adjust(black, $red: 12, $green: 24, $blue: 48)}",
            "#0c1830",
        ),
        (
            "a {b: color.adjust($color: black, $red: 12, $green: 24, $blue: 48)}",
            "#0c1830",
        ),
        (
            "a {b: color.adjust(black, $red: 12, $green: 24, $blue: 48, $alpha: -0.3)}",
            "rgba(12, 24, 48, 0.7)",
        ),
        // `@use "sass:color"` 拼法到达同一个函数（`modules/color/adjust.hrx::`）。
        ("a {b: color.adjust(lime, $green: -255)}", "black"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **hsl 轴不夹紧**，这正是 `color.adjust` 与 legacy `saturate` 的分界：
/// `color.adjust(plum, $saturation: 53%)` 保留 100.2868217054%（plum 自己的彩度加参数）并印 hsl
/// 三元组，而 `saturate(plum, 53%)` 印 `#ff7eff`——两者都是行，故两者都进断言
/// （[`Color::transform_axes`](sasspile::color::Color) 仍是 legacy 那一个）。
#[tokio::test]
async fn the_hsl_axes_keep_what_they_add_up_to() {
    for (input, want) in [
        (
            "a {b: color.adjust(plum, $saturation: 53%)}",
            "hsl(300, 100.2868217054%, 74.7058823529%)",
        ),
        (
            "a {b: color.adjust(plum, $saturation: 100%)}",
            "hsl(300, 147.2868217054%, 74.7058823529%)",
        ),
        (
            "a {b: color.adjust(plum, $saturation: 200%)}",
            "hsl(300, 247.2868217054%, 74.7058823529%)",
        ),
        (
            "a {b: color.adjust(red, $lightness: -100%)}",
            "hsl(0, 100%, -50%)",
        ),
        (
            "a {b: color.adjust(red, $lightness: 100%)}",
            "hsl(0, 100%, 150%)",
        ),
        (
            "a {b: color.adjust(red, $lightness: 200%)}",
            "hsl(0, 100%, 250%)",
        ),
        (
            "a {b: color.adjust(plum, $lightness: -100%)}",
            "hsl(300, 47.2868217054%, -25.2941176471%)",
        ),
        (
            "a {b: color.adjust(plum, $lightness: -200%)}",
            "hsl(300, 47.2868217054%, -125.2941176471%)",
        ),
        // legacy 入口，钉在上方那行旁边，让这个分界不会被悄悄并掉
        // （`saturate.hrx::two_args/max_remaining`）。
        ("a {b: saturate(plum, 53%)}", "#ff7eff"),
        // 一次调用同时点名三个轴（`adjust/hsl.hrx::all`）：black 被移动 12°、24%、48% 后仍在
        // 色域内，故印那个三元组蕴含的字节。
        (
            "a {b: color.adjust(black, $hue: 12, $saturation: 24%, $lightness: 48%)}",
            "rgb(59.52%, 41.088%, 36.48%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `adjust` *确实*保留的一个界：被压到零以下的彩度变成零所是的那种灰。
/// `color.adjust(plum, $saturation: -100%)` 与 `color.adjust(plum, $saturation: -48%)`（47.2868217054% − 48
/// 为负）都印 `rgb(74.7058823529%, …)`——plum 的明度，未变，成一个灰
/// （`adjust/hsl.hrx::saturation/{min,min_remaining}`）。明度没有这道夹紧，
/// [`the_hsl_axes_keep_what_they_add_up_to`] 用 `-50%` 演示了这点。
#[tokio::test]
async fn only_the_saturation_lower_bound_clamps() {
    let grey = "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)";
    for (input, want) in [
        ("a {b: color.adjust(plum, $saturation: -100%)}", grey),
        ("a {b: color.adjust(plum, $saturation: -48%)}", grey),
        ("a {b: color.adjust(plum, $saturation: -200%)}", grey),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 色相回绕，即两个入口共享的那一个界：`color.adjust(red, $hue: 540)` 是 `aqua`，
/// `color.adjust(red, $hue: -180)` 也是 `aqua`（`adjust/hsl.hrx::hue/{arg_above_max,negative}`），
/// 而一次留在圈内的移动则印它蕴含的字节（`hue/max` 是 `red` 移动 359°）。
#[tokio::test]
async fn the_hue_wraps_and_otherwise_prints_bytes() {
    for (input, want) in [
        ("a {b: color.adjust(red, $hue: 540)}", "aqua"),
        ("a {b: color.adjust(red, $hue: -180)}", "aqua"),
        (
            "a {b: color.adjust(red, $hue: 359)}",
            "rgb(100%, 0%, 1.6666666667%)",
        ),
        ("a {b: color.adjust(blue, $hue: 0)}", "blue"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// hwb **先加后归一化**——与夹紧相反的次序。
/// `color.adjust(#cc6666, $whiteness: 100%)` 把白度 60 → 160 对着黑度 40，然后这一对按自身之和
/// 缩放：`160/200` 与 `40/200`，即 `rgb(87.5%, …)` 那个灰（`adjust/hwb.hrx::whiteness/max`）。
/// 调用没点名的通道保留原值而不是被移动两次：`color.adjust(#993333, $blackness: 40%)` 是
/// `#333333`（`…::blackness/high`），而一对落到色域外时印 hsl 三元组，如
/// `$whiteness: -100%` → `hsl(0, 700%, 10%)` 所示（`…::whiteness/min`）。
#[tokio::test]
async fn hwb_moves_then_normalises_the_pair() {
    for (input, want) in [
        (
            "a {b: color.adjust(#cc6666, $whiteness: 100%)}",
            "rgb(87.5%, 87.5%, 87.5%)",
        ),
        (
            "a {b: color.adjust(#cc6666, $whiteness: 60%)}",
            "rgb(83.3333333333%, 83.3333333333%, 83.3333333333%)",
        ),
        ("a {b: color.adjust(#cc6666, $whiteness: 0%)}", "#cc6666"),
        (
            "a {b: color.adjust(#cc6666, $whiteness: -100%)}",
            "hsl(0, 700%, 10%)",
        ),
        ("a {b: color.adjust(#993333, $blackness: 40%)}", "#333333"),
        // `$hue:` 与这一对并列就是同一个 hwb 写入（`adjust/hwb.hrx::all`），结果落在整字节上，
        // 故印 hex。
        (
            "a {b: color.adjust(red, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
            "#339966",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 不点名任何通道是 no-op，而单写 `$alpha:` 仍会移动 alpha——夹在 `0–1`，而上面的彩度与明度
/// 都不夹：`color.adjust(red)` 与 `color.adjust($color: red)` 都是 `red`，`rgba(red, 0.5)` 加
/// `0.5`、`1` 或 `2` 都再变不透明（`red`，因颜色由关键字造出而按关键字印），减 `1` 或 `2` 则
/// `rgba(255, 0, 0, 0)`（`adjust/no_channels.hrx::{positional,named}` 及其 `alpha/*` 组）。量级按
/// 其携带的任何单位读取——`-0.3`、`-0.3%`、`-0.3px` 都把 `red` 带到 `rgba(255, 0, 0, 0.7)`
/// （`adjust/units.hrx::alpha/*`；两种弃用拼法带的警告 harness 不比较）。
#[tokio::test]
async fn naming_no_channel_is_a_no_op_and_the_alpha_still_moves() {
    for (input, want) in [
        ("a {b: color.adjust(red)}", "red"),
        ("a {b: color.adjust($color: red)}", "red"),
        ("a {b: color.adjust(rgba(red, 0.5), $alpha: 0.5)}", "red"),
        ("a {b: color.adjust(rgba(red, 0.5), $alpha: 1)}", "red"),
        ("a {b: color.adjust(rgba(red, 0.5), $alpha: 2)}", "red"),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: 0)}",
            "rgba(255, 0, 0, 0.5)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: 0.14)}",
            "rgba(255, 0, 0, 0.64)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: -0.14)}",
            "rgba(255, 0, 0, 0.36)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: -0.5)}",
            "rgba(255, 0, 0, 0)",
        ),
        (
            "a {b: color.adjust(rgba(red, 0.5), $alpha: -1)}",
            "rgba(255, 0, 0, 0)",
        ),
        (
            "a {b: color.adjust(red, $alpha: -0.3)}",
            "rgba(255, 0, 0, 0.7)",
        ),
        (
            "a {b: color.adjust(red, $alpha: -0.3%)}",
            "rgba(255, 0, 0, 0.7)",
        ),
        (
            "a {b: color.adjust(red, $alpha: -0.3px)}",
            "rgba(255, 0, 0, 0.7)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

// 本文件**不**认领的行。`adjust/hwb.hrx::alpha_input` 与 `…::alpha_arg` 这一对要
// `rgba(20%, 60%, 40%, …)`，而本编译器印 `rgba(51, 153, 102, …)`——同样两行也是
// `tests/color_change.rs` 为 `color.change` 留红的行，理由同样未解释（第三行 `rgb(#123, 0.5)` 与
// 两者都不一致），所以保持红，而不是从邻居的律里拟合出来。`adjust/space.hrx` 要的是一次本编译器
// 还没有的换算，而 `adjust/{lab,lch,oklab,oklch,srgb,display_p3,rec2020,prophoto_rgb,a98_rgb,
// srgb_linear,xyz_d50,xyz_d65}.hrx` 各文件要的是同一批现代空间；`adjust/global.hrx::legacy` 要
// `adjust-color()` 拼法，`adjust/no_channels.hrx` 与 `adjust/hsl.hrx::all` 已在上方覆盖，而
// `adjust/error/**` 各行要的是消息文本——那是 tasks.md 1.5 的 P7 半边，不是算术。
