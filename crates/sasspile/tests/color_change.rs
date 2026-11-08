//! `color.change` 落在 **`rgb` 通道**上，逐行钉在 `core_functions/color/change/rgb.hrx` 与命名空间
//! 孪件 `core_functions/modules/color/change.hrx` 上。下面每条输入与期望都抄自 fixture 自己的
//! `input.scss` / `output.css`——含 case 名，而那不是我可以发明的语法（`change/rgb.hrx` 恰好是
//! `red|green|blue/{max,min,low,high,out_of_range,none}` 加 `alpha_arg`；没有「混合通道」行可断言）。
//!
//! 本文件的主旨是**标签**：`color.change(black, $red: 500)` 与 `color.change(black, $green: -50)`
//! 各携一个数，必须落到不同颜色上，而这正是 registry 的位置参数表无法表达的东西。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// 被改的通道保住颜色自己的出处：由关键字造出的颜色在新字节仍命名一个关键字时印关键字，否则印
/// hex（`red/max`、`green/max`、`blue/max`、`red/min`、`red/low`）。
#[tokio::test]
async fn change_keeps_the_colour_origin() {
    for (input, want) in [
        ("a {b: color.change(black, $red: 255)}", "red"),
        ("a {b: color.change(black, $green: 255)}", "lime"),
        ("a {b: color.change(black, $blue: 255)}", "blue"),
        ("a {b: color.change(red, $red: 0)}", "black"),
        ("a {b: color.change(red, $red: 100)}", "#640000"),
        ("a {b: color.change(black, $red: 200)}", "#c80000"),
        ("a {b: color.change(blue, $blue: 100)}", "#000064"),
        // `@use "sass:color"` 拼法到达同一个函数（`modules/color/change.hrx`）。
        ("a {b: color.change(lime, $green: 0)}", "black"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **越域结果以 hsl 形式印出**，因为越界字节是 Sass 携带的值而不是它夹紧的值：`red/out_of_range`
/// 是 `color.change(black, $red: 500)` → `hsl(0, 5000%, 98.0392156863%)`，`blue/out_of_range` 是
/// `color.change(black, $blue: 256)` → `hsl(240, 100.7874015748%, 50.1960784314%)`。把第二个夹到
/// `rgb(0, 0, 255)` 会印出另一个颜色，而舍入会把 `100.7874015748%` 压平为 `100%`。
#[tokio::test]
async fn an_out_of_gamut_change_prints_hsl() {
    for (input, want) in [
        (
            "a {b: color.change(black, $red: 500)}",
            "hsl(0, 5000%, 98.0392156863%)",
        ),
        (
            "a {b: color.change(black, $blue: 256)}",
            "hsl(240, 100.7874015748%, 50.1960784314%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 调用点名的 **hsl 轴**替换颜色自己的：`color.change(plum, $saturation: 100%)` → `#ff7eff`
/// （出处保住，字节是整数），`color.change(red, $lightness: 63%)` → `rgb(100%, 26%, 26%)`（分数字节
/// ⇒ 百分比），而越域轴**逐字**印出所写的 hsl 三元组：`color.change(red, $lightness: 120%)` →
/// `hsl(0, 100%, 120%)`（`change/hsl.hrx::lightness/above_range`）。
///
/// 从隐含字节反推那个三元组会印成 `hsl(180, -100%, 120%)`——负 chroma 转动色相——这正是 `change`
/// 造出的颜色记住自己所得轴的原因。色域按**那些轴**判定，而它们的低端不对称，两样都由行证明：
/// 零以下的彩度仍是能印自己字节的颜色（`color.change(plum, $saturation: -20%)` →
/// `rgb(69.6470588235%, 79.7647058824%, 69.6470588235%)`，`…::saturation/below_range`），而零以下的
/// 明度不是（`color.change(red, $lightness: -20%)` → `hsl(0, 100%, -20%)`，
/// `…::lightness/below_range`）。
///
/// `units/hue/*` 各行也表明 `60deg`、`60` 与 `60in` 都指同一个数；那些 fixture 带一条 harness 不比较
/// 的弃用警告，故对上它们的输出不算弃用支持。
#[tokio::test]
async fn change_writes_the_hsl_axes_too() {
    for (input, want) in [
        ("a {b: color.change(plum, $saturation: 100%)}", "#ff7eff"),
        (
            "a {b: color.change(red, $lightness: 63%)}",
            "rgb(100%, 26%, 26%)",
        ),
        (
            "a {b: color.change(red, $lightness: 0.5%)}",
            "rgb(1%, 0%, 0%)",
        ),
        (
            "a {b: color.change(red, $lightness: 120%)}",
            "hsl(0, 100%, 120%)",
        ),
        (
            "a {b: color.change(plum, $saturation: 120%)}",
            "hsl(300, 120%, 74.7058823529%)",
        ),
        (
            "a {b: color.change(plum, $saturation: -20%)}",
            "rgb(69.6470588235%, 79.7647058824%, 69.6470588235%)",
        ),
        (
            "a {b: color.change(red, $lightness: -20%)}",
            "hsl(0, 100%, -20%)",
        ),
        ("a {b: color.change(red, $hue: 540)}", "aqua"),
        ("a {b: color.change(red, $hue: 60deg)}", "yellow"),
        ("a {b: color.change(red, $hue: 60in)}", "yellow"),
        (
            "a {b: color.change(black, $hue: 12, $saturation: 24%, $lightness: 48%)}",
            "rgb(59.52%, 41.088%, 36.48%)",
        ),
        // 与通道并列点名的 alpha（`change/rgb.hrx::alpha_arg`）。
        (
            "a {b: color.change(black, $red: 12, $green: 24, $blue: 48, $alpha: 0.3)}",
            "rgba(12, 24, 48, 0.3)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **白度与黑度可以不带任何 `$space:` 标签写入**——名字本身就说它属于哪个空间（`change/hwb.hrx`；
/// 对照 `color.channel`，它跨空间*读*才需要 `$space:`）。下面每行都是该文件自己的对子，同时钉住
/// 三件事：出处保住（`#cc6666` → `#cc0000`，hex，而不是 `rgb(…)`），一对溢出按其和归一化
/// （`color.change(#cc6666, $whiteness: 100%)` → 三通道全
/// `83.3333333333%`，即 `100/120`），以及一对离域的打印 hsl 三元组
/// （`color.change(#993333, $whiteness: 50%, $blackness: -20%)` → `hsl(0, 233.3333333333%, 85%)`）。
#[tokio::test]
async fn change_writes_whiteness_and_blackness() {
    for (input, want) in [
        ("a {b: color.change(#cc6666, $whiteness: 0%)}", "#cc0000"),
        ("a {b: color.change(#cc6666, $whiteness: 20%)}", "#cc3333"),
        ("a {b: color.change(#cc6666, $whiteness: 80%)}", "#cccccc"),
        (
            "a {b: color.change(#cc6666, $whiteness: 100%)}",
            "rgb(83.3333333333%, 83.3333333333%, 83.3333333333%)",
        ),
        ("a {b: color.change(#993333, $blackness: 0%)}", "#ff3333"),
        ("a {b: color.change(#993333, $blackness: 20%)}", "#cc3333"),
        ("a {b: color.change(#993333, $blackness: 80%)}", "#333333"),
        (
            "a {b: color.change(#993333, $blackness: 100%)}",
            "rgb(16.6666666667%, 16.6666666667%, 16.6666666667%)",
        ),
        (
            "a {b: color.change(#993333, $whiteness: 50%, $blackness: -20%)}",
            "hsl(0, 233.3333333333%, 85%)",
        ),
        // 色相随白/黑对一起走，无论位置还是标签。
        (
            "a {b: color.change(blue, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
            "#339966",
        ),
        (
            "a {b: color.change($color: blue, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
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

/// 本文件**不**认领的行，分两种。一种要把 `none` 写进**轴**，而轴三元组还携不了：
/// `color.change(hsl(0deg 100% 50%), $hue: none)` → `hsl(none 100% 50%)`（`change/hsl.hrx::hue/none`）
/// 与 `color.change(hwb(180deg 30% 50%), $whiteness: none)` → `hwb(180deg none 50%)`
/// （`change/hwb.hrx::whiteness/none`）正因如此保留文本。
///
/// 那张单子的字节半边已经不在：`color.change(black, $red: none)` → `rgb(none 0 0)`
/// （`change/rgb.hrx::{red,green,blue}/none`，以及 `change/space.hrx::missing/arg/legacy/same/implicit`
/// 处的 `color.change(pink, $red: none)` → `rgb(none 192 203)`）自本批起有答案：该行在 run
/// `change-writes-absence` 里首次转绿，在已入库基线 `change-writes-absence-v4` 里也绿（那 42 条
/// 逼出读序修复的 run `change-writes-absence-v2` 是第一个值得留的形状），故把它钉成拒答就断言了
/// 归档的反面。它作为答案钉在
/// `tests/color_legacy_missing.rs::change_can_write_absence_into_a_byte_channel`。
///
/// 一个是本编译器无法转换的空间（`change/space.hrx::identical`），保留文本恰好对上语料印出的。
///
/// 还有一对是**未解释、留红而不是拟合**：`change/hwb.hrx::alpha_input` 与 `…::alpha_arg` 在点名
/// alpha 后印出与 `rgba(20%, 60%, 40%, 0.7)` 相同的颜色，而 `…::all` 把它印成不透明的 `#339966`
/// （已在上方断言），`rgb(#123, 0.5)` 又印小数字节（`rgb/two_args::opaque_to/partial`）。此处没有
/// 任何机制能调和三者；发明一个会用两条看得见的红行换一个通过数看不见的错值。
///
/// 还有第三处分歧在 hue 推导自身：`change/rgb.hrx::green/out_of_range` 为 `(0, -50, 0)` 写
/// `hsl(120, 100%, -9.8039215686%)`，而 legacy hsl 公式给 hue `300` 与彩度 `-100%`，该文件的
/// `red/` 与 `blue/out_of_range` 各行却都与公式一致。
#[tokio::test]
async fn what_change_still_declines() {
    for call in [
        // `change/hsl.hrx::hue/none` —— 轴三元组还携不了 `none`。
        "color.change(hsl(0deg 100% 50%), $hue: none)",
        // `change/hwb.hrx::whiteness/none` —— 在 `hwb` 颜色上再来一次。
        "color.change(hwb(180deg 30% 50%), $whiteness: none)",
        // `change/space.hrx::identical` —— 关于本编译器无法转换的空间的请求；语料自己也不动
        // 那个颜色，故保留文本正好对上它的输出。
        "color.change(oklch(50% 0.2 0deg), $space: lab)",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "a call outside this slice's scope must keep its text: {call}"
        );
    }
}
