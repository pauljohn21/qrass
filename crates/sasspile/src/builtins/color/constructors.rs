//! 色彩**构造子**（`rgb`/`rgba`/`hsl`/`hsla`）——那些造出一个 [`Color`] 而不是读它或移动它的调用。从域根里
//! 拆出来，是因为语料把构造当成自己的一条律：在这里决定输出形体（`Representation`）的东西，不是轴操作后来
//! 决定的那一样。

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::{Color, Space};
use crate::value::Value;

/// `rgb(18, 52, 86)` → `rgb(18, 52, 86)`、`rgb(256, 0, 0, 0.5)` → `rgba(255, 0, 0, 0.5)`，以及
/// `rgb(#123, 1)` → `#112233` —— 各出自一个 fixture（`three_args/unitless::numbers`、
/// `rgb/four_args/clamped.hrx::red/finite`、`two_args::opaque_to/opaque`）。
///
/// 声明成**带名字的 variadic**：这个调用是重载的——一个或两个实参是颜色加一个可选 alpha，三个或四个是通道——
/// 所以个数由函数体判，而 `params`/`optional` 仍然给标签留下可绑的东西
/// （`rgb($red: 0, $green: 255, $blue: 127)` → `rgb(0, 255, 127)`，`three_args/unitless::named`）。
/// `channels` 也是一个名字，因为 CSS Color 4 的单参数写法就是这样写的（`rgb($channels: 0 255 127)`，
/// `rgb/one_arg/no_alpha.hrx::unitless/named`）。
pub(super) static RGB: Builtin = Builtin {
    name: "rgb",
    params: &["red", "green", "blue"],
    optional: &["alpha", "channels"],
    variadic: true,
    labelled: None,
    apply: channels,
};

/// `rgba(…)`，同一条律的四参数写法（`core_functions/color/rgba.hrx`）。
pub(super) static RGBA: Builtin = Builtin {
    name: "rgba",
    params: &["red", "green", "blue"],
    optional: &["alpha", "channels"],
    variadic: true,
    labelled: None,
    apply: channels,
};

/// `hsl(60rad, 100%, 50%)` → `hsl(197.7467707849, 100%, 50%)` 与 `hsl(0, 50, 50%)` → `hsl(0, 50%, 50%)`（两枚
/// 出自 `hsl/three_args/units.hrx`），而 `hsl(0, 500%, 50%)` 停在范围之外
/// （`hsl/three_args/out_of_gamut.hrx::saturation/above`）。标签绑起来与 `rgb` 一样（`hsl/three_args/named.hrx`）。
pub(super) static HSL: Builtin = Builtin {
    name: "hsl",
    params: &["hue", "saturation", "lightness"],
    optional: &["alpha", "channels"],
    variadic: true,
    labelled: None,
    apply: hsl_channels,
};

/// `hsla(…)`，四参数写法（`hsl/four_args/alpha.hrx`）。
pub(super) static HSLA: Builtin = Builtin {
    name: "hsla",
    params: &["hue", "saturation", "lightness"],
    optional: &["alpha", "channels"],
    variadic: true,
    labelled: None,
    apply: hsl_channels,
};

/// `hwb(0deg, 30%, 40%)` → `hsl(0, 33.3333333333%, 45%)` —— **打印**是 hsl，而值保住它的 whiteness 与
/// blackness（`hwb/three_args/units::hue/deg`）。登记两次，因为语料两种都调：全局的 `hwb`
/// （`hwb/global.hrx`，也是 CSS 自己的 `hwb()` 函数）与 `color.hwb`（`hwb/three_args/*`）。
pub(super) static HWB: Builtin = Builtin {
    name: "hwb",
    params: &["hue", "whiteness", "blackness"],
    optional: &["alpha", "channels"],
    variadic: true,
    labelled: None,
    apply: hwb_channels,
};

/// `color.hwb(…)`，同一条律的带命名空间写法。
pub(super) static COLOR_HWB: Builtin = Builtin {
    name: "color.hwb",
    params: &["hue", "whiteness", "blackness"],
    optional: &["alpha", "channels"],
    variadic: true,
    labelled: None,
    apply: hwb_channels,
};

/// 一个 alpha 实参：一个数字，或一个百分比——`hsl(0, 0%, 0%, 45.6%)` 就是 `hsla(0, 0%, 0%, 0.456)`
/// （`hsl/four_args/alpha.hrx`，而 `rgb/four_args/alpha.hrx` 对 `rgb` 说的是同一条）。
fn alpha(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, unit) if unit.is_unitless() => Some(*number),
        Value::Number(number, unit) if unit.single() == Some("%") => Some(*number / 100.0),
        _ => None,
    }
}

/// 一次 `rgb`/`rgba` 调用：三个通道加一个可选 alpha、一个颜色加一个可选 alpha，或 CSS Color 4 的
/// **单参数空格分隔**写法。一个颜色实参要么是一段声明本来会逐字保住的文本（hex **或**一个 CSS 关键词——
/// `rgba(red, 0.42)` 必须读成 `red`，`alpha.hrx::color/middle`），要么是一个已经造好的颜色（`Color::of`）。
fn channels(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        // 通道的种类与它们的单位是颜色自己的律（`Color::new` 除了数字与百分比都拒），所以函数体只需要把
        // 实参整成形状。
        [red, green, blue] => Color::new(red, green, blue, 1.0),
        [red, green, blue, given] => {
            alpha(given).and_then(|alpha| Color::new(red, green, blue, alpha))
        }
        // `rgb(18 52 86)` → `rgb(18, 52, 86)`（`rgb/one_arg/no_alpha.hrx::unitless/numbers`），而一个标签会把
        // 整个列表绑住（`…::unitless/named` 是 `rgb($channels: 0 255 127)`）。恰好三个分量：现代写法把它的 alpha
        // 放在一个 `/` 之后，那是另一条律、有它自己的 fixture（`rgb/one_arg/alpha.hrx`），这里不猜。
        [Value::Space(parts)] => match parts.as_slice() {
            [red, green, blue] => Color::new(red, green, blue, 1.0),
            _ => None,
        },
        // `rgb(18 52 86 / 0.5)` → `rgba(18, 52, 86, 0.5)`（`rgb/one_arg/alpha.hrx::in_gamut/partial`）：斜杠列表
        // 的第二个元素就是 alpha，按与逗号写法的第四个实参同一条律读——所以一个百分比 alpha 会换算
        // （`…::in_gamut/percent` 给出 `50%` → `0.5`），而一个等于 1 的 alpha 退回三参数写法
        // （`…::in_gamut/opaque`）。
        [Value::Slash(list)] => match list.as_slice() {
            [Value::Space(parts), given] => match parts.as_slice() {
                [red, green, blue] => {
                    alpha(given).and_then(|alpha| Color::new(red, green, blue, alpha))
                }
                _ => None,
            },
            _ => None,
        },
        [source] => Color::of(source),
        [source, given] => match (Color::of(source), alpha(given)) {
            (Some(color), Some(alpha)) => Some(color.with_alpha(alpha)),
            _ => None,
        },
        _ => None,
    };
    finished(built)
}

/// 一次 `hsl`/`hsla` 调用。alpha 分开读，因为颜色存的是归一到 `0–1` 的它。
fn hsl_channels(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        [hue, saturation, lightness] => Color::hsl(hue, saturation, lightness, 1.0),
        [hue, saturation, lightness, given] => {
            alpha(given).and_then(|alpha| Color::hsl(hue, saturation, lightness, alpha))
        }
        // `hsl(0 100% 50%)` → `hsl(0, 100%, 50%)`（`hsl/one_arg/no_alpha.hrx::in_gamut/red`）：现代写法的轴遵循
        // 与逗号写法同样的律——色相折叠成度（`…::units/hue/deg` 是 `hsl(0deg 100% 50%)`），饱和度与亮度得到它们
        // 的 `%`（`…::units/saturation/unitless` → `hsl(0, 50%, 50%)`），两者都不夹紧
        // （`…::out_of_gamut/saturation/above` 保住 `500%`）——所以这一臂只需要把实参整形，不需要重读它们。
        [Value::Space(parts)] => match parts.as_slice() {
            [hue, saturation, lightness] => Color::hsl(hue, saturation, lightness, 1.0),
            _ => None,
        },
        // alpha 放在斜杠后的那种写法，与 `rgb` 同一条律：`hsl(0 100% 50% / 1.1)` 把 alpha 夹到 1，于是打的是三参数
        // 形式，而 `/ -0.1` 夹到 `0` 并保住 `hsla(…)`
        // （`hsl/one_arg/alpha.hrx::clamped/alpha/{above,below}`）。轴保持自己的规则不变
        // （`…::clamped/saturation` 给出 `0%`，`…::clamped/lightness` 保住 `9999%`）。
        [Value::Slash(list)] => match list.as_slice() {
            [Value::Space(parts), given] => match parts.as_slice() {
                [hue, saturation, lightness] => {
                    alpha(given).and_then(|alpha| Color::hsl(hue, saturation, lightness, alpha))
                }
                _ => None,
            },
            _ => None,
        },
        _ => None,
    };
    finished(built)
}

/// 一次 `hwb`/`color.hwb` 调用。形状与 `hsl` 相同：三个逗号实参、三个空格分隔的（`hwb/one_arg.hrx`）、那些再
/// 加一个 `/ alpha`（`hwb/global.hrx::alpha/in_gamut/partial` 是 `hwb(180 30% 40% / 0.5)`），以及整个列表放在
/// `$channels:` 下（`…::alpha/in_gamut/named`）。
fn hwb_channels(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        [hue, whiteness, blackness] => Color::hwb(hue, whiteness, blackness, 1.0),
        [hue, whiteness, blackness, given] => {
            alpha(given).and_then(|alpha| Color::hwb(hue, whiteness, blackness, alpha))
        }
        [Value::Space(parts)] => match parts.as_slice() {
            [hue, whiteness, blackness] => Color::hwb(hue, whiteness, blackness, 1.0),
            _ => None,
        },
        [Value::Slash(list)] => match list.as_slice() {
            [Value::Space(parts), given] => match parts.as_slice() {
                [hue, whiteness, blackness] => {
                    alpha(given).and_then(|alpha| Color::hwb(hue, whiteness, blackness, alpha))
                }
                _ => None,
            },
            _ => None,
        },
        _ => None,
    };
    finished(built)
}

fn finished(built: Option<Color>) -> Result<Value, SassError> {
    built
        .map(Value::Color)
        .ok_or_else(|| SassError::new("not a colour this slice can build"))
}

/// CSS Color 4 的 `color()` 语法：`color(srgb 0.2 0.4 0.8)` → 它自己，`color(srgb 0.1 0.2 0.3 / 0.4)` 保住
/// alpha，`color(srgb none 0.2 0.3)` 把一个 **powerless** 通道作为数据保住（`Color::in_space`），而空间名是
/// 大小写不敏感地匹配的（`color/color/no_alpha.hrx::case` 写 `color(sRGB …)`，读回 `srgb`）。名字加三个通道
/// 作为一个空格分隔的列表到达，与 `rgb` 读的是同一个形状（`rgb/one_arg/no_alpha.hrx`），alpha 在 `/` 之后
/// （`rgb/one_arg/alpha.hrx`）。
///
/// 一个相对色 —— `color(from #aaa srgb r g b)` —— **不属于**这一刀：第一个分量是 `from` 这个词，而语料说那整个
/// 调用留作字符串（`color/color/relative_color.hrx`），所以在这里 decline 恰好答对，不是碰巧。
pub(super) static COLOR_SPACE: Builtin = Builtin {
    name: "color",
    params: &["channels"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: in_space,
};

/// 空间名与那三个通道，按原样读。一个不是 `deg` 也不是 `%` 的单位根本不是色彩通道（`color(rgb …)` 会是一个
/// `px` 量），所以那次调用退回自己的文本，而不是发明一个错误消息那摊工作（P7）还没说出的数字。
fn in_space(args: Vec<Value>) -> Result<Value, SassError> {
    let decline = || SassError::new("not a colour space this slice can build");
    let (parts, given) = split_run(&args).ok_or_else(decline)?;
    let [name, channels @ ..] = parts else {
        return Err(decline());
    };
    // 名字走到这里已经是一个裸词；一个插值或一个带引号的字符串不是一个空间。
    let Value::Raw(text) = name else {
        return Err(decline());
    };
    let space = Space::parse(text).ok_or_else(decline)?;
    build(space, channels, given).ok_or_else(decline)
}

/// 一次空间 run，以它两种形状里的那一种到达：一列裸的分量，或者那列分量再加一个 `/` 之后的 alpha。这件事住在
/// 这里，是因为 `color(…)` 与那些函数式写法（`lab(…)`、`lch(…)`，……）是同一次 run —— 只有空间的*名字*来自
/// 哪里不同 —— 而“斜杠带着 alpha”不能说两遍。
fn split_run(args: &[Value]) -> Option<(&[Value], f64)> {
    match args {
        [Value::Space(parts)] => Some((parts.as_slice(), 1.0)),
        [Value::Slash(list)] => {
            let [Value::Space(parts), given] = list.as_slice() else {
                return None;
            };
            Some((parts.as_slice(), alpha(given)?))
        }
        _ => None,
    }
}

/// run 的一个通道：`none` 作为一个 powerless 通道留在数据里，而一个数字保住这个空间蕴含的那条单位律
/// （`lab(50% …)` 是 `L = 50`、`oklab(100% …)` 是 `L = 1`、`color(srgb 50% …)` 是 `0.5` ——
/// [`Space::percent_full`] 是说出这一点的唯一一处，而打印机乘的是同一个数）。
fn channel(value: &Value, space: Space) -> Option<Option<f64>> {
    match value {
        value if crate::color::is_missing_word(value) => Some(None),
        Value::Number(number, unit) => {
            let scaled = if unit.single() == Some("%") {
                *number / space.percent_full()
            } else {
                *number
            };
            (unit.is_unitless() || matches!(unit.single(), Some("%") | Some("deg")))
                .then_some(Some(scaled))
        }
        _ => None,
    }
}

/// 三个通道加上 alpha，造出一个自带空间的颜色。run 读不了的任何东西都会让整次调用 decline——一个 `1px` 量、
/// 第四个分量、一个本该是数字的位置上的插值。
fn build(space: Space, parts: &[Value], alpha: f64) -> Option<Value> {
    let [first, second, third] = parts else {
        return None;
    };
    let channels = [
        channel(first, space)?,
        channel(second, space)?,
        channel(third, space)?,
    ];
    Some(Value::Color(Color::in_space(space, channels, alpha)))
}

/// CSS Color 4 的**函数式**写法 —— `lab(50% 50 -75)`、`lch(44.12% 60.09 280.5deg)`、`oklab(…)`、`oklch(…)`。
/// 它们自己命名自己的空间，所以那次 run 恰好是三个通道与 alpha，而那套读法背后的每一条律都是 `color(…)` 已经
/// 在用的（[`split_run`]、[`channel`]、[`build`]）。
///
/// 这些**不是 legacy 颜色**，而语料测的正是这件事：`to_space/lab/srgb.hrx::middle` 写
/// `color.to-space(lab(50% 50 -75), srgb)`，答 `color(srgb 0.5529242775 0.3381978942 0.9794902605)`；而
/// `hue.hrx::error/non_legacy` 直接拒掉 `color.hue(lch(0% 0 0deg))` —— 在这里造出的颜色在自己的空间里打印，并
/// 经由 `Representation::InSpace` 拒掉字节/轴访问器。
fn functional(args: Vec<Value>, space: Space) -> Result<Value, SassError> {
    let decline = || SassError::new("not a colour space this slice can build");
    let (parts, alpha) = split_run(&args).ok_or_else(decline)?;
    build(space, parts, alpha).ok_or_else(decline)
}

fn lab(args: Vec<Value>) -> Result<Value, SassError> {
    functional(args, Space::Lab)
}

fn lch(args: Vec<Value>) -> Result<Value, SassError> {
    functional(args, Space::Lch)
}

fn oklab(args: Vec<Value>) -> Result<Value, SassError> {
    functional(args, Space::Oklab)
}

fn oklch(args: Vec<Value>) -> Result<Value, SassError> {
    functional(args, Space::Oklch)
}

/// 四个函数式色彩空间，分开登记，因为一个 `Builtin` 带的是一个 fn 指针，而它们唯一的差别是各自那次 run 命名
/// 哪个空间。
pub(super) static LAB: Builtin = Builtin {
    name: "lab",
    params: &["channels"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: lab,
};

pub(super) static LCH: Builtin = Builtin {
    name: "lch",
    params: &["channels"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: lch,
};

pub(super) static OKLAB: Builtin = Builtin {
    name: "oklab",
    params: &["channels"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: oklab,
};

pub(super) static OKLCH: Builtin = Builtin {
    name: "oklch",
    params: &["channels"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: oklch,
};
