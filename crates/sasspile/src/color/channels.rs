//! 每个色彩空间**自己的通道名**，以及**每个通道的百分比底数** —— 这两张表说出：一次调用的 `$red:` /
//! `$x:` / `$chroma:` 在*这个*空间里指什么，以及它的 `40%` 是什么意思。
//!
//! 从 `space` 里拆出的理由与 `primaries` 相同：这些是逐行有出处的表，而这里每一行都是一行
//! fixture，不是回忆。本文件不换算任何东西；`space` 向它问两个问题。
//!
//! 命名的那一半是 `color.change` 写东西所依据的：一个自带空间的颜色**用那个空间的名字**写，而语料对同样三个
//! 数字的拼法是：`change/srgb.hrx` 用 `$red/$green/$blue`、`change/xyz.hrx` 用 `$x/$y/$z`、`change/lab.hrx`
//! 用 `$lightness/$a/$b`，而 `change/lch.hrx` 用 `$lightness/$chroma/$hue`。
//!
//! 百分比的那一半是一条律从两个方向读，而且它是**逐通道**的、不是逐空间的：lab 的明度与它的 `a` 不共用底数。
//! 说出这一点的行就引在每个臂下面。

use super::Space;

/// 这个空间作答的三个通道名，按下标顺序。
///
/// 对 [`Space`] 穷举是刻意的：以后新加的空间必须在这里说出自己的名字，而不是承继一个默认值，去替一个从没人
/// 问过它的 `color.change` 调用作答。
pub(super) fn names(space: Space) -> [&'static str; 3] {
    match space {
        // 七个 RGB 形体的 `color()` 空间，编码与线性两种形式都有：
        // `change/{srgb,srgb_linear,display_p3,display_p3_linear,a98_rgb,prophoto_rgb,rec2020}.hrx`。
        Space::Srgb
        | Space::SrgbLinear
        | Space::DisplayP3
        | Space::DisplayP3Linear
        | Space::A98Rgb
        | Space::ProphotoRgb
        | Space::Rec2020 => ["red", "green", "blue"],
        // `change/{xyz,xyz_d50}.hrx`.
        Space::Xyz | Space::XyzD50 => ["x", "y", "z"],
        // `change/{lab,oklab}.hrx`.
        Space::Lab | Space::Oklab => ["lightness", "a", "b"],
        // `change/{lch,oklch}.hrx`.
        Space::Lch | Space::Oklch => ["lightness", "chroma", "hue"],
    }
}

/// `name` 在这个空间里的下标；若该空间没有叫这个名字的通道则为 `None` —— 那正是 `color(srgb …)` 上 `$whiteness:`
/// 背后的问题；`change/error/incompatible_channel.hrx::legacy_channel` 把这个情形说成一条这一刀尚未抛出的错误，
/// 所以那次调用保住自己的文本。
///
/// 比较忽略大小写，这是一个**今日无路径可演练的声明过的选型**：没有任何 fixture 用别的字母大小写写通道标签
/// （`grep -rln '\$RED\|\$Lightness\|\$CHROMA' sass-spec/spec/core_functions/color/` 为空），而唯一的调用方
/// 递给这里的是 [`names`] 里规范的小写名字，所以一个走到这里的标签已经被脊柱对 `$X:` 所做的那一步规范化过了
/// —— 这一点*不*在此断言。
pub(super) fn index(space: Space, name: &str) -> Option<usize> {
    names(space)
        .iter()
        .position(|own| own.eq_ignore_ascii_case(name))
}

/// 这个空间的一个通道是否就是它的**色相** —— 那个实参以角度抵达、并折进一圈的通道
/// （`change/lch.hrx::hue/{negative,above_max}` 把 `-20deg` 读成 `340deg`、`400deg` 读成 `40deg`），
/// 而不是当作一个普通数字。由 [`names`] 导出，好让“hue”这个词不在第二处被写一遍。
pub(super) fn is_hue(space: Space, channel: usize) -> bool {
    names(space).get(channel) == Some(&"hue")
}

/// 一个通道**怎么打印**：派生空间的明度带 `%`，它的色相带 `deg`，而其余每个通道 —— 矩形族与 XYZ 族的全部，
/// 加上 `lab`/`oklab` 的轴与 `lch`/`oklch` 的彩度 —— 是一个普通数字。问的是 [`names`] 而不是逐空间 `match`，
/// 所以打印机长不出一个表里没说的后缀（`to_space/srgb/lab.hrx::middle` 打 `lab(44.1210142293% …)`，而
/// `…/lch.hrx::middle` 打 `lch(44.1210142293% 60.0926108873 280.5032243387deg)`）。
pub(super) fn suffix(space: Space, channel: usize) -> &'static str {
    if space.is_functional() {
        return "";
    }
    match names(space).get(channel).copied() {
        Some("lightness") => "%",
        Some("hue") => "deg",
        // 派生族的轴与彩度，以及不在这三个之中的任何通道下标。
        _ => "",
    }
}

/// 一个通道上的百分比实参所读的那个除数。`None` 意思是归档对这个通道没说过百分比 —— 那两个色相就是
/// （`change/{lch,oklch}.hrx` 没有 `hue/percent` 用例；它们的 `hue/unitless`、`hue/negative` 与 `hue/above_max`
/// 用例都把实参写成角度，而 `builtins/color/amount.rs` 里的读法在它还根本没问这个问题之前，就用语料那句
/// 角度错误答了一个 `%` —— `change/error/units/lch.hrx::hue`）。一个越过三个通道的下标也不是一个通道。
///
/// **每个空间自己答这个问题** —— 对空间没有 `_ =>`，理由就是 [`names`] 说的那一条：以后新加的空间必须说出
/// 它各通道的底数，而不是悄悄承继 `100`（或 `None`）。明度那一列用的是 [`full`]，而不是把那个数再写一次，
/// 所以打印方向与读入方向分不开。
pub(super) fn percent_divisor(space: Space, channel: usize) -> Option<f64> {
    let lightness = full(space);
    let bases: [Option<f64>; 3] = match space {
        // `lab` 的轴跨 `±125`，所以 `a` 或 `b` 的 `-40%` 是 `-50`（`change/lab.hrx::{a,b}/percent`）。
        Space::Lab => [Some(lightness), Some(0.8), Some(0.8)],
        // `lch` 的彩度一路到 150，所以它的 `40%` 是 `60`（`change/lch.hrx::chroma/percent`）。
        Space::Lch => [Some(lightness), Some(2.0 / 3.0), None],
        // oklab 族的轴跨 `±0.4`：`a` 或 `b` 的 `-40%` 是 `-0.16`（`change/oklab.hrx::{a,b}/percent`），而
        // `oklch` 彩度的 `40%` 是 `0.16`（`change/oklch.hrx::chroma/percent`）。
        Space::Oklab => [Some(lightness), Some(250.0), Some(250.0)],
        Space::Oklch => [Some(lightness), Some(250.0), None],
        // 矩形族与 XYZ 族三个通道都是“一”的一部分：`40%` → `0.4`，在 `color(srgb …)`
        // （`change/srgb.hrx::green/percent`）、在 `color(display-p3-linear …)`
        // （`change/display_p3_linear.hrx::green/percent`）与在 `color(xyz …)`（`change/xyz.hrx::y/percent`）里都是。
        Space::Srgb
        | Space::SrgbLinear
        | Space::DisplayP3
        | Space::DisplayP3Linear
        | Space::A98Rgb
        | Space::ProphotoRgb
        | Space::Rec2020
        | Space::Xyz
        | Space::XyzD50 => [Some(lightness); 3],
    };
    bases.get(channel).copied().flatten()
}

/// **一个满幅百分比**在这个空间的明度上点的那个数字：`lab`/`lch` 把明度说在 `0–100` 尺度上，所以它们的 `50%`
/// *就是* `50`（`to_space/srgb/lab.hrx::middle` 读 `lab(44.1210142293% …)`，打出的就是它存着的那个数），
/// 而其余每个空间都是“一”的一部分，所以 `100%` 是 `1`（`to_space/oklab/srgb.hrx::white` 读 `oklab(100% 0 0)`
/// 要 `color(srgb 1 1 1)`；`to_space/srgb/oklab.hrx::white` 对白色要回 `oklab(100% 0 0)`）。
///
/// 这是**一条**律，两个方向都从它读：实参解析器除以它（在 `builtins/color/constructors.rs`），而
/// [`super::text::print`] 在写那个 `%` 之前乘它。在此之前它被写过三遍 —— 打印后缀、这个除数，以及矩阵里的
/// `0–1` 定义域 —— 三者在 `oklab`/`oklch` 上不一致，把 `oklab(100% 0 0)` 变成了 `L = 100`，把一个换算变成了
/// 量级 `1e5` 的数字。
///
/// 它就是 [`percent_divisor`] 的 `channel = 0` 那一列；其余通道在那边点名，因为归档给了它们不同的底数
/// （`lab` 的 `a` 不在它的明度所在的那个 `0–100` 尺度上）。
pub(crate) fn full(space: Space) -> f64 {
    // 逐空间写全而不是 `| _ => 100.0`：十三个空间里有十一个会被一个默认值答掉，而这正是本仓库已经拒过两次的
    // 形状（`unwrap_or(&BRADFORD)`、`Space::white` 的 `_ =>` 臂），理由就是那一条 —— 以后新加的空间会被这里
    // 答掉，而从来没人问过它。
    match space {
        Space::Lab | Space::Lch => 1.0,
        Space::Srgb
        | Space::SrgbLinear
        | Space::DisplayP3
        | Space::DisplayP3Linear
        | Space::A98Rgb
        | Space::ProphotoRgb
        | Space::Rec2020
        | Space::Xyz
        | Space::XyzD50
        | Space::Oklab
        | Space::Oklch => 100.0,
    }
}
