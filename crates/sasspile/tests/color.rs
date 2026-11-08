//! 颜色定律，钉在 `core_functions/color/rgb/**` 的 golden 上。每条期望是该 fixture 自身的
//! `output.css`，不是推导：颜色被创建的表示决定它如何打印，所以相同通道会输出为
//! `rgb(…)`、`rgb(…%…)`、`rgba(…)` 或 `#112233`。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `rgb/three_args/unitless`：通道按所写打印，越界者钳制到 `0–255`
/// （`clamped/{red,green,blue}`）。
#[tokio::test]
async fn unitless_channels_print_and_clamp() {
    for (input, want) in [
        ("a {b: rgb(18, 52, 86)}", "rgb(18, 52, 86)"),
        ("a {b: rgb(190, 173, 237)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0, 255, 127)}", "rgb(0, 255, 127)"),
        ("a {b: rgb(256, 0, 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0, -1, 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(0, 0, 9999)}", "rgb(0, 0, 255)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `rgb/three_args/percents`：任一位置的百分比使整个颜色成为百分比形式，所以无单位
/// 通道被转换（`173` → `67.8431372549%`），而写出的百分比严格保留拼写。
#[tokio::test]
async fn one_percentage_channel_makes_all_percentages() {
    assert_eq!(
        css("a {b: rgb(7.1%, 20.4%, 33.9%)}").await,
        "a {\n  b: rgb(7.1%, 20.4%, 33.9%);\n}\n"
    );
    assert_eq!(
        css("a {b: rgb(74.7%, 173, 93%)}").await,
        "a {\n  b: rgb(74.7%, 67.8431372549%, 93%);\n}\n"
    );
}

/// `rgb/two_args::opaque_to/*` 与 `rgb/four_args/clamped/*`：由 hex 建的颜色在不透明时
/// 打印为 hex，一旦半透明即打印为 `rgba(…)`，alpha 实参选择四参数拼写。
/// 负零是 `0`，不是 `-0`。
#[tokio::test]
async fn representation_and_alpha_choose_the_spelling() {
    for (input, want) in [
        ("a {b: rgb(#123, 1)}", "#112233"),
        ("a {b: rgb(#123, 0.5)}", "rgba(17, 34, 51, 0.5)"),
        ("a {b: rgb(256, 0, 0, 0.5)}", "rgba(255, 0, 0, 0.5)"),
        ("a {b: rgb(0, -0, 0, 0.5)}", "rgba(0, 0, 0, 0.5)"),
        ("a {b: rgb(0, 0, 9999, 0.5)}", "rgba(0, 0, 255, 0.5)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hsl/three_args/units.hrx` 与 `…/bounds.hrx`，逐行读取：**hue 被折叠为度数并
/// 环绕进 `0–360`**（`60rad` → `197.7467707849`，`0deg` → `0`），饱和度与亮度
/// **总是**带 `%`（`50` → `50%`），而且与 `rgb` 通道不同，它们**不被钳制**（`500%` 保留）。
/// `hsl/four_args/alpha.hrx` 补充 alpha 定律：百分比 alpha 转换，为 1 的 alpha 返回
/// 三参数拼写。
#[tokio::test]
async fn hsl_folds_hue_and_prints_percentages() {
    for (input, want) in [
        ("a {b: hsl(0, 100%, 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0deg, 100%, 50%)}", "hsl(0, 100%, 50%)"),
        (
            "a {b: hsl(60rad, 100%, 50%)}",
            "hsl(197.7467707849, 100%, 50%)",
        ),
        ("a {b: hsl(0, 50, 50%)}", "hsl(0, 50%, 50%)"),
        ("a {b: hsl(0, 500%, 50%)}", "hsl(0, 500%, 50%)"),
        ("a {b: hsl(0, 0%, 0%, 45.6%)}", "hsla(0, 0%, 0%, 0.456)"),
        ("a {b: hsl(0, 0%, 0%, 100%)}", "hsl(0, 0%, 0%)"),
        ("a {b: hsl(0, 0%, 0%, 250%)}", "hsl(0, 0%, 0%)"),
        ("a {b: hsl(0, 0%, 0%, -10%)}", "hsla(0, 0%, 0%, 0)"),
        // 标签绑定，对 `hsl` 与 `rgb` 都一样——这正是注册表带可选参数的理由
        // （`hsl/three_args/named.hrx`、`rgb/three_args/unitless::named`）。
        (
            "a {b: hsl($hue: 0, $saturation: 100%, $lightness: 50%)}",
            "hsl(0, 100%, 50%)",
        ),
        (
            "a {b: rgb($red: 0, $green: 255, $blue: 127)}",
            "rgb(0, 255, 127)",
        ),
        // 不可建模的实参保持调用原样（`hsl/three_args/special_functions::var/*`）。
        ("a {b: hsl(var(--foo), 2%, 3%)}", "hsl(var(--foo), 2%, 3%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **不应**发生之事。颜色定律无法读取的实参拒绝该调用，行保留其逐字文本——
/// `three_args::1_of_3`（`var()` 在解析前被替换，所以列表可含任何内容）正是这样的
/// golden。而直接写进声明的 hex 根本不是 `Color`：`values/colors/alpha_hex/initial_letter`
/// 打印 `#AbCd`，大小写完好，所以在此归一化它就是伪装成胜利的回归。
#[tokio::test]
async fn unmodellable_arguments_decline_and_hex_is_left_alone() {
    assert_eq!(
        css("a {b: rgb(var(--foo), 3, 0.4)}").await,
        "a {\n  b: rgb(var(--foo), 3, 0.4);\n}\n"
    );
    assert_eq!(css("a {b: #AbCd}").await, "a {\n  b: #AbCd;\n}\n");
    assert_eq!(css("a {b: #98765432}").await, "a {\n  b: #98765432;\n}\n");
    // 通道是数字或百分比；`px` 两者皆非，且不为其发明错误消息（P7）。
    assert_eq!(
        css("a {b: rgb(1px, 2px, 3px)}").await,
        "a {\n  b: rgb(1px, 2px, 3px);\n}\n"
    );
}

/// `color.*` 访问器，以及选项 (a) 承诺的审计：关键字表来自 `color` crate 的关键字表，
/// 所以这里每条期望都是**语料库自身**钉住的名称。`gray` 被双重钉住——`equality.hrx`
/// 声明 `gray == hsl(none 0% 50.196078431373%)`（128/255）且 `color.invert(gray)` →
/// `#7f7f7f`（128 取反是 127）；`springgreen` 是 `rgb/three_args/unitless::springgreen`
/// 为 `rgb(0, 255, 127)` 所写的；`purple` 与 `pink` 来自 `equality.hrx` 与
/// `channel/hwb::foreign/whiteness`（`75.2941176471%` 是 `192/255`）。
#[tokio::test]
async fn accessors_read_channels_and_the_keyword_table_matches_the_corpus() {
    for (input, want) in [
        // fixture 行自身（`core_functions/color/red.hrx::{max,min,middle,named}`）。
        ("a {b: color.red(rgb(255, 0, 0))}", "255"),
        ("a {b: color.red(rgb(0, 0, 0))}", "0"),
        ("a {b: color.red(rgb(123, 0, 0))}", "123"),
        ("a {b: color.red($color: rgb(234, 0, 0))}", "234"),
        // `color.alpha`（`alpha.hrx::color/{max,middle,min,named}`）。
        ("a {b: color.alpha(red)}", "1"),
        ("a {b: color.alpha(rgba(red, 0.42))}", "0.42"),
        ("a {b: color.alpha(rgba(red, 0))}", "0"),
        ("a {b: color.alpha($color: rgba(red, 0.73))}", "0.73"),
        // 关键字解析，与上面的语料库值交叉核对。
        ("a {b: color.red(black)}", "0"),
        ("a {b: color.red(white)}", "255"),
        ("a {b: color.red(gray)}", "128"),
        // CSS 的 `-grey` 拼写是同一条目，语料库确实使用它们（`invert(grey)`、
        // `color.adjust(grey, …)`），所以别名必须解析到相同字节。
        ("a {b: color.red(grey)}", "128"),
        ("a {b: color.green(springgreen)}", "255"),
        ("a {b: color.blue(springgreen)}", "127"),
        ("a {b: color.red(purple)}", "128"),
        ("a {b: color.blue(pink)}", "203"),
        // hex 实参以文本到达，并从那里读取，而不是从解析后的声明值读取。
        ("a {b: color.red(#123)}", "17"),
        // `transparent` 是依赖知道而旧手写清单没有的关键字：下面两个 alpha 是
        // `rgb/two_args::transparent_to/{partial,transparent}`。该组的第三行
        // `rgb(transparent, 1)` → `black` **不**在此覆盖——把颜色打印为其名称是
        // 反查定律，仍未完成。
        ("a {b: rgb(transparent, 0.5)}", "rgba(0, 0, 0, 0.5)"),
        ("a {b: rgb(transparent, 0)}", "rgba(0, 0, 0, 0)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 诚实的边界：**hsl** 空间中的颜色没有可交付的 `0–255` 通道。
///
/// 这一条是测量而非假设，与下面的反方向不同：`grep` 在 `spec/` 下每个 `.hrx` 中搜
/// `color.(red|green|blue)(…hsl(…))` 返回**零行**——hsl 空间的 fixture 改为读取 hsl 轴
/// （`channel/hsl.hrx` 读 `hue`/`saturation`/`lightness`），所以没有 golden 可对照写
/// `hsl`→`rgb` 转换。报告从错误轴算出的数字会是判为通过错答案，所以该调用拒绝，文本保留。
#[tokio::test]
async fn an_hsl_colour_declines_an_rgb_channel() {
    assert_eq!(
        css("a {b: color.red(hsl(0, 100%, 50%))}").await,
        "a {\n  b: color.red(hsl(0, 100%, 50%));\n}\n"
    );
}

/// `color.hue` / `color.saturation` / `color.lightness`，每条 fixture 行一个输出，每行读出的
/// 一个输入来自 `core_functions/color/{hue,saturation,lightness}.hrx`——包括推理不会给出的
/// 轴单位：hue 打印 `359deg`，饱和度与亮度打印 `42%`。
///
/// 这三个文件中的每条值行都喂给 `hsl(…)` 颜色，所以本切片**不需要** rgb→hsl 转换；
/// 其中的非 `hsl` 实参（`1`、`lch(0% 0 0deg)`、`red, green`）是 `error/*` 行，要求本项目
/// 尚未构建的消息（P7）。
#[tokio::test]
async fn hue_saturation_lightness_read_the_hsl_axes() {
    for (input, want) in [
        ("a {b: color.hue(hsl(359, 50%, 50%))}", "359deg"),
        // 折叠发生在创建时（`hsl/three_args/units.hrx`），访问器只是读回它。
        ("a {b: color.hue(hsl(540, 50%, 50%))}", "180deg"),
        ("a {b: color.hue(hsl(0, 50%, 50%))}", "0deg"),
        ("a {b: color.hue(hsl(123, 50%, 50%))}", "123deg"),
        ("a {b: color.hue($color: hsl(234, 50%, 50%))}", "234deg"),
        ("a {b: color.saturation(hsl(0, 100%, 50%))}", "100%"),
        ("a {b: color.saturation(hsl(0, 50%, 50%))}", "50%"),
        ("a {b: color.saturation($color: hsl(0, 42%, 50%))}", "42%"),
        ("a {b: color.lightness(hsl(0, 100%, 100%))}", "100%"),
        ("a {b: color.lightness(hsl(0, 100%, 0%))}", "0%"),
        ("a {b: color.lightness(hsl(0, 100%, 50%))}", "50%"),
        ("a {b: color.lightness($color: hsl(0, 100%, 42%))}", "42%"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **另一个**方向。其中三行是 fixture——`core_functions/modules/color/{hue,saturation,
/// lightness}.hrx` 各自把一个 `rgb` 空间颜色喂给 hsl 访问器，这正是推翻本文件早期版本的
/// 证据（它把每个这样的调用以“没有 golden”拒绝）。期望是语料库自身的数字，用其声明的
/// 单位打印：
/// * 值行：`color.hue(#abcdef)` → `210deg`，`color.saturation(#abcdef)` → `68%`，
///   `color.lightness(#abcdef)` → `80.3921568627%`（`#abcdef` = `(171, 205, 239)`）。
///
/// 最后两行**不是** fixture 行；它们是同一公式对照语料库确有声明的话检验：
/// `values/colors/equality.hrx` 写 `purple == hsl(300, 100%, 25.098039215686%)`，而
/// `purple` = `(128, 0, 128)` 在这里给出恰好的 hue `300` 与 saturation `100%`。灰色刻意
/// 不作断言：同一文件声明 `gray == hsl(none 0% 50.196078431373%)`，而 `color.hue`
/// 如何打印无 hue 的颜色不是这三行能解决的。
#[tokio::test]
async fn rgb_colours_answer_hsl_axis_questions() {
    for (input, want) in [
        ("a {b: color.hue(#abcdef)}", "210deg"),
        ("a {b: color.saturation(#abcdef)}", "68%"),
        ("a {b: color.lightness(#abcdef)}", "80.3921568627%"),
        ("a {b: color.hue(purple)}", "300deg"),
        ("a {b: color.saturation(purple)}", "100%"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// hsl 往返与其暴露的打印规则，每条 fixture 行一条断言
/// （`core_functions/color/grayscale.hrx`）。`#633736` / `#494949` 这对就是全部定律：
/// 同一操作在往返落在字节之间时打印 `rgb(30%, 30%, 30%)`，落在字节上时打印 `#494949`
/// ——所以舍入属于打印，不属于转换。
#[tokio::test]
async fn grayscale_round_trips_and_prints_by_whole_bytes() {
    for (input, want) in [
        ("a {b: color.grayscale(#633736)}", "rgb(30%, 30%, 30%)"),
        ("a {b: color.grayscale(#494949)}", "#494949"),
        ("a {b: color.grayscale(red)}", "rgb(50%, 50%, 50%)"),
        (
            "a {b: color.grayscale(rgba(#633736, 0.3))}",
            "rgba(30%, 30%, 30%, 0.3)",
        ),
        // `color.grayscale(hsl(120deg 10% 20%))` → `hsl(120, 0%, 20%)` 是同文件的真实行，
        // 此处**不**声明：其输入是现代空格分隔拼写，本编译器尚不能解析，而其输出是
        // 旧式逗号形式。
        //
        // 唯一旧式形状的 hsl 源用例（`hsl(120, 10%, 20%)`）也没有 golden，所以不为其断言
        // 输出——从转换后的 hsl 颜色打印 hsl 颜色在 fixture 声明它之前保持未声明。
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 促成可变参数注册的 CSS 重载：`grayscale(10%)` 是滤镜函数，不是颜色调用，
/// 所以它必须不受影响地保留（`saturate.hrx` 对 `saturate(50%)` 声明同样的话）。
#[tokio::test]
async fn a_filter_argument_is_not_a_colour_call() {
    assert_eq!(
        css("a {b: grayscale(50%)}").await,
        "a {\n  b: grayscale(50%);\n}\n"
    );
}

/// 旧式 hsl 操纵器，从 `darken.hrx` / `lighten.hrx` / `desaturate.hrx` 逐行读出的一条断言。
/// 它们同时演练轴定律的两半：`desaturate(plum, 100%)` 与 `desaturate(plum, 48%)` 是同一颜色，
/// 因为饱和度在 100 钳制，而两种打印（`rgb(74.7058823529%, …)` vs `rgba(0, 0, 0, 0.2)`）
/// 只因一个落在整字节之间而不同。
///
/// 期望输出是颜色**名称**的行（`desaturate(plum, 0%)` → `plum`、`darken(red, 100%)` →
/// `black`）此处不声明——反向名称打印仍未完成。`saturate` 尚未在任何地方注册：
/// 见 tasks.md 1.5 中会把 `saturate($c)` 变成未替换文本的拒绝路径缺陷。
#[tokio::test]
async fn legacy_hsl_manipulators_move_the_axes_and_clamp() {
    for (input, want) in [
        ("a {b: darken(red, 14%)}", "rgb(72%, 0%, 0%)"),
        ("a {b: darken(red, 0.5%)}", "rgb(99%, 0%, 0%)"),
        (
            "a {b: darken($color: red, $amount: 14%)}",
            "rgb(72%, 0%, 0%)",
        ),
        ("a {b: darken(rgba(red, 0.2), 100%)}", "rgba(0, 0, 0, 0.2)"),
        ("a {b: lighten(red, 14%)}", "rgb(100%, 28%, 28%)"),
        ("a {b: lighten(red, 0.5%)}", "rgb(100%, 1%, 1%)"),
        (
            "a {b: desaturate(plum, 100%)}",
            "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)",
        ),
        (
            "a {b: desaturate(plum, 48%)}",
            "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **被拒绝**的调用必须保留什么。最初先尝试从求值后的实参重建调用，度量出 40 处回归，
/// 所以这里钉住语料库实际要求的三条规则；代码保留 token 行，只解析其中的变量读取
/// （`expr::interpolated`）。
#[tokio::test]
async fn a_declined_call_keeps_its_text_and_still_resolves_variables() {
    for (input, want) in [
        // Sass 变量变为其值（`saturate.hrx::css_overload/with_sass_var`）。
        ("$c: 1;\na {b: saturate($c)}", "saturate(1)"),
        // 一个实参内部的空格保留：`rgb(18 52 none)` 是单个现代语法实参，其通道不可建模
        // （`rgb/one_arg/no_alpha::missing/red`），行按所写打印。
        ("a {b: rgb(none 52 66)}", "rgb(none 52 66)"),
        // 另一个函数实参内部逗号前的空格也保留
        // （`rgb/three_args/special_functions::attr/arg_1`）。
        ("a {b: rgb(attr(c, %), 2, 3)}", "rgb(attr(c, %), 2, 3)"),
        // CSS 滤镜拼写根本不是颜色调用（`saturate.hrx::css_overload/unit`）。
        ("a {b: saturate(50%)}", "saturate(50%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// **由关键字创建**的颜色在其字节仍命名该关键字时再次打印该关键字，而语料库把这与
/// 纯值的规则分开：由 `rgb(0, 0, 0)` 建的颜色打印 `rgb(0, 0, 0)`，而非 `black`
/// （`core_functions/color/rgb/**` 下的六行），所以来源才是关键。
///
/// 关键字表本身不携带 RGB 值——每个名称经 `color::parse_color` 解析——所以这些行
/// 也审计拼写：写错的名称解析失败，会打印 hex。
#[tokio::test]
async fn a_keyword_created_colour_prints_its_keyword() {
    for (input, want) in [
        // 值不变，同一关键字返回（`lighten.hrx::min`、`darken.hrx::min`）。
        ("a {b: lighten(red, 0%)}", "red"),
        ("a {b: darken(red, 0%)}", "red"),
        // 灰色保留其来源关键字（`grayscale.hrx::{max,min}`）。
        ("a {b: color.grayscale(white)}", "white"),
        ("a {b: color.grayscale(black)}", "black"),
        // 无饱和度的颜色不变（`desaturate.hrx::min`）。
        ("a {b: desaturate(plum, 0%)}", "plum"),
        // `transparent` 也是关键字；在 alpha 1 时其字节命名 `black`
        // （`rgb/two_args::transparent_to/opaque`）。
        ("a {b: rgb(transparent, 1)}", "black"),
        // 由数字构建的用例，**不**变成关键字。
        ("a {b: rgb(0, 0, 0)}", "rgb(0, 0, 0)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `color.mix`，`core_functions/color/mix/{unweighted,explicit_weight,alpha,units}.hrx` 中
/// 每条 fixture 一行。两条 `rgba` 行正是钉住**按 alpha 加权**公式而非简单平均之物：以
/// `0.8`/`0.3`，第一色承载 `0.75`（`145 × 0.75 + 1 × 0.25 = 109` → `42.7450980392%`），
/// 镜像行 `0.4`/`0.9` 给出 `0.25` → `37` → `14.5098039216%`。alpha 本身是平均，结果
/// 保留**第一个**颜色的表示——这就是为什么 `mix(#91e16f, transparent)` 打印整字节数字
/// 而 `mix(#91e16f, #0144bf)` 打印百分比（255 中的 73 不是整字节）。
#[tokio::test]
async fn mix_averages_channels_weighted_by_alpha() {
    for (input, want) in [
        ("a {b: color.mix(#ff00ff, #00ff00)}", "rgb(50%, 50%, 50%)"),
        ("a {b: color.mix(#123456, #123456)}", "#123456"),
        (
            "a {b: color.mix(#91e16f, #0144bf)}",
            "rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)",
        ),
        ("a {b: color.mix(#91e16f, #0144bf, 100%)}", "#91e16f"),
        ("a {b: color.mix(#91e16f, #0144bf, 0%)}", "#0144bf"),
        // 权重无论带什么单位都按百分比读取（`mix/units.hrx`）。
        (
            "a {b: color.mix(#91e16f, #0144bf, 50px)}",
            "rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)",
        ),
        (
            "a {b: color.mix(rgba(#91e16f, 0.8), rgba(#0144bf, 0.3))}",
            "rgba(42.7450980392%, 72.8431372549%, 51.3725490196%, 0.55)",
        ),
        (
            "a {b: color.mix(rgba(#91e16f, 0.4), rgba(#0144bf, 0.9))}",
            "rgba(14.5098039216%, 42.0588235294%, 67.0588235294%, 0.65)",
        ),
        (
            "a {b: color.mix(#91e16f, transparent)}",
            "rgba(145, 225, 111, 0.5)",
        ),
        (
            "a {b: color.mix(transparent, #0144bf)}",
            "rgba(1, 68, 191, 0.5)",
        ),
        // 标签绑定（`mix/named.hrx` 的矩形行是相同数字配 `$weight: 92%`）。
        (
            "a {b: color.mix($color1: #91e16f, $color2: #0144bf, $weight: 92%)}",
            "rgb(52.3450980392%, 83.3098039216%, 46.0392156863%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// 用本编译器未实现的空间写出的颜色不得在 sRGB 中作答。关键字表背后的解析器会乐意读
/// `color(display-p3 1 0 0)`；拒绝括号文本正是让这些行保持逐字而非错误值的东西
/// （`mix/predefined.hrx`）。
#[tokio::test]
async fn a_colour_in_an_unimplemented_space_is_not_answered_in_srgb() {
    let input = "a {b: color.mix(color(display-p3 1 0 0), color(display-p3 0 1 0), 20%)}";
    assert_eq!(
        css(input).await,
        "a {\n  b: color.mix(color(display-p3 1 0 0), color(display-p3 0 1 0), 20%);\n}\n"
    );
}

/// 注册表现在拒绝什么，以及它仍必须解析什么。
///
/// 命名被调用者签名中不存在参数的 `$label:` 不是本内建要回答的调用，所以行保留自身文本：
/// 用旧式 sRGB 混合回答 `color.mix(…, $method: oklab)` 会是一个通过数看不见的错误值，
/// 因为这些行本就因其他原因失败（`mix/explicit_method`、`named::polar_space`、
/// `hue_interpolation`）。已声明的标签继续工作，而**无**标签的拒绝调用仍解析其变量——
/// 这条规则几乎破坏的行（`saturate.hrx::css_overload/with_sass_var`）被钉在此处，
/// 使其不能靠再次猜测被“修复”。
#[tokio::test]
async fn an_unknown_parameter_label_is_a_refusal_not_a_legacy_answer() {
    for (input, want) in [
        (
            "a {b: color.mix(#91e16f, #0144bf, $method: oklab)}",
            "color.mix(#91e16f, #0144bf, $method: oklab)",
        ),
        (
            "a {b: color.mix(#91e16f, #0144bf, $space: hsl)}",
            "color.mix(#91e16f, #0144bf, $space: hsl)",
        ),
        // 带已声明标签的同一调用仍混合。
        (
            "a {b: color.mix($color1: #91e16f, $color2: #0144bf, $weight: 100%)}",
            "#91e16f",
        ),
        // 以及无标签的拒绝仍解析变量（`css_overload/with_sass_var`）。
        ("$c: 1;\na {b: saturate($c)}", "saturate(1)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
