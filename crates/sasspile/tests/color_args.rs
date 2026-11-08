//! CSS Color 4 的**单参数**颜色拼法（`rgb(18 52 86)`、`hsl(0 100% 50%)`），以及越域百分比行
//! 暴露的夹紧律。每条期望都是那份 fixture 自己的 `output.css`。
//!
//! 这处读法是一个词素级的事实：`calls::split` 曾把整个调用串的空白剥掉再找被调用者，于是
//! `rgb(18 52 86)` 到达时是 `[Word("18"), Word("52"), Word("86")]`、没有任何分隔符，空格列表
//! 在词素以下的任何粒度都读不出来。保住括号内的排版才是这些行可读的根本。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `rgb/one_arg/no_alpha`：三个空格分隔的通道是一个参数，以同文件 `three_args` 各行所用的 legacy
/// 逗号形式回印。任何位置出现百分比都会转换整个颜色（`percents/unitless/green`），越域通道夹紧
/// （`unitless/clamped/*`），而含不可解析成分（`none`）的串原样保留（`missing/*`）。
#[tokio::test]
async fn rgb_space_separated_channels() {
    for (input, want) in [
        ("a {b: rgb(18 52 86)}", "rgb(18, 52, 86)"),
        ("a {b: rgb(190 173 237)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0 255 127)}", "rgb(0, 255, 127)"),
        // 单参数拼法也可**经标签**到达：`channels` 是 `rgb`/`hsl` 的真实参数（语料自己的诊断
        // 以 `$channels:` 为前缀——`rgb/error/one_arg.hrx`），而带标签的参数落在哪就绑到哪
        // （`rgb/one_arg/no_alpha.hrx::unitless/named`）。
        ("a {b: rgb($channels: 0 255 127)}", "rgb(0, 255, 127)"),
        ("a {b: rgb(256 0 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0 -1 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(7.1% 20.4% 33.9%)}", "rgb(7.1%, 20.4%, 33.9%)"),
        (
            "a {b: rgb(74.7% 173 93%)}",
            "rgb(74.7%, 67.8431372549%, 93%)",
        ),
        (
            "a {b: rgb(190 68% 237)}",
            "rgb(74.5098039216%, 68%, 92.9411764706%)",
        ),
        ("a {b: rgb(0% 100% 50%)}", "rgb(0%, 100%, 50%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `rgb/one_arg/no_alpha::percents/clamped/*` 陈述的是 legacy 逗号 fixture 从未走过的夹紧律：一个
/// 不得不动的百分比**丢掉 `%` 拼法**，印成它夹到的字节。精确的 `100%`（`percents/boundaries`）
/// 不动，故保住 `%`。同一条律也在这次改动一并转绿的 legacy 逗号行上回读
/// （`three_args/percents::clamped/{red,green,blue}`）。
#[tokio::test]
async fn a_clamped_percentage_prints_as_a_byte() {
    for (input, want) in [
        ("a {b: rgb(100.1% 0 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0 -0.1% 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(0 0 200%)}", "rgb(0, 0, 255)"),
        ("a {b: rgb(100.1%, 0, 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0, -0.1%, 0)}", "rgb(0, 0, 0)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hsl/one_arg/no_alpha`：与逗号形式同样的轴，经空格分隔读法到达。色相折成度（`units/hue/deg`），
/// 无单位的彩度与明度获得 `%`（`units/{saturation,lightness}/unitless`），彩度在 0 夹紧而明度在
/// fixture 所示的越域方向上不会变负（`out_of_gamut/saturation/above` 保留 `500%`）。
#[tokio::test]
async fn hsl_space_separated_axes() {
    for (input, want) in [
        ("a {b: hsl(0 100% 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(240 100% 50%)}", "hsl(240, 100%, 50%)"),
        ("a {b: hsl(60 60% 50%)}", "hsl(60, 60%, 50%)"),
        ("a {b: hsl(0 500% 50%)}", "hsl(0, 500%, 50%)"),
        ("a {b: hsl(0 -100% 50%)}", "hsl(0, 0%, 50%)"),
        ("a {b: hsl(0 100% 500%)}", "hsl(0, 100%, 500%)"),
        ("a {b: hsl(0deg 100% 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0 50 50%)}", "hsl(0, 50%, 50%)"),
        ("a {b: hsl(0 100% 50)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl($channels: 0 100% 50%)}", "hsl(0, 100%, 50%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 读法无法变成值的成分使整个串被拒——于是 `rgb(none 52 66)` 逐字印出，包括内部空格，既不塌成
/// 三参数列表也不塌成除法（`rgb/one_arg/no_alpha::missing/*`）。
#[tokio::test]
async fn an_unreadable_component_keeps_the_run_verbatim() {
    for input in [
        "a {b: rgb(none 52 66)}",
        "a {b: rgb(18 none 66)}",
        "a {b: rgb(18 52 none)}",
    ] {
        let body = input
            .strip_prefix("a {b: ")
            .unwrap()
            .strip_suffix("}")
            .unwrap();
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {body};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 现代的 **alpha 在斜杠后**拼法，`rgb/one_arg/alpha.hrx` 与 `hsl/one_arg/alpha.hrx` 每行一条断言。
/// 语料陈述了结构是什么：它的 `slash_list` 行写 `rgb(list.slash(0 255 127, 0))` 并期望与
/// `rgb(0 255 127 / 0)` 相同的颜色（`in_gamut/transparent`），所以 alpha 是 slash list 的**第二个
/// 元素**，而不是最后一个通道的除法——这也是为什么像 `127/0.3`（`/` 粘在成分内部）这样的成分保持
/// 算术。
///
/// 此处仍**不**认领的行，因为它们需要本编译器缺乏的能力：
/// * `slash_list` —— `list.slash(…)` 尚未注册；
/// * `missing/slash` 与 `missing/slash_list` —— `none` 作 alpha，语料对 `rgb` 把串留作
///   文本，而对 `hsl` 半重排（`hsl(180deg 60% 50% / none)`）。
#[tokio::test]
async fn modern_alpha_follows_a_slash() {
    for (input, want) in [
        ("a {b: rgb(18 52 86 / 0.5)}", "rgba(18, 52, 86, 0.5)"),
        // 百分比 alpha 会转换，与逗号形式第四参数同律。
        ("a {b: rgb(18 52 86 / 50%)}", "rgba(18, 52, 86, 0.5)"),
        // alpha 为 1 时回到三参数拼法；为 0 则不。
        ("a {b: rgb(190 173 237 / 1)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0 255 127 / 0)}", "rgba(0, 255, 127, 0)"),
        ("a {b: rgb(0 255 127 / 0.3)}", "rgba(0, 255, 127, 0.3)"),
        // 同一调用带标签写法（`rgb/one_arg/alpha.hrx::in_gamut/named`）。
        (
            "a {b: rgb($channels: 0 255 127 / 0.3)}",
            "rgba(0, 255, 127, 0.3)",
        ),
        ("a {b: hsl(180 60% 50% / 0.5)}", "hsla(180, 60%, 50%, 0.5)"),
        (
            "a {b: hsl($channels: 180 60% 50% / 0.4)}",
            "hsla(180, 60%, 50%, 0.4)",
        ),
        ("a {b: hsl(180 60% 50% / 1)}", "hsl(180, 60%, 50%)"),
        ("a {b: hsl(180 60% 50% / 0)}", "hsla(180, 60%, 50%, 0)"),
        // alpha 夹紧，各轴保持自己的律（负彩度到 0，明度不动）。
        ("a {b: hsl(0 100% 50% / 1.1)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0 100% 50% / -0.1)}", "hsla(0, 100%, 50%, 0)"),
        ("a {b: hsl(0 -0.1% 50% / 0.5)}", "hsla(0, 0%, 50%, 0.5)"),
        (
            "a {b: hsl(0 100% 9999% / 0.5)}",
            "hsla(0, 100%, 9999%, 0.5)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
