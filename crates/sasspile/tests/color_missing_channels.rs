//! 本文件守的律：**`none` 通道经 `color.to-space` 后留在写下的位置，但只在索引对齐族内。**
//!
//! 归档把两种行为分开，各用自己的行陈述：
//!
//! * **在通道是索引对齐的 RGB 或 XYZ 三元组的族内**（`srgb`、`srgb-linear`、`display-p3`、
//!   `display-p3-linear`、`a98-rgb`、`prophoto-rgb`、`rec2020`、`xyz`、`xyz-d50`），缺的通道留在
//!   **同一索引**、另两个照常算——这也证明缺通道仍给算术贡献 `0`：`to_space/srgb/xyz.hrx::missing/red`
//!   答 `color(xyz none 0.028962782 0.0735618977)`，那两个数正是 red 取 `0` 的产物。
//! * **其余地方缺通道被代入**，三元组变成三个数。两个方向由 `Color::converted` 里同一条件覆盖（一侧
//!   派生 ⇒ 不传播）；绿证人在派生**源**方向：`to_space/lab/srgb.hrx::missing/lightness` →
//!   `color(srgb 0.1653748669 -0.0646023407 -0.1599982587)`。派生**目标**方向由归档同一批行陈述，但仍在
//!   第十位红（`to_space/srgb/lab.hrx::missing/red` 要 `-7.7361660103`，本实现印 `-7.7361660069`）——
//!   那是 `tasks.md` 9.14 记录的天花板；这批没挣到、也不声称。
//! * **三缺则三缺**进入空间：`to_space/srgb/lab.hrx::missing/all` → `lab(none none none)`、
//!   `to_space/srgb/oklab.hrx::missing/all` → `oklab(none none none)`。`hsl` 目标是归档用自己行陈述的
//!   例外——`to_space/srgb/hsl.hrx::missing/all` → `hsl(0, 0%, 0%)`，零，因为该形式没法印 `none`。
//!   （`hwb` 是另一个未决问题：`to_space/hwb/hwb.hrx::missing/non_hue` *确实*印 `hwb(10deg none none)`，
//!   且那文件不用 mixin，而 `color.to-space` 干脆拒绝 `hwb` 目标——点名于 `tasks.md` 9.15。）
//!
//! 没有 fixture 陈述「全缺的**矩形**颜色换算到另一矩形空间」是什么，故本文件不断言：位置规则覆盖它
//! （`none` 留在自己索引），但那是我的读法，不是归档印出的一行。
//!
//! 本文件**故意**不钉的，每次都附实测理由：
//!
//! * **polar 对（`lab`↔`lch`、`oklab`↔`oklch`）未实现。** 它的 22 条 `missing/*` 行（`lab/lch` 5、
//!   `lch/lab` 6、`oklab/oklch` 5、`oklch/oklab` 6）中 20 条合一条候选律——按位置携带明度，且仅当派生
//!   通道所依赖的通道**全**缺时它才 `none`（`lab(10% none 30)` → `lch(10% 30 90deg)`、
//!   `lab(10% none none)` → `lch(10% none none)`、`lch(10% none 30deg)` → `lab(10% 0 0)`、
//!   `oklch(none none 10deg)` → `oklab(none 0 0)`）。两行破律，都在 `lch` → `lab`：
//!   `to_space/lch/lab.hrx::missing/lightness`（`lch(none 20 30deg)` → `lab(none none none)`）与
//!   `…::missing/non_hue`（`lch(none none 10deg)` → `lab(none none none)`），而同形经 `oklch` →
//!   `oklab` 合律。所以破的是 `lab` 自己，不是对代数——而把律写成合 20 行会悄悄发明那两行，故此处不对
//!   该族声称规则。
//! * `to_space/xyz_d50/srgb.hrx::missing/y` —— `none` 现在对、差一位数（`0.6863615753` 对归档
//!   `0.6863615754`），`tasks.md` 9.14 的第十位小数天花板。
//! * `to_space/xyz/xyz_d50.hrx::gray` —— 同一天花板，在库所派生的那一侧（`0.5014942182` 对本实现
//!   `0.5014942183`）。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// 索引对齐族：`none` 留在自己索引，其余通道保留各自数字。
#[tokio::test]
async fn a_missing_channel_stays_missing_at_its_own_index() {
    let cases = [
        (
            "color(srgb none 0.2 0.3)",
            "xyz",
            "color(xyz none 0.028962782 0.0735618977)",
        ),
        (
            "color(srgb 0.1 0.2 none)",
            "xyz",
            "color(xyz 0.0159710671 0.0258067358 none)",
        ),
        (
            "color(xyz none 0.2 0.3)",
            "srgb",
            "color(srgb none 0.6558430379 0.5622938017)",
        ),
        (
            "color(display-p3-linear 0.1 0.2 none)",
            "display-p3",
            "color(display-p3 0.3491902126 0.4845292045 none)",
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

/// 派生目标不接受单个缺通道的 `none`——同一颜色去 `xyz` 保留 `none`，去 `srgb` 变成三个数。
#[tokio::test]
async fn a_derived_target_substitutes_the_missing_channel() {
    let built = css("a {b: color.to-space(lab(none 20 30), srgb)}").await;
    assert!(
        built.contains("color(srgb 0.1653748669 -0.0646023407 -0.1599982587)"),
        "`to_space/lab/srgb.hrx::missing/lightness` substitutes `0` for the lightness; this compiler answered:\n{built}"
    );
}

/// 早于这批的规则继续成立：全然无写的颜色进空间时 powerless；而 legacy 目标——无法打印 `none`——代入零。
#[tokio::test]
async fn a_colour_with_no_channels_at_all_is_powerless_or_substituted_by_target_form() {
    let built = css("a {b: color.to-space(color(srgb none none none), lab)}").await;
    assert!(
        built.contains("lab(none none none)"),
        "`to_space/srgb/lab.hrx::missing/all` is `lab(none none none)`; this compiler answered:\n{built}"
    );
    let hsl = css("a {b: color.to-space(color(srgb none none none), hsl)}").await;
    assert!(
        hsl.contains("hsl(0, 0%, 0%)"),
        "`to_space/srgb/hsl.hrx::missing/all` substitutes — the legacy form cannot print `none`; got:\n{hsl}"
    );
    let oklab = css("a {b: color.to-space(color(srgb none none none), oklab)}").await;
    assert!(
        oklab.contains("oklab(none none none)"),
        "`to_space/srgb/oklab.hrx::missing/all` keeps all three; this compiler answered:\n{oklab}"
    );
}
