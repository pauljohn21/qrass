//! 本文件守的律：**未写入的 `rgb` 通道是关于颜色本身的事实：随换算传递、被 modify 路径拒绝 ——
//! 输出时代入不得，凭空发明也不得。**
//!
//! [`color_missing_channels`](mod@crate::color::space) 已陈述该规则对写成 `color(…)` 的颜色是什么*形状*。
//! 这些行补充的是：**legacy** 三元组参与同一条规则，因为它*就是*个 srgb 三元组：`Color::new` 现在读
//! `none` 并存一个缺席通道（`color::Channel`），而不是拒绝这次调用、把整个表达式留成文本。归档对它的
//! 四条读数，各有各的行：
//!
//! * **同一性换算保留写下的内容。** `to_space/rgb/rgb.hrx::missing/red` 要
//!   `color.to-space(rgb(none 20 30), rgb)` 并答 `rgb(none 20 30)` —— 用空格分隔形式，因为 legacy 逗号
//!   形式没有 `none` 的拼法。同一调用打在*空间*颜色上（`to_space/srgb/rgb.hrx::missing/red`）答
//!   `rgb(0%, 20%, 30%)`：那里 `none` 属于空间三元组，legacy 目标代入。两行合起来解释为何守卫按
//!   表示（representation）而非仅按「缺」判定。
//! * **在索引对齐族内它留在自己的索引上** —— `rgb(10 none 30)` 得
//!   `color(xyz 0.0035949053 none 0.0123994639)`，另两个通道正是 `0` 产生的数，于是算术答案与打印答案
//!   是两个问题（`color::missing_as_zero` 是唯一陈述前者之处）。
//! * **派生目标代入**：`rgb(none 20 30)` 到 `hsl` 是 `hsl(200, 100%, 5.8823529412%)`
//!   （`to_space/rgb/hsl.hrx::missing/red`），而全缺对空间 powerless（`to_space/rgb/lab.hrx::missing/all`
//!   得 `lab(none none none)`）。
//! * **改未写的通道被拒绝、不来答案。** 语料把它陈述为一个错误——
//!   `$red: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently
//!   support modifying missing channels (color: rgb(none 0 0))`（`adjust/error/missing.hrx::legacy`；`scale`
//!   在 `scale/error/missing.hrx::legacy` 重复）。缺少 `Color::with_channel` 的守卫时，本编译器曾把该
//!   调用答成 `rgb(10, 0, 0)`（实测：`reports/hrx.db` 的 run `legacy-missing-channel`，行
//!   `core_functions/color/adjust/error/missing::legacy`）—— 一个归档禁止的值；那比这批接受的静默更糟。
//!
//! 本文件**故意**不钉的，每次都附实测理由：
//!
//! * **目标为 `lab`/`lch`/`oklab`/`oklch`/`display-p3` 的十三条 `rgb(none …)` 行。** 它们从「根本无
//!   答案」变成「答案差第九或第十位小数」——`to_space/rgb/lab.hrx::missing/red` 要 `5.2408778377%`，
//!   本实现印 `5.2408778375%`。该残量是 `tasks.md` 9.14 记录的未校准锥/primaries 表，不是这批的事：
//!   下方最后一个测试断言代入后的答案与同颜色显式写 `0` 得到的答案**完全相同**，故凡有偏差都在共用
//!   路线里。钉住自己的数字会把一个已知错的数冻住。
//! * **要命名颜色的行**——归档答裸 `black`/`white`/`red`，本编译器印 `#000000`；该簇住在**目标**为
//!   `rgb` 的文件里（`to_space/{hwb,lch,oklab,oklch,lab,…}/rgb.hrx`）。号码两种口径都数过、好重现：
//!   语料侧 `grep -rh "^  b: [a-z]*;$" sass-spec/spec/core_functions/color/to_space/ | wc -l`，库侧
//!   `ledger failures … "b: black;" --count` 等——两者都记于 `tasks.md` 9.23。在此钉住任何一条都要
//!   「换算后仍答名字」的律，而它未实现。
//! * **带 `none` 通道的 `hsl`/`hwb`。** 它们的构造子仍拒绝，所以 `hsl(none 20% 30%)` 仍是文本；等待中的
//!   行点名在 `tasks.md` 9.23（且读它们需要 `hsl`/`hwb` 空间与坐标算术，不只是解析）。把「缺」写进
//!   `hsl` 或 `hwb` 的**轴**是同一等待的另一侧——`change/hsl.hrx::hue/none` 要 `hsl(none 100% 50%)`、
//!   `change/hwb.hrx::whiteness/none` 要 `hwb(180deg none 50%)`，于是坐标三元组也得携缺，而
//!   `Color::with_hwb`（目前重新推导字节）必须留住这一对。`tasks.md` 9.25 有剩余 `change/*/none` 行
//!   的实测划分。
//! * **带 alpha 的 `none` 通道**（`rgb(0 0 0 / none)`）**或越域的同类**——没有 fixture 印出两者之一，所以
//!   `Color::modern` 的 `/ alpha` 一半是推广（仅由 `change/no_space.hrx::alpha/none` 旁证，它不写的是
//!   *alpha*），记录为一条推广，不断言。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// 同一性换算保留未写的位置，并打印空格分隔形式。
#[tokio::test]
async fn an_identity_conversion_keeps_the_unwritten_channel() {
    let cases = [
        ("rgb(none 20 30)", "rgb(none 20 30)"),
        ("rgb(10 none 30)", "rgb(10 none 30)"),
        ("rgb(10 20 none)", "rgb(10 20 none)"),
    ];
    for (source, printed) in cases {
        let built = css(&format!("a {{b: color.to-space({source}, rgb)}}")).await;
        assert!(
            built.contains(printed),
            "`to_space/rgb/rgb.hrx` keeps the unwritten channel: `{source}` must print `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 同一个 `none` 按位置传入索引对齐族，周围通道是 `0` 产生的数——这正是「算术代入了 `0`、打印没有」
/// 的判据。
#[tokio::test]
async fn a_missing_channel_propagates_by_position_from_a_legacy_triple() {
    let cases = [
        (
            "rgb(none 20 30)",
            "srgb",
            "color(srgb none 0.0784313725 0.1176470588)",
        ),
        (
            "rgb(10 20 none)",
            "srgb-linear",
            "color(srgb-linear 0.0030352698 0.0069954102 none)",
        ),
        (
            "rgb(10 none 30)",
            "xyz",
            "color(xyz 0.0035949053 none 0.0123994639)",
        ),
    ];
    for (source, target, printed) in cases {
        let built = css(&format!("a {{b: color.to-space({source}, {target})}}")).await;
        assert!(
            built.contains(printed),
            "`{source}` to `{target}`: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 派生目标不接受单个未写通道的 `none`，而全然无写的颜色对同一空间 powerless。
#[tokio::test]
async fn a_derived_target_substitutes_unless_nothing_was_written() {
    let built = css("a {b: color.to-space(rgb(none 20 30), hsl)}").await;
    assert!(
        built.contains("hsl(200, 100%, 5.8823529412%)"),
        "`to_space/rgb/hsl.hrx::missing/red` substitutes the unwritten red; this compiler answered:\n{built}"
    );
    let all = css("a {b: color.to-space(rgb(none none none), lab)}").await;
    assert!(
        all.contains("lab(none none none)"),
        "`to_space/rgb/lab.hrx::missing/all` is powerless into `lab`, same as a space colour; got:\n{all}"
    );
}

/// 替代「发明答案」的拒绝：`adjust` 落在未写通道上不得以数字作答。阳性对照是头部那次实测——同一调用
/// 曾有一批打印出 `rgb(10, 0, 0)`，而归档把该行为直接判为不支持。
#[tokio::test]
async fn modifying_an_unwritten_channel_is_declined_not_invented() {
    let adjusted = css("a {b: color.adjust(rgb(none 0 0), $red: 10)}").await;
    assert!(
        !adjusted.contains("rgb(10, 0, 0)"),
        "the archive errors on this call (`adjust/error/missing.hrx::legacy`), so no answer is allowed to be \
         computed for the unwritten red; this compiler answered:\n{adjusted}"
    );
    // 拒绝以调用点名的索引为键，而不是「该颜色任意处有 `none`」；这种窄是 fixture 的要求、不是我的偏好
    // ——见下面的测试。
}

/// **未写兄弟的颜色，其已写通道仍必须可改。** 语料从 `color.change(rgb(none 0 none), $green: 300)` 造出
/// `to_gamut/space.hrx::clip/missing/legacy/same/with_mapping` 的答案 `rgb(none 255 none)`：green 是已写
/// 通道，两个兄弟是 `none`，最终行保持 `none`。以「颜色任意处有 `none`」为键的拒绝绝产不出那一行，所以
/// 此断言该内层调用根本被作答。（该行本身在 run `legacy-missing-channel-v3` 仍红——`color.to-gamut` 尚不携
/// `none`，`tasks.md` 9.23——而内层调用*印出*什么是归档不单独陈述的文本，故本文件不钉一种。）
#[tokio::test]
async fn a_written_channel_of_a_colour_with_unwritten_siblings_is_still_modified() {
    let built = css("a {b: color.change(rgb(none 0 none), $green: 300)}").await;
    assert!(
        !built.contains("color.change("),
        "`to_gamut/space.hrx::clip/missing/legacy/same/with_mapping` needs this inner call to be answered, not \
         echoed; this compiler returned its text:\n{built}"
    );
}

/// **`change` 能写「缺」。** `color.change(black, $red: none)` 答 `rgb(none 0 0)`，green 与 blue 同理
/// （`change/rgb.hrx::{red,green,blue}/none`）；从关键字创建的颜色接受同一写入并停止打印关键字，因为
/// 形状必须改变才能说 `none`（`change/space.hrx::missing/arg/legacy/same/implicit` 是
/// `color.change(pink, $red: none)` → `rgb(none 192 203)`）。`adjust` 与 `scale` 不在此律：其读取器
/// `amount::byte` 直接拒绝 `none`，而把它们当参数的行只期望一条类型错误——`color.adjust(red, $alpha: none)`
/// → `$alpha: none is not a number.`（`adjust/error/type.hrx::none`；`scale/error/type.hrx::none` 同理），
/// 故没有 fixture 要求 shift 或 scale 移动一个「缺」。
#[tokio::test]
async fn change_can_write_absence_into_a_byte_channel() {
    let cases = [
        ("black", "$red: none", "rgb(none 0 0)"),
        ("black", "$green: none", "rgb(0 none 0)"),
        ("black", "$blue: none", "rgb(0 0 none)"),
        ("pink", "$red: none", "rgb(none 192 203)"),
    ];
    for (colour, write, printed) in cases {
        let built = css(&format!("a {{b: color.change({colour}, {write})}}")).await;
        assert!(
            built.contains(printed),
            "`color.change({colour}, {write})`: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 为什么那十三条未修的行不是这批的缺陷：进入派生空间的路线上未写通道被代入，故 `rgb(none 20 30)` 与
/// `rgb(0 20 30)` 必须答得**完全相同**。那十三条里凡有不准，都只是 `tasks.md` 9.14 记录的共用
/// 锥/primaries 残量，不是这批开的第二套算术——而断言在两种拼法漂移的那天就会失败。
#[tokio::test]
async fn a_substituted_channel_leaves_the_route_unchanged() {
    for target in ["lab", "lch", "oklab", "oklch"] {
        let missing = css(&format!(
            "a {{b: color.to-space(rgb(none 20 30), {target})}}"
        ))
        .await;
        let written = css(&format!("a {{b: color.to-space(rgb(0 20 30), {target})}}")).await;
        let value = |text: &str| {
            text.lines()
                .find(|line| line.contains("b:"))
                .unwrap_or(text)
                .trim()
                .to_string()
        };
        assert_eq!(
            value(&missing),
            value(&written),
            "into `{target}` the archive substitutes the unwritten red, so the two spells must not diverge"
        );
    }
}
