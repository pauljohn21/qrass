//! `color.change` / `color.adjust` / `color.scale` 的**写入侧**：一次调用在写哪个空间，以及四种表示各自为它
//! 拿到的算术。从 [`modify`] 里拆出来，那个文件留着签名——一个族声明哪些标签，以及一个读不了的实参的
//! 错误措辞——因为两半答的是不同的问题：“调用点名的”是一条签名律，“它写什么”是一条算术律，而两个都装在一个
//! 文件里已经越过本仓库的行数预算。

use super::amount::{self, axis, byte, keep, scale_toward, share, step};
use super::modify::Writing;
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::{Given, missing};
use crate::color::Color;
use crate::value::Value;

/// 一次调用写的是颜色自己哪个空间。只决定一次，从调用点名的东西算出来，好让下面的算术对 `(space, writing)`
/// 做 `match`，而不是每个函数叠一道守卫——三个函数乘三个空间就是九臂，而一条 `if` 链每多来第四种写法就得重读
/// 一遍。
pub(super) enum Space {
    /// 只有 `$alpha:`，或者一个通道都没点名——颜色原样透过去（`adjust/no_channels.hrx`）。
    Nothing,
    /// `rgb` 的通道，颜色把它们存成字节。
    Bytes,
    /// whiteness 与 blackness，颜色可能直接存着它们，也可能由字节推出。
    Hwb,
    /// hsl 的轴。
    Axes,
}

/// 一次 `color.change`/`color.adjust`/`color.scale` 调用：读颜色，对它点名的通道施加改动——按字节、按 hsl 轴或
/// 按 hwb 轴——然后让颜色自己的打印律决定输出形体。
pub(super) fn route(given: Vec<Given>, writing: Writing) -> Result<Value, SassError> {
    let decline = Err(amount::decline());
    let colour = match Given::find(&given, "color") {
        // `color.change()` —— 语料点名缺的那个参数（`change/error/args.hrx::too_few`，`adjust`/`scale` 里同一行）。
        None => return Err(missing("color".to_string())),
        // `$color: 1 is not a color.` 是对一个**数字**说的（`change/error/type.hrx::color`）。一个颜色在本
        // 编译器没有的空间里——`color(srgb …)`、`lab(…)`、`oklch(…)`——继续 decline：语料对这些调用没给消息，
        // 而把它们读成 sRGB 就是 [`Color::of`] 已经拒掉的那个错值。
        Some(value @ Value::Number(_, _)) => {
            return Err(SassError::typed(
                ErrorKind::NotAColor,
                format!("$color: {} is not a color.", value.to_css()),
            ));
        }
        Some(value) => Color::of(value).ok_or_else(amount::decline)?,
    };
    // 这三个把颜色按位置拿、把每个通道按名字拿：`color.change(red, 1)` 是
    // `Error: Only one positional argument is allowed. All other arguments must be passed by name.`
    // （`change/error/args.hrx::too_many`，`adjust`/`scale` 里同一行）。
    if Given::positional(&given) > 1 {
        return Err(SassError::typed(
            ErrorKind::TooManyArguments,
            "Only one positional argument is allowed. All other arguments must be passed by name.",
        ));
    }
    // `$space:` 要的是一个本编译器没有换算的空间，所以它 decline；现代空间的 `change`/`adjust`/`scale` 文件因此
    // 仍是红的——除非那个实参根本不是一个字符串，语料对这件事说得直接：`color.scale(red, $space: 1)` →
    // `Error: $space: 1 is not a string.`（`scale/error/type.hrx::space`，`adjust` 与 `change` 里同一行）。
    let beyond = match Given::find(&given, "space") {
        Some(Value::Raw(_)) => true,
        Some(value) => {
            return Err(SassError::typed(
                ErrorKind::NotAString,
                format!("$space: {} is not a string.", value.to_css()),
            ));
        }
        None => false,
    };
    let bytes = [("red", 0), ("green", 1), ("blue", 2)]
        .into_iter()
        .filter(|(name, _)| Given::names(&given, name))
        .collect::<Vec<(&str, usize)>>();
    let hsl = ["saturation", "lightness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .collect::<Vec<&str>>();
    let hwb = ["whiteness", "blackness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .collect::<Vec<&str>>();
    let hue = Given::names(&given, "hue");
    // **一个自带空间的颜色用那个空间自己的通道来写**，用的是那个空间自己的词汇：
    // `color.change(color(srgb 0.2 0.5 0.7), $red: 0.5)` → `color(srgb 0.5 0.5 0.7)`
    // （`change/srgb.hrx::red/unitless`），而 `change/xyz.hrx` 在同一个位置上写 `$x`、`change/lch.hrx` 写
    // `$chroma`——名字来自 `color::channels` 里那张空间表，不是这里的一份清单。只接了 `change`：
    // `adjust`/`scale` 打在自带空间上仍按现在这样 decline，而 `tasks.md` 9.25 点名了还在等它们的行。
    let owned = match (colour.carried_space(), writing) {
        (Some(space), Writing::Set) => space
            .channel_names()
            .into_iter()
            .filter(|name| Given::names(&given, name))
            .filter_map(|name| space.channel(name).map(|channel| (name, channel)))
            .collect::<Vec<(&'static str, usize)>>(),
        _ => Vec::new(),
    };
    // 色相没有可朝它移动的界限，所以点名它的 scale 被点名拒收：`color.scale(red, $hue: 10%)` →
    // `Error: $hue: Channel isn't scalable.`（`scale/error/args.hrx::unknown`；`white` 那一例在
    // `scale/error/polar.hrx::legacy`，同文件另两例是 `…::{lch,oklch}`），而 `scale/*.hrx`
    // 其他情形下从不使用这个参数。
    if writing == Writing::Scale && hue {
        return Err(SassError::typed(
            ErrorKind::Channel,
            "$hue: Channel isn't scalable.",
        ));
    }
    // 调用写下的那些通道名，按签名顺序——下面的拒语也就是按这个顺序报告的。
    let channels = bytes
        .iter()
        .map(|(name, _)| *name)
        .chain(hue.then_some("hue"))
        .chain(hsl.iter().copied())
        .chain(hwb.iter().copied())
        .collect::<Vec<&str>>();
    // 一次调用里出现两个空间是**点名拒收**，不是保住文本：第一个组说出这次调用在写哪个空间，而来自另一个空间的
    // 第一个名字就对着它报——`color.change(red, $red: 1, $hue: 1)` →
    // `Error: $hue: Color space rgb doesn't have a channel with this name.`，
    // `color.scale(red, $saturation: 1%, $blackness: 1%)` → `Error: $blackness: Color space hsl …`
    // （`{change,adjust,scale}/error/mixed_formats.hrx`）。
    if let Some(foreign) = amount::mixed_channel(&channels) {
        return Err(foreign);
    }
    // 一个自带空间的颜色是由**写入本身**在更下面拒掉的：`Color::with_axes` 经由 [`Color::axes_of`] 读它的轴，
    // 而后者对 `Representation::InSpace` 答 `None`；`Color::with_hwb` 会问 [`Color::writes_bytes`]，它对那个
    // 表示答 `false` —— 两条路都让这个调用退回自己的文本。这里刻意不在入口处放那道守卫。曾把守卫挪到通道读取
    // 之前的一次实验被量过并回退了，而理由是由行说出的，不是一枚再也复取不到的计数：
    // `scale/error/polar.hrx::lch` 写 `color.scale(lch(50% 30% 180deg), $hue: 10%)`，它的金样**就是**
    // `Error: $hue: Channel isn't scalable.`，而 `scale/error/units/oklch.hrx::wrong/lightness` 写
    // `color.scale(lch(30% 70% 200deg), $lightness: 1px)`，期望
    // `Error: $lightness: Expected 1px to have unit "%".` —— 一刀全拒两句都答不上，因为消息来自读实参那一步。
    // 让读取跑下去的代价记在 `tasks.md` 9.9：两行
    // （`change/unusual_numbers::{infinity,negative_infinity}/polar`）报
    // `$hue: calc(infinity) is not a number or unquoted "none"`，而语料要的是 `lch(50% 150 0deg)`，因为
    // `calc()` 到数字与 Color-4 的 `$space` 写入两样都还没造。
    // `$alpha:` 是在下面那道“其他什么都没点名”的拒语**之前**读的，因为语料对一次只给了一个坏 alpha 的调用也要报
    // 它：`color.change(red, $alpha: c)` → `Error: $alpha: c is not a number or unquoted "none".`
    // （`change/error/type.hrx::alpha`，带引号的 `"none"` 见 `…::quoted_none`），而
    // `color.adjust(red, $alpha: none)` → `Error: $alpha: none is not a number.` 对不接它的两族说的是朴素的那句
    // （`adjust/error/type.hrx::none`、`scale/error/type.hrx::none`）。
    let alpha = if Given::names(&given, "alpha") {
        Some(axis(&given, "alpha", writing)?)
    } else {
        None
    };
    // `change` 只在那个 alpha 本来就在范围内时才写它：每一条点名 alpha 的 `change` 行给的都是范围内的值
    // （`change/hsl.hrx::alpha_arg` 用 `0.7`），所以一个范围外的值该做什么——像 `rgb()` 那样夹紧，还是报一个类型
    // 错误——没有金样，而 decline 让那一行保持红态，而不是从另一个函数借一条律。
    if writing == Writing::Set && alpha.is_some_and(|alpha| !(0.0..=1.0).contains(&alpha)) {
        return decline;
    }
    // 这次调用在写哪个空间：字节、hsl 轴，还是 hwb 轴。`$hue:` 两个轴空间都有（`change/hsl.hrx::hue/*` 把它按
    // hsl 写，`change/hwb.hrx::all` 把它与 whiteness、blackness 一起写），所以它从不单独决定什么——而一次同时点名
    // 两个空间的调用在上面已经被点名拒收了，所以这里最多只有一个组被置上。
    let named = bytes.len() + hsl.len() + hwb.len() + usize::from(hue) + owned.len();
    if beyond || (named == 0 && writing == Writing::Set) {
        return decline;
    }
    // 一个被点名却读不出来的通道会停下这次调用——实参在场但用不了时用语料那句消息（`$red: c is not a number.`），
    // 而当这个族有一条这一刀还没拿到的律时退回拒答：`color.change(hsl(0deg 100% 50%), $hue: none)` 要的是
    // `hsl(none 100% 50%)`，那是一条缺通道的律，不是一条类型错误。
    let reads = ["hue", "saturation", "lightness", "whiteness", "blackness"]
        .into_iter()
        .filter(|name| Given::names(&given, name))
        .map(|name| axis(&given, name, writing).map(|value| (name, value)))
        .collect::<Result<Vec<(&str, f64)>, SassError>>()?;
    // 写哪个空间。上面那道守卫已经拒掉了同时点名两个空间的调用，所以这几个标志里恰好只有一个被置上。
    let space = match (named == 0, !bytes.is_empty(), !hwb.is_empty()) {
        (true, _, _) => Space::Nothing,
        (false, true, _) => Space::Bytes,
        (false, false, true) => Space::Hwb,
        _ => Space::Axes,
    };
    let built = match colour.carried_space() {
        // 空间自己的名字在上面已经收好了，所以这里的 `owned` 为空意味着这次调用点了一个该空间没有通道的名字——
        // 它保住自己的文本，和这条臂存在之前一样。
        Some(space) if !owned.is_empty() => amount::write_carried(&colour, &given, space, &owned),
        _ => match (space, writing) {
            (Space::Nothing, _) => Ok(colour.clone()),
            (Space::Bytes, _) => {
                bytes
                    .into_iter()
                    .try_fold(colour.clone(), |colour, (name, index)| {
                        let argument = Given::find(&given, name).ok_or_else(amount::decline)?;
                        // 实参是在颜色的当前字节**之前**读的，而这个次序是承重墙：语料那句消息就来自这一次读。
                        // `scale/error/units/srgb.hrx::wrong/red` 期望的是 `$red: Expected 1px to have unit "%".`，
                        // 而那次调用的字节读取本也会失败；把两次读倒过来，它就只用调用的文本作答了——我恰好做过
                        // 这个交换并量到：42 条 `scale/error/units/*` 行变红。
                        //
                        // 语料在 legacy 字节上写下的通道形式对 `change` 与一次 shift 是同一个：一个 `0–255` 的无单位
                        // 数字；百分比那条形在 legacy 字节侧没有 fixture（见 [`byte`] 的注），带 `%` 的字节通道只在自带
                        // 空间那侧出现，那一侧走 [`amount::written_channel`]。一个 scale 的实参不是通道，而是到界限还剩
                        // 那段距离的一个**份额**：`$red: 100%` 意思是走到 255 的全程
                        // （`color.scale(black, $red: 100%)` → `red`，`scale/rgb.hrx::red/max`），所以它必须带 `%`
                        // （`scale/error/units/rgb.hrx`）。只有 `change` 可以写**缺席**——`$red: none` 就是缺通道那个答
                        // （`change/rgb.hrx::red/none` → `rgb(none 0 0)`）——这就是它的读法是
                        // [`amount::written_byte`]、唯一一个允许 `none` 的读法。
                        let amount: Option<f64> = match writing {
                            Writing::Set => amount::written_byte(argument, name)?,
                            Writing::Shift => Some(byte(argument, name, writing)?),
                            Writing::Scale => Some(share(name, argument)?),
                        };
                        // 然后是当前字节，为那些要移动它的写法读它。`change` 也读它，而这个读正是让
                        // `Color::with_channel` 在一个颜色从未写过的通道上拒掉一次数字写入的东西
                        // （`adjust/error/missing.hrx::legacy`）——`change` 绕不过它。
                        let current = colour.byte(index).ok_or_else(amount::decline)?;
                        let written: Option<f64> = match (writing, amount) {
                            // `change` 保住给它的那个值，并让 out-of-gamut 规则改打 hsl；这里什么都不夹紧
                            // （`change/rgb.hrx::red/out_of_range` 保住 500）。
                            (Writing::Set, written) => written,
                            // `adjust` 移动通道而过不去两端：`color.adjust(#abcdef, $red: 200)` 是 `#ffcdef`，那个字节
                            // 被夹到 255（`adjust/rgb.hrx::red/above_max`）。
                            (Writing::Shift, Some(delta)) => {
                                Some((current + delta).clamp(0.0, 255.0))
                            }
                            // 一个 scale 按构造也过不去任何一端（`scale/rgb.hrx::red/{max,min}`），所以它提不出夹紧的问题。
                            (Writing::Scale, Some(part)) => {
                                Some(scale_toward(current, part, 0.0, 255.0))
                            }
                            // 除了 `written_byte` 没有别的读法会答 `None`，而那一个只在 `Set` 下跑；这里 decline 而不是
                            // 代一个 `0` 进去，是为了不让一个不可达变成一枚编造的值。
                            (_, None) => return Err(amount::decline()),
                        };
                        colour
                            .with_channel(index, written)
                            .ok_or_else(amount::decline)
                    })
            }
            (Space::Hwb, _) => {
                // 颜色没存的 whiteness 与 blackness 必须由它的字节推出；一个 `hsl` 颜色没有这种读法，也没有哪一行要它，
                // 所以那次调用保住自己的文本。
                let current = colour.hwb_of().ok_or_else(amount::decline)?;
                // 这里对任何写法都没有夹紧：那一对在之后按它的和归一化，这正是
                // `color.adjust(#cc6666, $whiteness: 100%)` → `rgb(87.5%, …)` 所示
                // （`adjust/hwb.hrx::whiteness/max`），而 `color.scale(#66cc99, $whiteness: 100%)` →
                // `rgb(83.3333333333%, …)` 重复了它（`scale/hwb.hrx::whiteness/max`）。
                //
                // 调用**没**点名的通道保住它当前的值——而这一点按写法读起来不同：`change` 要的是值本身（什么都不写），
                // `adjust` 不要偏移，`scale` 不要份额。把当前值再喂过同一条 `keep(..)` 路径曾把它翻倍
                // （`#993333` 配 `$blackness: 40%` 出来是 `#555555` 而不是 `#333333`，`…::blackness/high`），
                // 因为那次 shift 又加上了本已在那儿的量。
                //
                // 色相坐在下标 0 上，而这里**不**折叠它：点名它的 `scale` 在上面就被拒了，所以它的份额是零，
                // `scale_toward(hue, 0.0, …)` 返回当前值；一个 `set` 写的是 `angle_of` 在读的时候已经折好的值；而一个
                // 超过 360 的 `shift` 由它被交给的那个换算（[`crate::axes::axes_to_rgb`]）wrap 掉——一次折叠服务所有
                // 调用者，而不是每种写法一次。
                let applied = |index: usize, name: &str| match writing {
                    Writing::Set => keep(&reads, name, current[index]),
                    Writing::Shift => current[index] + step(&reads, name),
                    Writing::Scale => scale_toward(current[index], step(&reads, name), 0.0, 100.0),
                };
                // 它留下的不对称记在这里，而不是就地定死：下面那条轴分支在*存之前*折叠一个被移动的色相，所以一个
                // 超过一圈的和会原样到达 [`Color::with_hwb`]、却是折好地到达 [`Color::with_axes`]，而只有 `authored`——
                // out-of-gamut 的那次 hsl 打印——能把差别显示出来。没有任何一行能：没有一行给一个自身色相非零的颜色
                // 加上非零的色相（`adjust/hsl.hrx`/`adjust/hwb.hrx` 从 `red` 或 `black` 起步，而 `blue` 加的是 `0`），
                // 而 `color::text` 的 `out_of_gamut` 只按饱和度与亮度对上 authored 的三元组。
                colour
                    .with_hwb(
                        applied(0, "hue"),
                        applied(1, "whiteness"),
                        applied(2, "blackness"),
                    )
                    .ok_or_else(amount::decline)
            }
            (Space::Axes, _) => colour
                .with_axes(|[hue, saturation, lightness]| match writing {
                    // `change` 把它拿到的三元组原样写出、不夹紧，并为 out-of-gamut 的打印记住它（`change/hsl.hrx`）。
                    Writing::Set => [
                        keep(&reads, "hue", hue),
                        keep(&reads, "saturation", saturation),
                        keep(&reads, "lightness", lightness),
                    ],
                    // legacy 的夹紧不属于这里：`adjust/hsl.hrx::saturation/max_remaining` 保住
                    // `color.adjust(plum, $saturation: 53%)` 累出的 100.2868217054% 并把它按 hsl 打，而
                    // `saturate.hrx::two_args/max_remaining` 对同一次移动给出 `#ff7eff`——两个条目，因为两条相反的律
                    // 都被证实过（[`Color::transform_axes`] 是 legacy 那一条）。
                    //
                    // 不对称的界限，两边都出自行：被推到零以下的饱和度变成零饱和度那个灰
                    // （`…::saturation/{min,min_remaining}` → `rgb(74.7058823529%, …)`），被推到零以下的亮度保持负值并
                    // 按 hsl 打（`…::lightness/min` → `hsl(0, 100%, -50%)`）。色相 wrap，这是两个条目共享的那一条界限
                    // （`…::hue/arg_above_max` 把 540 读成 180）。
                    Writing::Shift => [
                        (hue + step(&reads, "hue")).rem_euclid(360.0),
                        (saturation + step(&reads, "saturation")).max(0.0),
                        lightness + step(&reads, "lightness"),
                    ],
                    // 朝 0 或 100 按点名的份额移动，它出不去 gamut——所以提不出夹紧问题，也没有任何一行 `scale` 打
                    // hsl（`scale/hsl.hrx::saturation/max` → `#ff7eff`）。
                    Writing::Scale => [
                        hue,
                        scale_toward(saturation, step(&reads, "saturation"), 0.0, 100.0),
                        scale_toward(lightness, step(&reads, "lightness"), 0.0, 100.0),
                    ],
                })
                .ok_or_else(amount::decline),
        },
    }?;
    // `adjust` 移动 alpha，而 `clamp_alpha` 把它留在 `0–1` 里，这正是
    // `color.adjust(black, $red: 12, $green: 24, $blue: 48, $alpha: -0.3)` → `rgba(12, 24, 48, 0.7)` 所示
    // （`adjust/rgb.hrx::alpha_arg`）；`scale` 把它朝符号指向的那一侧界限移动
    // （`color.scale(rgba(red, 0.5), $alpha: 14%)` → `0.57`，`scale/no_space.hrx::alpha/high`）；`change` 照给的值写，
    // 而它在读到的那一处已经把范围外的拒掉了。
    Ok(Value::Color(match (alpha, writing) {
        (Some(alpha), Writing::Set) => built.with_alpha(alpha),
        (Some(alpha), Writing::Shift) => built.with_alpha(built.alpha() + alpha),
        (Some(alpha), Writing::Scale) => {
            built.with_alpha(scale_toward(built.alpha(), alpha, 0.0, 1.0))
        }
        (None, _) => built,
    }))
}
