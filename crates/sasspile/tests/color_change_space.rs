//! 本文件守的律：**带自己空间的颜色经该空间自己的词汇、按该空间自己的通道写入**——
//! `color.change(color(srgb …), $red:)`、`change/xyz.hrx` 经 `$x` 写同一位置、`change/lab.hrx` 经
//! `$a`、`change/lch.hrx` 经 `$chroma`/`$hue`。
//!
//! 此前根本没有这条路径：每个 `change/<space>.hrx` 文件一度整件红（实测：`change/srgb.hrx` 有 15 个
//! case，在 run `change-writes-absence-v4` 里 15 个全红）。归档同时钉死四件事，每件都是一行而不是
//! 一条惯例：
//!
//! * **名字属于空间** —— `change/{xyz,lab,lch}.hrx` 从不拼出 `red/green/blue`，而
//!   `change/{srgb,display_p3,a98_rgb,prophoto_rgb,rec2020,…}` 从不拼出 `x/y/z`。
//! * **百分比逐通道读，而不是逐空间读。** `change/lab.hrx::lightness/percent`（`30%` → `30`）与
//!   `change/lab.hrx::a/percent`（`-40%` → `-50`）在同一个颜色里是同*一种*参数、两个不同的底数，
//!   oklab 族把自己的轴放在 `±0.4`（`change/oklab.hrx::a/percent`、
//!   `change/oklch.hrx::chroma/percent`），`lch` 把自己的 chroma 放在 150
//!   （`change/lch.hrx::chroma/percent`）。
//! * **hue 是角度，负 chroma 把 hue 翻转半圈** —— `change/lch.hrx::hue/*` 把 `-20deg` 读作
//!   `340deg`、`400deg` 读作 `40deg`，而 `…::chroma/negative` 以
//!   `color.change(lch(50% 20 30deg), $chroma: -10)` 答 `lch(50% 10 210deg)`；`oklch` 各行一致，故
//!   一次折叠服务这一对。没有别的通道折叠：`change/{lab,oklab}.hrx::a/out_of_range` 保住负值，
//!   且没有夹紧——`change/xyz.hrx::x/out_of_range` 保住写下的 `1.2`。
//! * **既不是 `%` 也不是无单位的单位按名拒答**，且 hue 干脆拒 `%`：
//!   `change/error/units/srgb.hrx::red` → `$red: Expected 0.5px to have unit "%" or no units.`，
//!   `change/error/units/lch.hrx::hue` → `$hue: Expected 20% to have an angle unit (deg, grad, rad, turn).`。
//!   把 `0.5px` 读成量级 `0.5`——这一臂的第一个版本就是这么做的——会静默答出归档称为错误的 27 行，
//!   那正是那两条断言存在的意义。
//!
//! 本文件**故意**不钉的，每次都附实测理由：
//!
//! * **四行 `change/{lab,lch,oklab,oklch}.hrx::lightness/out_of_range`。** 对
//!   `color.change(lab(50% 20 -30), $lightness: 120%)`，归档印
//!   `color-mix(in lab, color(xyz 1.7255148283 1.6190494947 2.5266428703) 100%, black)`——一个
//!   `color-mix` *表达式*，本编译器根本发不出（在 `.qoder/rules/rust.md` 里记为一个独立能力缺口）。
//!   我们现在印 `lab(120% 20 -30)` 而不回显调用；该行仍红，而归档在两种拼法之间划的界是这些
//!   fixture 无法定夺的，故此处不对它声称什么。
//! * **六行 `*/lightness/none` 与 `{lch,oklch}::hue/none`。** `$lightness` 与 `$hue` 也是 *legacy* 轴的
//!   名字，而 `route` 对那些轴的预读（正是它发出语料的轴消息）在本臂到达之前就拒绝了 `none`。把
//!   缺写进 `hsl`/`hwb` 轴三元组是同一件未决项（`tasks.md` 9.25）；在它存在之前，这六行保留文本。
//! * **自带空间上的 `adjust` 与 `scale`** —— 故意不接线，因为它们的算术（`scale_toward` 朝哪个界？
//!   `share` 以什么为底？）是这些行没有陈述的第二套律。它们的文件保持红，点名在 `tasks.md` 9.27。
//! * **颜色空间不拥有的名字**保留文本，而不是答
//!   `Color space srgb doesn't have a channel with this name.`——在*空间*颜色上要求这条消息的行是
//!   `change/error/incompatible_channel.hrx::{legacy_channel,modern_both}`——`color.change(color(srgb 0.2 0.5 0.7),
//!   $whiteness: 50%)` 与 `…, $chroma: 50%`——三者都在 run `change-in-carried-space-v5` 里红，与
//!   基线同。`change/error/mixed_formats.hrx`——为 **legacy** 组携带那条消息的文件——不含任何空间色
//!   行（`grep -c "color(srgb\|lab(\|lch(" → 0`），所以这不是那份文件的律靠猜复用。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

async fn failure(scss: &str) -> String {
    match compile(scss).await {
        Ok(text) => panic!("expected {scss:?} to fail, but it compiled to:\n{text}"),
        Err(error) => error.to_string(),
    }
}

/// 每个空间经自己的名字写入、不夹紧，且 `none` 仍是 `none`。
#[tokio::test]
async fn a_carried_space_is_written_through_its_own_channels() {
    let cases = [
        (
            "a {b: color.change(color(srgb 0.2 0.5 0.7), $red: 0.5)}",
            "color(srgb 0.5 0.5 0.7)",
        ),
        (
            "a {b: color.change(color(srgb 0.2 0.5 0.7), $blue: none)}",
            "color(srgb 0.2 0.5 none)",
        ),
        (
            "a {b: color.change(color(srgb 0.2 0.5 0.7), $red: 0.7, $green: 0.4, $blue: 0.2)}",
            "color(srgb 0.7 0.4 0.2)",
        ),
        (
            "a {b: color.change(color(xyz 0.2 0.5 0.7), $x: 1.2)}",
            "color(xyz 1.2 0.5 0.7)",
        ),
        (
            "a {b: color.change(oklab(50% 0.2 -0.3), $a: 1)}",
            "oklab(50% 1 -0.3)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $chroma: none)}",
            "lch(50% none 30deg)",
        ),
        (
            "a {b: color.change(color(xyz-d50 0.2 0.5 0.7), $z: none)}",
            "color(xyz-d50 0.2 0.5 none)",
        ),
        (
            "a {b: color.change(color(rec2020 0.2 0.5 0.7), $red: 0.5, $alpha: 0.9)}",
            "color(rec2020 0.5 0.5 0.7 / 0.9)",
        ),
    ];
    for (input, printed) in cases {
        let built = css(input).await;
        assert!(
            built.contains(printed),
            "{input}: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 百分比底数是**一个通道**的属性，而一个空间持有两个：`lab` 的明度在 `0–100`、其 `a` 在 `±125`，
/// oklab 族读 `±0.4`。
#[tokio::test]
async fn a_percentage_is_read_per_channel_and_not_per_space() {
    let cases = [
        (
            "a {b: color.change(lab(50% 20 -30), $lightness: 30%)}",
            "lab(30% 20 -30)",
        ),
        (
            "a {b: color.change(lab(50% 20 -30), $a: -40%)}",
            "lab(50% -50 -30)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $chroma: 40%)}",
            "lch(50% 60 30deg)",
        ),
        (
            "a {b: color.change(oklab(50% 0.2 -0.3), $a: -40%)}",
            "oklab(50% -0.16 -0.3)",
        ),
        (
            "a {b: color.change(oklch(50% 0.2 30deg), $chroma: 40%)}",
            "oklch(50% 0.16 30deg)",
        ),
        (
            "a {b: color.change(color(display-p3-linear 0.2 0.5 0.7), $green: 40%)}",
            "color(display-p3-linear 0.2 0.4 0.7)",
        ),
    ];
    for (input, printed) in cases {
        let built = css(input).await;
        assert!(
            built.contains(printed),
            "{input}: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// hue 以角度到达并折进一圈；**负 chroma** 把 hue 带转半圈，在两个 polar 空间都一样。
#[tokio::test]
async fn an_angle_folds_and_a_negative_chroma_turns_the_hue() {
    let cases = [
        (
            "a {b: color.change(lch(50% 20 30deg), $hue: 400deg)}",
            "lch(50% 20 40deg)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $hue: -20deg)}",
            "lch(50% 20 340deg)",
        ),
        (
            "a {b: color.change(lch(50% 20 30deg), $chroma: -10)}",
            "lch(50% 10 210deg)",
        ),
        (
            "a {b: color.change(oklch(50% 0.2 30deg), $chroma: -0.1)}",
            "oklch(50% 0.1 210deg)",
        ),
        (
            "a {b: color.change(oklch(50% 0.2 30deg), $hue: 400deg)}",
            "oklch(50% 0.2 40deg)",
        ),
    ];
    for (input, printed) in cases {
        let built = css(input).await;
        assert!(
            built.contains(printed),
            "{input}: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
    // …且没有别的通道折叠：`lab` 与 `oklab` 的负轴保持负值。
    let stays = css("a {b: color.change(lab(50% 20 -30), $b: -200)}").await;
    assert!(
        stays.contains("lab(50% 20 -200)"),
        "`change/lab.hrx::b/out_of_range` keeps the negative it was given; got:\n{stays}"
    );
}

/// 归档为这些调用陈述的两种单位拒答。没有它们，写入会把 `0.5px` 答成 `0.5`——那是 27 行称为
/// 错误的值。
#[tokio::test]
async fn a_wrong_unit_is_refused_by_name() {
    assert_eq!(
        failure("a {b: color.change(color(srgb 0.2 0.5 0.7), $red: 0.5px)}").await,
        "$red: Expected 0.5px to have unit \"%\" or no units."
    );
    assert_eq!(
        failure("a {b: color.change(lch(50% 30 50deg), $hue: 20%)}").await,
        "$hue: Expected 20% to have an angle unit (deg, grad, rad, turn)."
    );
    assert_eq!(
        failure("a {b: color.change(lab(50% 30 -50), $a: 20px)}").await,
        "$a: Expected 20px to have unit \"%\" or no units."
    );
}

/// 为什么移动了的 44 行嵌套 `to_space/{lab,lch,oklab,oklch}/*` 不是第二套算术：写一个明度与在字面量
/// 里*陈述*同一个数必须换算出**同一文本**。那些行无论差什么，都是共用 oklab/lab 路线的残量
/// （`tasks.md` 9.14），而这断言会在两种拼法漂移的那天失败。
#[tokio::test]
async fn a_written_channel_and_the_same_number_written_out_convert_alike() {
    for (written, literal, target) in [
        (
            "color.change(oklab(0% -2 2), $lightness: -50%)",
            "oklab(-50% -2 2)",
            "srgb",
        ),
        (
            "color.change(oklch(30% 0.1 40deg), $hue: 400deg)",
            "oklch(30% 0.1 40deg)",
            "srgb",
        ),
        (
            "color.change(lab(50% 20 -30), $a: -30)",
            "lab(50% -30 -30)",
            "xyz",
        ),
    ] {
        let changed = css(&format!("a {{b: color.to-space({written}, {target})}}")).await;
        let literal = css(&format!("a {{b: color.to-space({literal}, {target})}}")).await;
        let value = |text: &str| {
            text.lines()
                .find(|line| line.contains("b:"))
                .unwrap_or(text)
                .trim()
                .to_string()
        };
        assert_eq!(
            value(&changed),
            value(&literal),
            "`{written}` and the colour `{literal}` name the same channels, so `{target}` cannot answer them \
             differently"
        );
    }
}
