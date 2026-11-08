//! `color.scale`,在 `core_functions/color/scale/{rgb,hsl,hwb,no_space,global}.hrx` 上逐行钉住。
//! 每一条输入与期望,都照抄各夹具自己的 `input.scss` / `output.css`。
//!
//! scale 抛出的拒绝与措辞(`$hue: Channel isn't scalable.`、amount 必带 `%`、
//! `-100%…100%` 区间)在 [`tests/color_diagnostics.rs`](crate),那里收着三个函数族的共同诊断律。
//!
//! 本文件要钉的律:scale 把一个通道**沿其符号所指的方向,移动该通道到那个端点之间剩余距离的
//! 对应份额**——`sienna` 的 red `160` 配 `$red: 12%` 即 `160 + (255 − 160) × 0.12`,
//! `lightcoral` 的 red `240` 配 `-33%` 即 `240 − 240 × 0.33 = 160.8`。
//! 因为目标就是端点本身,scale 永远不会越出色域:`100%` **正好落在**端点上,这正是
//! `scale/rgb.hrx` 没有 `above_max` 行而 `adjust/rgb.hrx` 有的原因。
//!
//! 本文件钉的另一半是**参数的读法**:当份额读(`-33%` 即 33),不是当通道值读
//! (`byte()` 会把 `100%` 变成 255)。这里先前犯过那个错,害得每一条 `scale/rgb` 行都越出
//! 色域,所以能抓住它的行都断言两遍——一遍过关键字结果(`red/max` → `red`),
//! 一遍过小数结果(`red/high` → `89.5137254902%`)。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// 字节朝 `0` 或 `255` 移动(`scale/rgb.hrx`)。`black` 配 `$red: 100%` 是 `red`,`red` 配
/// `-100%` 是 `black`——即端点本身;`lightcoral`(red 240)配 `-33%` 得 `160.8` =
/// `63.0588235294%`,`turquoise`(red 64)配 `86%` 得 `228.26` = `89.5137254902%`。
/// 结果是整字节时保留颜色自身的出处:`blue/max` → 按名字叫 `blue`;
/// `salmon` 配 `$blue: 42%` → 百分比,因为移动后的 blue `224.34` 不是整字节。
#[tokio::test]
async fn bytes_move_toward_the_bound_the_sign_points_at() {
    for (input, want) in [
        ("a {b: color.scale(black, $red: 100%)}", "red"),
        ("a {b: color.scale(red, $red: -100%)}", "black"),
        ("a {b: color.scale(black, $red: 0%)}", "black"),
        (
            "a {b: color.scale(lightcoral, $red: -33%)}",
            "rgb(63.0588235294%, 50.1960784314%, 50.1960784314%)",
        ),
        (
            "a {b: color.scale(turquoise, $red: 86%)}",
            "rgb(89.5137254902%, 87.8431372549%, 81.568627451%)",
        ),
        ("a {b: color.scale(black, $green: 100%)}", "lime"),
        ("a {b: color.scale(lime, $green: -100%)}", "black"),
        ("a {b: color.scale(black, $blue: 100%)}", "blue"),
        ("a {b: color.scale(blue, $blue: -100%)}", "black"),
        (
            "a {b: color.scale(seagreen, $green: -86%)}",
            "rgb(18.0392156863%, 7.631372549%, 34.1176470588%)",
        ),
        (
            "a {b: color.scale(cadetblue, $green: 12%)}",
            "rgb(37.2549019608%, 66.5254901961%, 62.7450980392%)",
        ),
        (
            "a {b: color.scale(salmon, $blue: 42%)}",
            "rgb(98.0392156863%, 50.1960784314%, 67.9294117647%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 一次调用移动三个通道、带标签的颜色、以及 alpha 参数——`scale/rgb.hrx::{all,named,
/// alpha_input,alpha_arg}`。`sienna` (160, 82, 45) 朝 255 按 12 / 24 / 48% 缩放,无论颜色怎么
/// 称呼都得 `rgb(67.2156862745%, 48.4392156863%, 57.1764705882%)`;`$alpha: -70%` 把 alpha 从
/// 1 朝 0 按该份额移动 → `0.3`,与 `rgba(sienna, 0.3)` 到达同一个数。
#[tokio::test]
async fn one_call_can_move_several_channels_and_the_alpha() {
    let sienna = "rgb(67.2156862745%, 48.4392156863%, 57.1764705882%)";
    for (input, want) in [
        (
            "a {b: color.scale(sienna, $red: 12%, $green: 24%, $blue: 48%)}",
            sienna,
        ),
        (
            "a {b: color.scale($color: sienna, $red: 12%, $green: 24%, $blue: 48%)}",
            sienna,
        ),
        (
            "a {b: color.scale(rgba(sienna, 0.3), $red: 12%, $green: 24%, $blue: 48%)}",
            "rgba(67.2156862745%, 48.4392156863%, 57.1764705882%, 0.3)",
        ),
        (
            "a {b: color.scale(sienna, $red: 12%, $green: 24%, $blue: 48%, $alpha: -70%)}",
            "rgba(67.2156862745%, 48.4392156863%, 57.1764705882%, 0.3)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 同一公式作用于 hsl 轴,其端点界为 `0–100`(`scale/hsl.hrx`)。plum 自身饱和度是
/// 47.2868217054%,故 `100%` 到达 100 → `#ff7eff`,`0%` 恰好还是 `plum`;red 的 lightness 50
/// 按 `94%` 缩放得 97,对应字节为 `rgb(100%, 94%, 94%)`——这里 lightness 从不打印,因为
/// scale 不能越出色域,越域规则无从触发。
#[tokio::test]
async fn the_hsl_axes_scale_between_zero_and_a_hundred() {
    for (input, want) in [
        ("a {b: color.scale(plum, $saturation: 100%)}", "#ff7eff"),
        ("a {b: color.scale(plum, $saturation: 0%)}", "plum"),
        (
            "a {b: color.scale(plum, $saturation: -100%)}",
            "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)",
        ),
        (
            "a {b: color.scale(plum, $saturation: 67%)}",
            "rgb(95.6%, 53.8117647059%, 95.6%)",
        ),
        (
            "a {b: color.scale(plum, $saturation: -43%)}",
            "rgb(81.5235294118%, 67.8882352941%, 81.5235294118%)",
        ),
        ("a {b: color.scale(red, $lightness: 100%)}", "white"),
        ("a {b: color.scale(red, $lightness: 0%)}", "red"),
        ("a {b: color.scale(red, $lightness: -100%)}", "black"),
        (
            "a {b: color.scale(red, $lightness: 94%)}",
            "rgb(100%, 94%, 94%)",
        ),
        (
            "a {b: color.scale(red, $lightness: -14%)}",
            "rgb(86%, 0%, 0%)",
        ),
        (
            "a {b: color.scale(turquoise, $saturation: 24%, $lightness: -48%)}",
            "rgb(6.2327249603%, 52.4966868045%, 47.87029062%)",
        ),
        (
            "a {b: color.scale(rgba(turquoise, 0.7), $saturation: 24%, $lightness: -48%)}",
            "rgba(6.2327249603%, 52.4966868045%, 47.87029062%, 0.7)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 白度与黑度同样朝 `100` 与 `0` 缩放,随后溢出的一对**按两者和归一化**,做法与 `adjust`
/// 完全一致:`#66cc99`(w 40, b 20)配 `$whiteness: 100%` 变成 w 100 / b 20,归一化后为
/// 83.3333333333% / 16.6666666667%——即灰 `rgb(83.3333333333%, …)`
/// (`scale/hwb.hrx::whiteness/max`)。`$whiteness: -100%` 则抽空白度,得 `#00cc66`
/// (`…::whiteness/min`);`#33cc80` 配 `50%` 保留小数 blue(`70.0653594771%`),因为 hue
/// 扛过了往返(`…::whiteness/high`)。
#[tokio::test]
async fn whiteness_and_blackness_scale_then_normalise() {
    for (input, want) in [
        (
            "a {b: color.scale(#66cc99, $whiteness: 100%)}",
            "rgb(83.3333333333%, 83.3333333333%, 83.3333333333%)",
        ),
        ("a {b: color.scale(#66cc99, $whiteness: 0%)}", "#66cc99"),
        ("a {b: color.scale(#66cc99, $whiteness: -100%)}", "#00cc66"),
        (
            "a {b: color.scale(#33cc80, $whiteness: 50%)}",
            "rgb(60%, 80%, 70.0653594771%)",
        ),
        (
            "a {b: color.scale(#66cc99, $whiteness: -50%)}",
            "rgb(20%, 80%, 50%)",
        ),
        (
            "a {b: color.scale(#339966, $blackness: 100%)}",
            "rgb(16.6666666667%, 16.6666666667%, 16.6666666667%)",
        ),
        ("a {b: color.scale(#339966, $blackness: 0%)}", "#339966"),
        ("a {b: color.scale(#339966, $blackness: -100%)}", "#33ff99"),
        (
            "a {b: color.scale(#33cc80, $blackness: 50%)}",
            "rgb(20%, 40%, 30.0653594771%)",
        ),
        (
            "a {b: color.scale(#66cc99, $whiteness: -50%, $blackness: 50%)}",
            "rgb(20%, 40%, 30%)",
        ),
        (
            "a {b: color.scale(rgba(#66cc99, 0.7), $whiteness: -50%, $blackness: 50%)}",
            "rgba(20%, 40%, 30%, 0.7)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// alpha 的端点界为 `0–1`,而只点名 **alpha 之外什么也没点名** 的调用是空操作而非报错
/// (`scale/no_space.hrx`)。`rgba(red, 0.5)` 配 `$alpha: 100%` 重新变不透明——且由关键字
/// 构成的不透明颜色打印该关键字;`0%` 什么也没变;`-100%` 抽空它。从 0.5 到 1 的 `14%`
/// 处是 `0.57`,从 0.3 到 0 的 `-36%` 处是 `0.192`。
#[tokio::test]
async fn the_alpha_scales_between_zero_and_one_and_nothing_named_is_a_no_op() {
    for (input, want) in [
        ("a {b: color.scale(red)}", "red"),
        ("a {b: color.scale($color: red)}", "red"),
        ("a {b: color.scale(rgba(red, 0.5), $alpha: 100%)}", "red"),
        (
            "a {b: color.scale(rgba(red, 0.5), $alpha: 0%)}",
            "rgba(255, 0, 0, 0.5)",
        ),
        (
            "a {b: color.scale(rgba(red, 0.5), $alpha: -100%)}",
            "rgba(255, 0, 0, 0)",
        ),
        (
            "a {b: color.scale(rgba(red, 0.5), $alpha: 14%)}",
            "rgba(255, 0, 0, 0.57)",
        ),
        (
            "a {b: color.scale(rgba(red, 0.3), $alpha: -36%)}",
            "rgba(255, 0, 0, 0.192)",
        ),
        // 旧式全局拼写是同一个函数(`scale/global.hrx::legacy`):pink 的 blue 203 移动了
        // 255 之下剩余 52 的 20%。
        (
            "a {b: scale-color(pink, $blue: 20%)}",
            "rgb(100%, 75.2941176471%, 83.6862745098%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 这是**本切片**的边界,不是语料行:输出为本编译器无法产出的调用,保留调用文本——
/// `$space:` 指向它没有转换的空间,以及它拒绝读入 sRGB 的现代 `color(…)` 颜色。
/// 那些行保持红色,而钉住文本正是拦住后续改动悄悄用错误空间回答它们的手段。
///
/// `none` 通道**不再**在此列表中,语料是原因:`scale/no_channels.hrx::missing` 请求
/// `color.scale(rgb(none none none))` 并打印 `rgb(none none none)`——是回答,不是调用文本。
/// 该行在基线运行 `instrument-split-and-guards` 中为红,自运行 `legacy-missing-channel` 起为绿
/// (`reports/hrx.db`),所以把它留在这里会钉住与语料库矛盾的行为。
///
/// hue 也不在此:`color.scale(white, $hue: 10%)` 用语料自带的消息作答,钉在
/// `tests/color_diagnostics.rs`(`scale/error/args.hrx::hue`)。
#[tokio::test]
async fn calls_this_slice_cannot_answer_keep_their_text() {
    for call in [
        // `$space:` 要求本编译器没有转换的空间(`scale/no_channels.hrx::identical`
        // 是 oklch 行,其文本与该行自身期望不符——该行有意保持红色)。
        "color.scale(oklch(50% 0.2 0deg), $space: lab)",
        // 以文本书写的现代颜色不读入 sRGB(`scale/out_of_gamut.hrx::above_gamut/up`)。
        "color.scale(color(srgb 1.2 0.5 0.7), $red: 10%)",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "a call outside this slice's scope must keep its text: {call}"
        );
    }
}
