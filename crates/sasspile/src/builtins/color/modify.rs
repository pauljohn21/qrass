//! `color.change`、`color.adjust` 与 `color.scale` —— **重写这次调用点名的那些通道**，其余保持这个颜色原有的。
//!
//! 正是这些函数体需要登记表的标签：`color.change(black, $red: 500)` 与 `color.change(black, $green: -50)`
//! 各传一个数，语料把它们打成 `hsl(0, 5000%, 98.0392156863%)` 和 `hsl(120, 100%, -9.8039215686%)`
//! （`change/rgb.hrx::{red,green}/out_of_range`）。没有参数名，这两个调用到函数体看见时已经无从区分，
//! 这就是 [`Given`] 存在的原因。
//!
//! 一个 [`route`] 服务这三个，按 [`Writing`] 分流——置值、按偏移移动、或朝界限移动——并按 [`Space`] 分流
//! （字节、hsl 轴、hwb 轴）。每一个这样的分叉都是一条语料律的住所，所以代码说出是哪一行定的它，而一个本族
//! 用不上的实参用归档自己的消息作答，不是沉默。各自目录
//! `core_functions/color/{change,adjust,scale}/error/` 下的文件并不齐（现量：change 有 args/bounds/incompatible_channel/
//! mixed_formats/space/type/units，adjust 有 args/incompatible_channel/missing/mixed_formats/space/type/units，
//! scale 另有 bounds 与 polar），所以不能写成一个叉乘。

use crate::SassError;
use crate::builtins::{Builtin, Given};
use crate::value::Value;

/// `color.change(black, $red: 255)` → `red`、`color.change(red, $red: 100)` → `#640000`、
/// `color.change(black, $green: 255)` → `lime` 与 `color.change(black, $red: 500)` →
/// `hsl(0, 5000%, 98.0392156863%)` —— 出自 `core_functions/color/change/rgb.hrx` 的四行。前三行保住这个颜色
/// 自己的打印（`Representation::Named` 在字节仍能点到一个名字时写关键词，否则写 hex）；第四行是
/// [`Color::to_css`] 的 out-of-gamut 规则。
///
/// 声明成**带名字的 variadic**，好让 `$red:` 能绑定，而一个不认识的标签仍按 unknown decline；由函数体判
/// 哪些通道被点了名——这正是它认标签的全部理由。
pub(super) static COLOR_CHANGE: Builtin = Builtin {
    name: "color.change",
    params: &["color"],
    optional: &[
        "red",
        "green",
        "blue",
        "alpha",
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
        "space",
        // Color-4 各空间用来自称通道的名字：`$x/$y/$z`（`change/xyz.hrx`）、`$a/$b`（`change/lab.hrx`、
        // `change/oklab.hrx`）与 `$chroma`（`change/lch.hrx`、`change/oklch.hrx`）。一个没在这里声明的标签
        // 根本到不了函数体——那次调用按未知标签 decline——所以 `change/xyz.hrx` 的 15 行在 `color.change`
        // 学会往一个自带空间里写之前全是红的，尽管对 `$red` 而言那次写本身早就正确。
        "x",
        "y",
        "z",
        "a",
        "b",
        "chroma",
    ],
    variadic: true,
    // 这次调用的每一种读法都需要标签，所以脊柱走 [`labelled`]；这个字段之所以存在，只是因为一个签名必须说出
    // 它怎么读自己的实参，而靠位置去猜对 `color.change` 是错的答案。
    apply: needs_labels,
    labelled: Some(changed),
};

/// 这些内置函数刻意没有的那种按位置的读法。
fn needs_labels(args: Vec<Value>) -> Result<Value, SassError> {
    let _ = args;
    Err(SassError::new(
        "this colour call reads its arguments by parameter",
    ))
}

/// `color.adjust(black, $red: 255)` → `red` 与 `color.adjust(#abcdef, $red: 200)` → `#ffcdef`
/// （`core_functions/color/adjust/rgb.hrx::{red/max,red/above_max}`）：与 [`COLOR_CHANGE`] 同一套路由，只是
/// 每个被点名的通道按实参**移动**而不是置成它。字节过不去两端——`color.change` 会留下一个越界通道并打 hsl，
/// 而 `adjust` 夹紧到 `0–255`。
///
/// 轴在各空间遵循*不同*的律，每一条都由行说出：
/// * hwb 先相加，之后才归一化溢出的那一对：`#cc6666` 上的 `$whiteness: 100%` → `rgb(87.5%, 87.5%, 87.5%)`
///   （`adjust/hwb.hrx::whiteness/max`），而 `$whiteness: -100%` 留下 `hsl(0, 700%, 10%)`——out of gamut，
///   按写下的轴打印（`…::whiteness/min`）；
/// * hsl **不**走 legacy 的夹紧：`color.adjust(plum, $saturation: 53%)` 保住 100.2868217054% 并打 hsl
///   （`adjust/hsl.hrx::saturation/max_remaining`），而 legacy 的 `saturate(plum, 53%)` 在 100 处夹紧并打
///   `#ff7eff`（`saturate.hrx::two_args/max_remaining`）——所以 [`Color::transform_axes`] 继续作为 legacy
///   入口，这里不用它。只有饱和度的**下**界会夹（`…::saturation/{min,min_remaining}` 要的是零饱和度给出的
///   那个灰），而 `$lightness: -100%` 保持负值并打 `hsl(0, 100%, -50%)`（`…::lightness/min`）；
/// * 一次不点名任何通道的调用是 no-op，而单有 `$alpha:` 仍会移动 alpha——夹紧到 `0–1`
///   （`adjust/no_channels.hrx::{positional,named}` 与它的 `alpha/*` 那一组，其中 `rgba(red, 0.5)` 加 `1`
///   又打出 `red`）。`change` 一侧同样有行（`change/no_space.hrx::{positional,named}` 与 `…::alpha/{max,min}`，
///   现量 12 行全红），所以 `change` 在那里 decline 是一处未闭的缺口，不是“语料没说”。
pub(super) static COLOR_ADJUST: Builtin = Builtin {
    name: "color.adjust",
    params: &["color"],
    optional: &[
        "red",
        "green",
        "blue",
        "alpha",
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
        "space",
    ],
    variadic: true,
    apply: needs_labels,
    labelled: Some(adjusted),
};

/// 一次调用是**替换**一个通道（`color.change`）、按偏移**移动**它（`color.adjust`），还是按还剩多少距离的
/// 那个份额**朝它的界限移动**它（`color.scale`）。
///
/// 用一个类型而不是三个标志位，用一个路由函数而不是它的三份副本：这三者只在值上的算术与夹紧落在哪里这两点上
/// 不同，而一条律写两遍正是本周 `fractional()` 与 gamut 测试各自和语料不一致的来路。
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Writing {
    Set,
    Shift,
    Scale,
}

/// 一次 `color.change` 调用。
fn changed(given: Vec<Given>) -> Result<Value, SassError> {
    route(given, Writing::Set)
}

/// 一次 `color.adjust` 调用。
fn adjusted(given: Vec<Given>) -> Result<Value, SassError> {
    route(given, Writing::Shift)
}

/// 一次 `color.scale` 调用。
fn scaled(given: Vec<Given>) -> Result<Value, SassError> {
    route(given, Writing::Scale)
}

/// `color.scale(black, $red: 100%)` → `red` 与 `color.scale(lightcoral, $red: -33%)` →
/// `rgb(63.0588235294%, …)`（`core_functions/color/scale/rgb.hrx::{red/max,red/low}`）：一个通道
/// **朝符号指向的那一侧界限移动**，移动量是它到那个界限之间还剩那段距离的给定份额——
/// 240 − (240 − 0) × 0.33 = 160.8，而 `turquoise` 的红 64 + (255 − 64) × 0.86 = 228.26 = `89.5137254902%`
/// （`…::red/high`）。所以一次 scale 永远出不去 gamut，这就是这个文件的行没有 `above_max` 的原因
/// （现量：`sass-spec/spec/core_functions/color/scale/` 下 `grep -rl above_max` = 0 个文件，对照同层的 `adjust/` = 17）：
/// `100%` 落在界限**上**
/// （`scale/hsl.hrx::saturation/max` → `#ff7eff`）。
///
/// 同一个公式也跑在 hsl 轴上（`scale/hsl.hrx`：plum 自己的饱和度 47.2868217054% 按 67% 缩放给出
/// `rgb(95.6%, 53.8117647059%, 95.6%)`），也跑在 whiteness 与 blackness 上——那里溢出的一对之后按与 `adjust`
/// 完全相同的方式归一化，`color.scale(#66cc99, $whiteness: 100%)` → `rgb(83.3333333333%, …)`
/// （`scale/hwb.hrx::whiteness/max`）——也跑在 alpha 上，它的界限是 `0–1`：`rgba(red, 0.5)` 配 `$alpha: 14%`
/// → `0.57`，`0.3` 配 `-36%` → `0.192`（`scale/no_space.hrx::alpha/{high,low}`）。
///
/// `$hue:` **decline**：语料对它给的是硬错误 `Error: $hue: Channel isn't scalable.`
/// （`scale/error/args.hrx::unknown`，另见 `scale/error/polar.hrx::{legacy,lch,oklch}`），这句消息本编译器还没造出来，
/// 所以这一刀退回文本；而角度本来也没有可朝它缩放的界限——借 `adjust` 的 wrap 过来就是发明那条律。一次不点名
/// 通道的调用是 no-op（`scale/no_space.hrx::{positional,named}` → `red`），`adjust` 已经演示过。
pub(super) static COLOR_SCALE: Builtin = Builtin {
    name: "color.scale",
    params: &["color"],
    optional: &[
        "red",
        "green",
        "blue",
        "alpha",
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
        "space",
    ],
    variadic: true,
    apply: needs_labels,
    labelled: Some(scaled),
};

/// 同一条律的 legacy 全局写法。`scale/global.hrx::legacy` 是
/// `scale-color(pink, $blue: 20%)` → `rgb(100%, 75.2941176471%, 83.6862745098%)`——pink 的蓝 203 移动了
/// 255 之下还剩 52 的那段的 20%。那个文件的 `non_legacy` 行用 `$space: oklch` 点名 `$chroma:`，而这一刀
/// 没有那个空间的位置，所以它保住自己的文本。
pub(super) static SCALE_COLOR: Builtin = Builtin {
    name: "scale-color",
    params: &["color"],
    optional: &[
        "red",
        "green",
        "blue",
        "alpha",
        "hue",
        "saturation",
        "lightness",
        "whiteness",
        "blackness",
        "space",
    ],
    variadic: true,
    apply: needs_labels,
    labelled: Some(scaled),
};

use super::write::route;
