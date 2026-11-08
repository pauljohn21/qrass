//! Color —— 一个**由函数造出的**颜色，作为数据：它的通道、它的 alpha，以及定下它怎么打的那种表示。
//!
//! 三个文件，各带一条律，分开是因为它们各自被语料按自己的条件反驳过：这个文件持有值与它的读法
//! （[`Color::byte`]、[`Color::hsl_channel`]），[`ops`] 持有让颜色穿过 hsl 移动的轴算术，[`text`] 持有打印律
//! （整字节打数字或 hex，中间值打百分比，关键词造出的颜色打它的关键词）。
//!
//! ## 为什么颜色不只是通道
//!
//! 语料显示同一套通道会打三种不同的形，所以*颜色被创建时的形体*是值的一部分，不是事后一个格式化选型：
//!
//! | 由什么造出 | 打成形 | fixture |
//! |---|---|---|
//! | `rgb(18, 52, 86)` | `rgb(18, 52, 86)` | `core_functions/color/rgb/three_args/unitless::numbers` |
//! | `rgb(7.1%, 20.4%, 33.9%)` | `rgb(7.1%, 20.4%, 33.9%)` | `…/three_args/percents::all/percent` |
//! | `rgb(74.7%, 173, 93%)` | `rgb(74.7%, 67.8431372549%, 93%)` | `…/percents::unitless/green` |
//! | `rgb(#123, 1)` | `#112233` | `…/two_args::opaque_to/opaque` |
//! | `rgb(256, 0, 0, 0.5)` | `rgba(255, 0, 0, 0.5)` | `…/four_args/clamped.hrx::red/finite` |
//!
//! 一个百分比/无单位混用的调用把**全部**通道打成百分比形 —— 任何一处有百分比就把整个颜色变百分比形 ——
//! 而一个来自 hex 的颜色打 hex 并把缩写展开。这里没有任何东西是从“另一个实现怎么存颜色”推出来的：每一行都是
//! fixture 自己的 `output.css`。
//!
//! ## 一个 `Color` 不由“解析一个声明值”产出
//!
//! 这是本项目的**选型**，不是从某行金样读出来的律：一个 `Color` 只在某个函数把它造出来之后才存在，所以下面那些
//! 被夹紧的通道值是正形（它们进来时就是数字）。而**语料在这一件上正要相反的东西**：
//! `values/colors/alpha_hex/initial_letter` 的 input.scss 写 `four-digit: #AbCd;`，它的 output.css 逐字要的是
//! `  four-digit: rgba(170, 187, 204, 0.8666666667);` —— 也就是说归档要那个 hex **被解被归一**，而不是保住
//! 拼法与大小写。本编译器现在逐字回声它，所以这一行今天仍是红的（现量：
//! `cargo run -q -p ledger -- failures <run> "initial_letter"` 交出 got `#AbCd` / want `rgba(170, 187, 204, …)`）。
//! 不得把这个未闭的缺口当律写：保住拼法是一个**实现选择**，而它与此 case 的期望直接相冲。
//!
//! ## 通道保住它们被写下时的那个尺度
//!
//! `7.1%` 必须再以 `7.1%` 出来。把每个通道归一到 0–255 再除回去，得到的数不再逐字等于写下的那个（上面表格
//! 第三行就是这一类：`173` 要除回 `67.8431372549%`），所以一个通道存它的值**并且**存它进来的那个尺度，
//! 换算只发生在某个形体需要另一个尺度的那一刻。
//!
//! 通道在进来的路上被夹紧 —— `256` → `255`、`-1` → `0`、`-0` → `0`（`four_args/clamped.hrx::red/finite` 等）——
//! 范围外的百分比亦然：`percents.hrx::clamped/red` 的 input 是 `rgb(100.1%, 0, 0)`，而它的 output.css 逐字打
//! `  b: rgb(255, 0, 0);` —— 夹紧发生在数值上，百分比那个形随之消失（见 [`Color::new`] 里的同一件事）。

use crate::value::Value;

/// 一个写下的通道与属于它的两条尺度律，在构造子与打印分发把这个文件顶过行数预算时拆出去。那条缝就是代码
/// 本来就有的：一个通道说一个数怎么行为，这个文件说一个颜色是什么。
mod channel;
/// 每个色彩空间自己的通道名与它的逐通道百分比底数，作为每张表背后都有 fixture 的表格。从 [`space`] 拆出的理由
/// 与 [`primaries`] 相同，而且行数预算只是一半：一个 `change` 调用用空间自己的词汇点名通道
/// （`$red` / `$x` / `$chroma`），而归档给同一个空间的通道不同的底。
mod channels;
mod matrix;
mod ops;
/// primaries 表，在那些表与它们的出处把 [`space`] 顶过行数预算时从那里拆出。这里是四张字面表，第五张
/// （`srgb`）是对 `matrix` 里那个常数的委派；每一张**被拟合过**的都点名量出它的命令，而每个拟合数字
/// 背后的读数只住一份，在该批的 `openspec/…/tasks.md` 里。
mod primaries;
mod read;
mod space;
mod text;

pub use space::{InSpace, Space};

pub(crate) use channel::is_missing_word;
use channel::{Channel, missing_as_zero};

/// alpha 夹在 `0–1`，而 `NaN` 读成全透明 —— 与 `rgb` 的 `calc(NaN)` 通道得的是同一个读法
/// （`rgb/four_args/clamped`）。
fn clamp_alpha(alpha: f64) -> f64 {
    // 一个零在这里丢掉它的符号，因为 alpha 是被**夹紧**而不是被携带：`rgb(0, 0, 0, -0)` → `rgba(0, 0, 0, 0)`，
    // `hwb(…, -0)` → `hsla(…, 0)`（`core_functions/color/rgb/four_args/clamped.hrx::alpha/negative_zero`、
    // `…/hwb/four_args.hrx::alpha/negative_zero`）。一个超越函数收到 `-0.0` 则保住它（`math.sin(-0.0)` → `-0`）——
    // 那条律在打印机里，不在这里。
    if alpha.is_nan() || alpha == 0.0 {
        0.0
    } else {
        alpha.clamp(0.0, 1.0)
    }
}

/// 一个数字实参的幅值，不论它的单位 —— 上面那些轴律定下怎么处置单位，而 `hsl`/`hwb` 在这点上不同：饱和度、
/// 亮度、whiteness 与 blackness 拿写下的数，而色相会换算一个角（[`angle_of`]）。
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, _) => Some(*number),
        _ => None,
    }
}

/// 以一个色相作为度数，折进 `0–360`。一个**角单位**先换算（`hsl(60rad, 100%, 50%)` → `hsl(197.7467707849, …)`，
/// `hsl/three_args/units.hrx::hue/angle`）；一个不是角的单位什么都不换算，所以 `hwb(1in, 30%, 40%)` 是
/// `hsl(1, …)`（`hwb/three_args/units::hue/non_angle`），而不是这一刀还没有对应消息的错误。
///
/// 那个折叠也会**丢掉零的符号**：`color.hwb(-0, 30%, 40%, 0.5)` → `hsla(0, 33.3333333333%, 45%, 0.5)`，
/// `hsl(-1 * 0, 100%, 50%)` → `hsl(0, 100%, 50%)`（`core_functions/color/hwb/four_args.hrx::hue/negative_zero`、
/// `…/hsl/four_args/in_gamut.hrx::negative_zero/hue`）—— `f64::rem_euclid` 保住 `-0.0`，所以这条路不保。
pub fn angle_of(value: &Value) -> Option<f64> {
    let factor = match value {
        Value::Number(_, unit) => unit
            .single()
            .filter(|name| crate::unit::is_angle(name))
            .map_or(1.0, |name| crate::unit::measure(name).1),
        _ => 1.0,
    };
    let folded = (number(value)? * factor).rem_euclid(360.0);
    Some(if folded == 0.0 { 0.0 } else { folded })
}

/// 颜色被创建时的那个形体，它定下怎么打印。
///
/// 为 [`Representation::InSpace`] 变体丢掉了 `Eq`：一个在自己空间里的颜色携带 `f64` 通道，假装它们构成一个
/// 全等价会是关于浮点的第二个断言，而语料里没有任何东西这么说。本仓所有调用者曾经要过的只有 `PartialEq`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Representation {
    /// `rgb(18, 52, 86)` —— legacy 的逗号形，通道是数字
    Legacy,
    /// `rgb(7.1%, 20.4%, 33.9%)` —— 同一个形体，但写下的是百分比
    Percent,
    /// `#112233` —— 由 hex 字面量造出，所以它以 hex 打回去
    Hex,
    /// `red` —— 由 CSS 关键词造出。这样一个颜色在其字节仍能命名一个关键词时打回那个关键词
    /// （`lighten(red, 0%)` → `red`、`complement(red)` → `aqua`），否则退到 hex
    /// （`complement(turquoise)` → `#e04050`）；而一个从 `rgb(0, 0, 0)` 造出的颜色继续打 `rgb(0, 0, 0)` ——
    /// 分开两者的 fixture 见 [`text`]。
    Named,
    /// `hsl(60, 100%, 50%)` —— 色相是一个普通的度数字，饱和度与亮度是百分比
    /// （`core_functions/color/hsl/three_args/units.hrx`）
    Hsl,
    /// `hwb(0, 30%, 40%)` —— 通道是 hue、whiteness 与 blackness，而**打印**用语料给的那个 hsl 形：
    /// `hwb(0deg, 30%, 40%)` → `hsl(0, 33.3333333333%, 45%)`
    /// （`core_functions/color/hwb/three_args/units.hrx::hue/deg`）。whiteness 与 blackness 仍是这个颜色*是*
    /// 什么，这就是为何 `color.channel(…, "whiteness", $space: hwb)` 能把它们读回（`channel/hwb.hrx`），
    /// 而一个普通的 hsl 颜色不能。
    Hwb,
    /// 这个颜色**就是**它的空间通道：由 `color(…)` 造出或由 `color.to-space` 移过来。它是表示的一个变体而不是
    /// 旁边的第二个字段，因为一个事实必须只有一个判者。当 `space: Option<InSpace>` 坐在 `representation` 旁边时，
    /// 每个只看表示的读法都看到 `Legacy`，于是拿着空的字节三元组去答一个 `display-p3` 的颜色 —— 那次量具点名了
    /// 15 条 `non_legacy` 行被打成 `0%`、`1` 或 `rgb(85, 85, 85)`（现量：
    /// `cargo run -q -p ledger -- failures <run> "error/non_legacy" --count` = **15**，它们是 15 个函数文件各一条，
    /// 而不是同一个文件里的 15 行），而语料对它们答的是一个错误 —— `hue.hrx::error/non_legacy` 逐字要：
    /// `Error: color.hue() is only supported for legacy colors. Please use color.channel() instead with an explicit $space argument.`
    /// —— 那整句单独占一行（不得拆成两行写，否则 grep 不回归档原文）。现在一个按表示匹配的消耗者必须
    /// 说出一个空间色对它是什么意思，否则构建就失败。
    InSpace(InSpace),
}

impl Representation {
    /// 通道是 `0–255` 字节的那些表示。**这个集合只命名一次**，因为同样四个臂过去在每个用处被重打一遍
    /// （`xyz`、换算的源侧读法、打印分发），而每一份副本都是一个可能漏掉第五个存字节表示的地方。`Space` 颜色不在
    /// 里面：它们的三元组由 [`Color::in_space`] 填成 `Some(0.0)`，而它们的通道是空间自己的，不是字节。
    pub(super) fn writes_bytes(self) -> bool {
        matches!(self, Self::Legacy | Self::Percent | Self::Hex | Self::Named)
    }
}

/// 一个颜色值。
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    channels: [Channel; 3],
    alpha: f64,
    representation: Representation,
    /// **由产出这个颜色的那次调用写下的** hsl 轴，当它来自 [`Color::with_axes`] 时。一个 gamut 外的颜色打它的轴，
    /// 而那些轴必须是调用点名的：`color.change(red, $lightness: 120%)` → `hsl(0, 100%, 120%)`
    /// （`change/hsl.hrx::lightness/above_range`），而从它蕴含的字节反推会得到 `hsl(180, -100%, 120%)`，因为一个
    /// 负的彩度会旋转色相。其余地方都是 `None`，所以一个被构造或被 adjust 的颜色什么都不变。
    authored: Option<[f64; 3]>,
}

impl Color {
    /// 三个通道照写下那样存，已经夹紧，加上一个夹在 `0–1` 的 alpha。一个写成 `none` 的通道作为缺席被携带并
    /// 保持缺席：`rgb(none 20 30)` 转到 `rgb` 得 `rgb(none 20 30)`，转到 `srgb` 得
    /// `color(srgb none 0.0784313725 0.1176470588)`（`to_space/rgb/{rgb,srgb}.hrx::missing/red`），所以那个未书写
    /// 的通道是关于颜色的一个事实，不是解析器发明的一个 `0`。
    pub fn new(red: &Value, green: &Value, blue: &Value, alpha: f64) -> Option<Self> {
        let channel = |value: &Value| match value {
            Value::Raw(text) if is_missing_word(value) => Some(Channel {
                value: None,
                percent: false,
            }),
            Value::Number(number, unit) if unit.is_unitless() => Some(Channel {
                value: Some(*number),
                percent: false,
            }),
            Value::Number(number, unit) if unit.single() == Some("%") => Some(Channel {
                value: Some(*number),
                percent: true,
            }),
            _ => None,
        };
        let [red, green, blue] =
            [channel(red)?, channel(green)?, channel(blue)?].map(Channel::clamped);
        Some(Self {
            channels: [red, green, blue],
            alpha: clamp_alpha(alpha),
            // 由写下的通道构造而来，所以没有一个独立的 authored 三元组。
            authored: None,
            // 是在夹紧**之后**定的，因为一个被夹过的百分比不再是百分比（[`Channel::clamped`]）：
            // `rgb(100.1% 0 0)` 已经没有百分比通道了，于是它打 `rgb(255, 0, 0)` 而不是全百分比形。
            representation: match (red.percent, green.percent, blue.percent) {
                (false, false, false) => Representation::Legacy,
                _ => Representation::Percent,
            },
        })
    }

    /// 以 hex 写下的颜色：三位、四位、六位或八位数字，大小写不限。其余一律 `None`，所以调用者保住它自己原有的
    /// 逐字行为。
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        let hex = |c: char| c.to_digit(16);
        let value = |from: usize, width: usize| {
            (0..width)
                .map(|index| hex(digits.chars().nth(from + index)?))
                .try_fold(0u32, |acc, nibble| Some(acc * 16 + nibble?))
        };
        match digits.len() {
            3 | 4 => {
                let byte = |index: usize| value(index, 1).map(|nibble| nibble * 17);
                let red = byte(0)?;
                let green = byte(1)?;
                let blue = byte(2)?;
                let alpha = match digits.len() {
                    4 => f64::from(byte(3)?) / 255.0,
                    _ => 1.0,
                };
                Self::rgb(f64::from(red), f64::from(green), f64::from(blue), alpha)
            }
            6 | 8 => {
                let red = value(0, 2)?;
                let green = value(2, 2)?;
                let blue = value(4, 2)?;
                let alpha = match digits.len() {
                    8 => f64::from(value(6, 2)?) / 255.0,
                    _ => 1.0,
                };
                Self::rgb(f64::from(red), f64::from(green), f64::from(blue), alpha)
            }
            _ => None,
        }
        .map(|color| Self {
            authored: None,
            representation: Representation::Hex,
            ..color
        })
    }

    /// 在 CSS 里以一个名字命名的颜色（`color.alpha(red)` → `1`，`core_functions/color/alpha.hrx::color/max`）。
    ///
    /// 那些关键词是经 `color` 这个 crate（linebender；活跃维护，面向 CSS Color 4）解出的，而不是一张手打表格，
    /// 因为本地语料没有一个完整的「名→RGB」来源 —— `values/colors/` 只有 `alpha_hex/` 与 `equality.hrx`（后者只说
    /// 少量几个，如 `purple == hsl(300, 100%, 25.098039215686%)`），而一个被审过的依赖强过我的记忆。
    /// **只取关键词→颜色那张映射**；一个颜色怎么打仍是本项目自己的律（`Representation`、`to_css`）。
    ///
    /// 那个依赖的表唯一缺的是 CSS 的 **`-grey` 拼法** —— 它的关键词表是 `148 − 7 个别名 + transparent = 142`。
    /// 语料到底用了哪些拼法，这里按现量写：`grep -rn "invert(grey" sass-spec/spec/` 交出的行形是
    /// `core_functions/color/invert/error.hrx` 里的 `a {b: invert(grey, $space: hsl)}`（不是裸 `invert(grey)`），
    /// `color.adjust(grey, $hue: 10deg, $space: hsl)` 也是真行；把语料里所有 `-grey` 词形拉出来统计，只有裸
    /// `grey`（17 行）与 `greyscale`，而 `darkgrey`/`lightgrey`/`slategrey` 那七个复合拼法**0 行** ——
    /// 所以别名折叠是为一个语料只部分使用过的覆盖面保险，不是它已被逐行演练过。那些拼法不需要任何 RGB 数据：
    /// CSS 把每个 `-grey` 形定义为同一条目的另一个拼法，所以别名在这里折掉，而值仍来自那个依赖。
    pub fn from_name(name: &str) -> Option<Self> {
        let keyword = name.trim().to_lowercase().replace("grey", "gray");
        let rgba = color::parse_color(&keyword)
            .ok()?
            .to_alpha_color::<color::Srgb>()
            .to_rgba8();
        Self::rgb(
            f64::from(u16::from(rgba.r)),
            f64::from(u16::from(rgba.g)),
            f64::from(u16::from(rgba.b)),
            f64::from(u16::from(rgba.a)) / 255.0,
        )
        .map(|color| Self {
            authored: None,
            representation: Representation::Named,
            ..color
        })
    }

    /// 以文本写下的颜色：hex 或关键词。用在实参作为不带引号的文本到达的地方，那正是一个 declaration 保住 `red`
    /// 与 `#123` 的方式 —— 写成调用的 `rgb(...)`/`hsl(...)` 在到这里之前已经求值为 [`Value::Color`]。
    ///
    /// 带括号的文本被拒：[`Self::from_name`] 背后的那个解析器也能读 `color(display-p3 1 0 0)`、`lab(…)` 与
    /// `lch(…)`，而用 sRGB 字节去答那些，对于一个本编译器尚未实现的空间会是一个**错值**。decline 保住调用照写的
    /// 形，那正是 `core_functions/color/mix/predefined.hrx` 与其他空间的 `channel/*` 行要的。
    pub fn of(value: &Value) -> Option<Color> {
        match value {
            Value::Color(color) => Some(color.clone()),
            Value::Raw(text) => {
                let text = text.trim();
                if text.contains('(') {
                    return None;
                }
                if text.starts_with('#') {
                    Self::from_hex(text)
                } else {
                    Self::from_name(text)
                }
            }
            _ => None,
        }
    }

    /// 一个 `hwb`/带 alpha 的 `hwb` 颜色，来自全局的 `hwb`/`color.hwb` 名字或 `color()` 语法。
    ///
    /// 通道按调用给的那样存 —— `[hue, whiteness, blackness]` —— 因为那正是 `color.channel(…, $space: hwb)` 读回的
    /// 东西（`channel/hwb.hrx`）。fixture 说出的两条律，而两者都与 `rgb` 不同：
    /// * 色相循 `hsl` 那条律：一个角单位折到度数（`hwb(1rad, 30%, 40%)` → `hsl(57.2957795131, …)`），而一个非角
    ///   被当作普通数字取（`hwb(1in, …)` → `hsl(1, …)`，`hwb/three_args/units::hue/non_angle`）；
    /// * whiteness 与 blackness **不夹在 `0–100`**，而一对溢出该颜色的值在**创建时就被归一**：
    ///   `hwb(10deg 70% 50%)` 把它的 whiteness 报成 `58.3333333333%`、blackness 报成 `41.6666666667%`
    ///   （`channel/hwb.hrx::local/normalized/*` —— 即 `70/120` 与 `50/120`），所以到任何东西把它读回时，这个颜色
    ///   *是*什么已经变了。`101%` 与 `-1%` 也以同样的方式到达轴，那正是钉住 [`crate::axes::hwb_to_hsl`] 的东西
    ///   （`hwb/four_args::whiteness/*`）。
    pub fn hwb(hue: &Value, whiteness: &Value, blackness: &Value, alpha: f64) -> Option<Self> {
        let (white, black) = crate::axes::normalise_hwb(number(whiteness)?, number(blackness)?);
        Some(Self {
            channels: [
                Channel {
                    value: Some(angle_of(hue)?),
                    percent: false,
                },
                Channel {
                    value: Some(white),
                    percent: true,
                },
                Channel {
                    value: Some(black),
                    percent: true,
                },
            ],
            alpha: clamp_alpha(alpha),
            authored: None,
            representation: Representation::Hwb,
        })
    }

    /// 一个来自 `0–255` 字节值的颜色，给 hex 解析用。
    fn rgb(red: f64, green: f64, blue: f64, alpha: f64) -> Option<Self> {
        let plain = |value: f64| Value::Number(value, crate::unit::Unit::unitless());
        Self::new(&plain(red), &plain(green), &plain(blue), alpha)
    }

    /// 一个 `hsl`/`hsla` 颜色。fixture 说出的三条律，没有一条与 `rgb` 共通：
    /// * **色相折到度数并 wrap 进 `0–360`** —— `hsl(60rad, 100%, 50%)` 是 `hsl(197.7467707849, 100%, 50%)`，
    ///   而 `0deg` 打成 `0`（`hsl/three_args/units.hrx`）；
    /// * 饱和度与亮度**永远**带一个 `%`（`hsl(0, 50, 50%)` → `hsl(0, 50%, 50%)`），而上端两者都不夹：
    ///   `hsl(0, 500%, 50%)` 与 `hsl(0, 100%, 500%)` 原样留住
    ///   （`hsl/three_args/out_of_gamut.hrx`，以及 `hsl/one_arg/no_alpha.hrx::out_of_gamut/saturation/above`
    ///   与 `hsl/one_arg/no_alpha.hrx::clamped/lightness/above`）；
    /// * 而一个**负的饱和度**会去 `0` —— `hsl(0 -100% 50%)` 是 `hsl(0, 0%, 50%)`
    ///   （`hsl/one_arg/no_alpha.hrx::clamped/saturation/below`）—— 而一个负的亮度则不管
    ///   （`hsl/one_arg/no_alpha.hrx::clamped/lightness/below` 保住 `-100%`）。**这条不对称不只在单参数拼法上：**
    ///   legacy 逗号形自己就说出来 —— `hsl/three_args/bounds.hrx::saturation/below` 的 `hsl(0, -100%, 50%)` 得
    ///   `value: hsl(0, 0%, 50%)`，而 `::lightness/below` 的 `hsl(0, 100%, -100%)` 原样留住；
    /// * alpha 夹在 `0–1`，而一个 `1` 的 alpha 返回三参数的拼法（`hsl/four_args/alpha.hrx`）。
    pub fn hsl(hue: &Value, saturation: &Value, lightness: &Value, alpha: f64) -> Option<Self> {
        Some(Self {
            channels: [
                Channel {
                    value: Some(angle_of(hue)?),
                    percent: false,
                },
                Channel {
                    value: Some(number(saturation)?.max(0.0)),
                    percent: true,
                },
                Channel {
                    value: Some(number(lightness)?),
                    percent: true,
                },
            ],
            alpha: clamp_alpha(alpha),
            authored: None,
            representation: Representation::Hsl,
        })
    }

    /// 写过一个 `rgb` 通道之后的同一个颜色，保住它被造出的方式：`color.change(black, $red: 255)` → `red`，
    /// `color.change(red, $red: 100)` → `#640000`（`change/rgb.hrx`）。
    ///
    /// **这里什么都不夹。** `color.change(black, $red: 500)` 保住 500，而语料把结果打成
    /// `hsl(0, 5000%, 98.0392156863%)`（`change/rgb.hrx::red/out_of_range`）—— 一个 gamut 外的值是 Sass 能
    /// 表示的一个值，不是一个错误，而夹紧会把它变成 `rgb(255, 0, 0)`，一个不同的颜色。[`Channel::clamped`] 就待在那：
    /// 只有一个*写下的* `rgb`/`hsl` 通道在创建时夹紧。
    ///
    /// 值是个 `Option`，因为调用可以写**缺**：`color.change(black, $red: none)` → `rgb(none 0 0)`，
    /// green 与 blue 同理（`change/rgb.hrx::{red,green,blue}/none`）。`adjust` 与 `scale` 永远不会带着 `None`
    /// 走到这里 —— 它们的读者是 `byte`，它直接拒绝一个 `none`
    /// （`amount::written_byte` 是唯一允许它的读者，而它是 `change` 一家到这里唯一的路），所以没有 fixture 要一
    /// 个 shift 或 scale 去移动一个缺。
    ///
    /// **一个数字写入需要那个被点名的通道存在。** 语料把它说成一个错误 ——
    /// `color.adjust(rgb(none 0 0), $red: 10)` 必须逐字以它下面那一整行的错误失败
    /// `Error: $red: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently support modifying missing channels (color: rgb(none 0 0)).`
    /// （`adjust/error/missing.hrx::legacy`，而 `scale` 在 `scale/error/missing.hrx::legacy` 同理）。把那个缺失
    /// 通道读成 `0` 再写一个答，会**发明一个归档禁止的值**（`rgb(10, 0, 0)`），这正是这道守卫存在的目的；在这里
    /// 返 `None` 是那个 *decline*，它保住调用自己的文本。把语料那句消息逐字报出来是一个独立的增量，
    /// 在 `tasks.md` 9.23 里点了名。这个拒答是按**调用点名的那个通道**定键的，而一个 fixture 要的就是这份狭窄而不是
    /// 我的偏好：语料要 `color.change(rgb(none 0 none), $green: 300)` 回来时 green 已写、两个孪生值仍是 `none`
    /// （`to_gamut/space.hrx::clip/missing/legacy/same/with_mapping`，它的 output.css 打 `rgb(none 255 none)`）。
    /// 那一行在每个真跑过它的 run 里都是红的（不得写成“所有记录的 run”：现量
    /// `cargo run -q -p ledger -- failures <run> "missing/legacy/same/with_mapping" --count` 在本批 run 与
    /// `change-writes-absence-final` 都交出 **2** 条 —— `clip/…` 与 `local_minde/…` 各一 —— 而库里最早那批 run
    /// 根本还没跑到这个 fixture，所以上一句只能限定到“有这条 needle 的 run”），而这一点本行不得假装不知道：
    /// `color.to-gamut` 根本还没接线
    /// （现量：`grep -rn "to-gamut\|to_gamut" crates/sasspile/src` 只命中注释，而
    /// `cargo run -q -p ledger -- failures change-writes-absence-final "to_gamut" --count` = **117 行红**），
    /// 所以它回声整个调用、并不携带关于内层那次的信息 —— 这份狭窄靠的是金样本文本身，而它被
    /// `tests/color_legacy_missing.rs` 直接断言。
    ///
    /// 写入一个缺不需要读任何东西，所以在一个已经未书写的通道上 `change` 在这里是被允许的。
    /// **没有 fixture 裁这个 case**（现量：`grep -rn "color.change(rgb(none" sass-spec/spec/` 只命中不带通道实参的
    /// 调用 —— `change/space.hrx` 那三行都不带通道实参，其中两行只带 `$space:`，另一行什么实参也没有 —— 加上
    /// `to_gamut/space.hrx` 里那两行的 `$green: 300`），所以它是 `Option` 那个形状的一个推论 —— 幂等，而且它
    /// 发明不出一个数字 —— 而不是归档印出的一条律。
    pub fn with_channel(&self, index: usize, value: Option<f64>) -> Option<Self> {
        let written = Channel {
            value,
            percent: false,
        };
        let writable = self
            .channels
            .get(index)
            .is_some_and(|channel| channel.value.is_some() || value.is_none());
        (self.writes_bytes() && writable).then(|| Self {
            channels: [
                if index == 0 {
                    written
                } else {
                    self.channels[0]
                },
                if index == 1 {
                    written
                } else {
                    self.channels[1]
                },
                if index == 2 {
                    written
                } else {
                    self.channels[2]
                },
            ],
            ..self.clone()
        })
    }

    /// 这个颜色的通道是否是 `0–255` 字节，那正是一个通道能被 `color.change` **写入**的条件；一个 `hsl` 或
    /// `hwb` 颜色存的是不同的轴，往它们里写一个字节会是一个不同的颜色。**注意这个条件没有金样**（现量：
    /// `grep -c '\$red' change/hsl.hrx` 与 `change/hwb.hrx` 都是 **0**，两文件里只出现 `$hue`/`$saturation`/
    /// `$lightness`/`$whiteness`/`$blackness`/`$alpha`；全仓 `color.change(hsl…$red` 与 `color.change(hwb…$red` 也
    /// 是 0 命中），所以“写字节进 hsl/hwb 会得一个不同的颜色”是按值型推的，不是归档说的；这里的 `writes_bytes`
    /// 守卫与“那些调用会 decline”同属一个选型。
    pub fn writes_bytes(&self) -> bool {
        self.representation.writes_bytes()
    }

    /// 颜色是否带有一个从未书写的通道。[`Color::in_space`] 会为一个空间颜色把字节三元组填成 `Some(0.0)`
    /// （而一个 `hsl`/`hwb` 颜色还存不下一个 `none`），所以这两类在这里都是 false —— [`Color::as_legacy_rgb`]
    /// 里那道恒等守卫靠的正是这个事实，所以写在这里而不是留给读者自己去核。
    pub(super) fn has_unwritten_channel(&self) -> bool {
        self.channels.iter().any(|channel| channel.value.is_none())
    }

    /// 同一个颜色换一个 alpha，保住它被造出的方式：`rgb(#123, 0.5)` 就是那些 hex 通道加一个新的 alpha，
    /// 不是一个不同的颜色（`two_args::opaque_to/partial`）。
    pub fn with_alpha(&self, alpha: f64) -> Self {
        Self {
            alpha: clamp_alpha(alpha),
            ..self.clone()
        }
    }
}
