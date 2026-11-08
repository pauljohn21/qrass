//! `color.channel`，在 `core_functions/color/channel/{rgb,hsl,srgb,named}.hrx` 上逐行钉住。下面每条输入与
//! 期望都是该夹具自己的 `input.scss` / `output.css` 行，照抄而非塑形：读法依赖空间（`pink` 的 `"red"`
//! 在 `rgb` 是 `255`、在 `srgb` 是 `1`），所以一个貌似合理的数字不是证据。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// 颜色**自身**的空间，即不带 `$space:` 的调用的含义（`channel/rgb.hrx::local/*` 读 hex，
/// `…::foreign/*` 读关键字创建的颜色）。
#[tokio::test]
async fn channel_reads_the_colour_own_space() {
    for (input, want) in [
        ("a {b: color.channel(#abcdef, \"red\")}", "171"),
        ("a {b: color.channel(#abcdef, \"green\")}", "205"),
        ("a {b: color.channel(#abcdef, \"blue\")}", "239"),
        ("a {b: color.channel(pink, \"red\")}", "255"),
        ("a {b: color.channel(pink, \"green\")}", "192"),
        ("a {b: color.channel(pink, \"blue\")}", "203"),
        // 一个 hsl 颜色自身的空间是 hsl：hue 带 `deg`，其余轴是 `%`（`channel/hsl.hrx`）。
        ("a {b: color.channel(hsl(10 40% 60%), \"hue\")}", "10deg"),
        (
            "a {b: color.channel(hsl(10deg 40% 60%), \"saturation\")}",
            "40%",
        ),
        (
            "a {b: color.channel(hsl(10deg 40% 60%), \"lightness\")}",
            "60%",
        ),
        // hue 在颜色构建时已折叠进 `0–360`（`hsl(540 …)` → `180deg`）。
        ("a {b: color.channel(hsl(540 40% 60%), \"hue\")}", "180deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **跨空间**读只因调用用 `$space:` 明示而发生（`channel/hsl.hrx::foreign/*`、
/// `channel/srgb.hrx::foreign/*`）；这些数字是同一颜色在另一把尺上的度量，所以 `srgb` 把字节
/// 除以 255 再打印裸数。
#[tokio::test]
async fn a_space_label_selects_the_axes() {
    for (input, want) in [
        (
            "a {b: color.channel(pink, \"hue\", $space: hsl)}",
            "349.5238095238deg",
        ),
        (
            "a {b: color.channel(pink, \"saturation\", $space: hsl)}",
            "100%",
        ),
        (
            "a {b: color.channel(pink, \"lightness\", $space: hsl)}",
            "87.6470588235%",
        ),
        ("a {b: color.channel(pink, \"red\", $space: srgb)}", "1"),
        (
            "a {b: color.channel(pink, \"green\", $space: srgb)}",
            "0.7529411765",
        ),
        (
            "a {b: color.channel(pink, \"blue\", $space: srgb)}",
            "0.7960784314",
        ),
        // 所有实参都带标签（`channel/named.hrx`）。
        (
            "a {b: color.channel($color: pink, $channel: \"hue\", $space: hsl)}",
            "349.5238095238deg",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 本切片**不得**回答的东西。每条调用都保留其书写形式，因为该空间或该颜色需要此处没有实现的换算 ——
/// 而一个从错误轴上读出的数字会是一个通过计数看不见的错误值：
/// * 以 `color()` 语法书写的 `srgb` 空间颜色（`channel/srgb.hrx::local/red`）—— `Color::of` 正因
///   这个原因拒收括号文本；
/// * **`hsl`** 颜色的 whiteness/blackness，它两者都不存储；
/// * 未知通道名。
///
/// rgb 族颜色的 hwb 读从前在此列表上；hwb 实现后它们可答了，现在在 `tests/hwb.rs` 里按夹具值钉住，
/// 所以把它们列在这里会把一条已被取代的限制记成仍是律。
#[tokio::test]
async fn an_unreachable_space_declines_rather_than_guesses() {
    for call in [
        "color.channel(color(srgb 0.2 0.5 0.8), \"red\")",
        "color.channel(hsl(10deg 40% 60%), \"whiteness\", $space: hwb)",
        "color.channel(#abcdef, \"luminance\")",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unreadable channel request must not be answered: {call}"
        );
    }
}
