//! 本文件补上的缺口：**白度/黑度分支上移过一整圈的 hue**。
//!
//! 律不新，值也不新。三行把绕回的 hue 读作其余数 ——
//! `core_functions/color/adjust/hsl.hrx::hue/arg_above_max`、`core_functions/color/change/hsl.hrx::hue/above_max`
//! 与 `core_functions/color/adjust_hue/above_max.hrx`，全部用 `aqua` 回答 540 ——
//! 且 `tests/color_adjust.rs::the_hue_wraps_and_otherwise_prints_bytes` 已按值钉住它，连同
//! `adjust/hwb.hrx::all`（`color.adjust(red, $hue: 150, $whiteness: 20%, $blackness: 40%)` → `#339966`）。
//!
//! 没有任何一行练到的是 hwb 分支上*和*越过 360：移动 hue 并同动白度/黑度的行都从 `red`（hue 0）加 150 起，
//! 而在 `change` 下把 hue 与白黑度对并提的行从 `blue` 起但是**设置**（`change/hwb.hrx::all`），所以那里没有
//! 两个 hue 相加。缺失的这种形状正是该分支可能把越过一整圈的角度交给换算的地方。
//!
//! 所以本文件不引入任何颜色字面量 —— 绝对值住在 `color_adjust.rs`，在这里照抄它们会是同一定律的
//! 第二道守卫。它以语料留空的形状陈述这个已证的等价，并直说它能见与不能见：当两条分支对绕回
//! **不一致**时它转红。两条分支一起丢掉折叠是对称失败，任何相等断言都抓不到 —— 那一对由
//! `color_adjust.rs` 里的绝对值持有，这也是本文件有意不复制其中任何之一的原因。

use sasspile::compile;

async fn css(sass: &str) -> String {
    let built = compile(sass)
        .await
        .unwrap_or_else(|e| panic!("{sass} failed: {}", e.message));
    // 未被求值的调用会保留自己的源码文本，那样下面每个比较都会因错误的原因而一致。
    // 两种拼写都必须已产生一个颜色。
    assert!(
        !built.contains("color.adjust"),
        "the call was kept verbatim instead of evaluated: {built}"
    );
    built
}

/// `#abcdef` 的 hue 位于 210（`ab/cd/ef` = 171/205/239，其最大分量是 blue：`60 * ((171 - 205) / 68 + 4)`），
/// 所以 `$hue: 200` 到达 410。白度与黑度各移 `0%` 对它们不增分毫，所以下面两个调用只差在
/// **哪个分支写 hue** —— 第一个经白度/黑度分支，第二个经轴分支 —— 因而必须是同一个颜色。
#[tokio::test]
async fn a_hue_shift_past_a_turn_wraps_on_the_hwb_space_too() {
    let through_hwb =
        css("a {b: color.adjust(#abcdef, $hue: 200, $whiteness: 0%, $blackness: 0%)}").await;
    let through_axes = css("a {b: color.adjust(#abcdef, $hue: 200)}").await;
    assert_eq!(through_hwb, through_axes);
}
