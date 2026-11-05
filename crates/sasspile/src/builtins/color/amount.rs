//! 读 `color.change` / `color.adjust` / `color.scale` 的**通道实参**、把一个实参变成一个写入值的算术，以及
//! 实参不可用时语料给的那句消息。从 [`super::modify`] 里拆出来，是因为那个文件讲的是*哪一个*通道被调用点名、
//! 以及怎么分派，而这个文件讲的是实参本身——它到底能不能被读（P7 的诊断住的那一半），以及它的量怎么与颜色
//! 已有的值合并（[`scale_toward`]、[`step`]、[`keep`]）。
//!
//! 这里的每一句措辞都是一行 case，而三族不共用一套措辞：
//! * `change` 说 `c is not a number or unquoted "none".`，因为不带引号的 `none` 在那里**有意义**——一个缺
//!   通道——而那一行的消息是这一刀还没有的另一条律
//!   （`core_functions/color/change/error/type.hrx::{red,quoted_none}`；`…/change/hwb.hrx::whiteness/none` 就
//!   是那个被豁免的值）；
//! * `adjust` 与 `scale` 对同一个值也说 `c is not a number.`
//!   （`core_functions/color/adjust/error/type.hrx::none` 是 `color.adjust(red, $alpha: none)` →
//!   `Error: $alpha: none is not a number.`，而 `scale/error/type.hrx` 对 `$alpha` 同样）；
//! * scale 的量必须带 `%`（`scale/error/units/rgb.hrx::none/red` 是 `$red: 1` →
//!   `Error: $red: Expected 1 to have unit "%".`），`adjust` 与 `change` 的 whiteness、blackness 也必须
//!   （`adjust/error/units/hwb.hrx::none/whiteness`）——而其余每个 legacy 通道量都取它所带单位的数值
//!   （`change/hsl.hrx::units/*`：`60deg`、`60` 与 `60in` 都算 60；这些行带一条 harness 不比较的 deprecation
//!   warning，所以对得上它们的输出不等于支持 deprecation）。

use super::modify::Writing;
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::Given;
use crate::color::{Color, Space};
use crate::value::Value;

/// 这一刀已经做出的拒答，理由语料没有回答——读不了的颜色、它换算不了的 `$space:`、同时点名两个空间。
/// 不致命：那次调用保住自己的文本。
pub(super) fn decline() -> SassError {
    SassError::new("a colour or channel this slice cannot change")
}

/// 一个轴（或 alpha）的实参。hue 会把角度换算并折叠（`color.change(red, $hue: 60rad)` →
/// `rgb(0%, 70.4220486918%, 100%)`，`change/hsl.hrx::units/hue/angle`；`$hue: 540` → `aqua`），其余每个轴都取
/// 它所带单位的数值——除了要求 `%` 的地方，也就是 scale 用的 [`share`] 与下面那条 whiteness/blackness 分支。
///
/// hue 上带 `%` 只在 `lch` 与 `oklch` 颜色上被证实过，句子是
/// `Expected 20% to have an angle unit (deg, grad, rad, turn).`（`adjust/error/units/lch.hrx`、`…/oklch.hrx`）。
/// 但“这一刀不读这两种空间”只对 `adjust`/`scale` 成立：它们在自带空间上仍 decline，所以这个读法不认领那句
/// 消息；`change` 一侧是由 [`written_channel`] 的 hue 分支答这句的（金样 `change/error/units/lch.hrx::hue`、
/// `…/oklch.hrx::hue`）。
pub(super) fn axis(given: &[Given], name: &str, writing: Writing) -> Result<f64, SassError> {
    let value = Given::find(given, name).ok_or_else(decline)?;
    if name == "hue" {
        // 不带引号的 `none` 又是那个缺通道的情形，不是类型错误：
        // `color.change(hsl(0deg 100% 50%), $hue: none)` 期望的输出是 `hsl(none 100% 50%)`，所以大声拒掉它会把
        // 一条错值行变成一条意外错误行。
        return crate::color::angle_of(value).ok_or_else(|| {
            if crate::color::is_missing_word(value) {
                decline()
            } else {
                not_a_number(name, value, writing)
            }
        });
    }
    if writing == Writing::Scale {
        return share(name, value);
    }
    let magnitude = number(name, value, writing)?;
    // 按语料自己的说法，whiteness 与 blackness 是百分比，对 `adjust` 与 `change` 也一样：
    // `color.adjust(white, $whiteness: 1)` → `Error: $whiteness: Expected 1 to have unit "%".`
    // （`adjust/error/units/hwb.hrx::none/whiteness`，`change` 里是同一个文件）。
    if matches!(name, "whiteness" | "blackness") && !is_percent(value) {
        return Err(expected_percent(name, value));
    }
    Ok(magnitude)
}

/// 一个 scale 的量：它是到界限还剩那段距离的一个**份额**，所以必须带 `%` 并停在 `-100%` 与 `100%` 之间
/// （单位那一侧见 `scale/error/units/rgb.hrx::{none,wrong}/*`；范围那一侧是
/// `color.scale(red, $saturation: -100.001%)` →
/// `Error: $saturation: Expected -100.001% to be within -100% and 100%.`，即
/// `scale/error/bounds.hrx::legacy/too_low`，它的姊妹行 `too_high` 用的是 `$whiteness: 100.001%`）。
pub(super) fn share(name: &str, value: &Value) -> Result<f64, SassError> {
    let magnitude = number(name, value, Writing::Scale)?;
    if !is_percent(value) {
        return Err(expected_percent(name, value));
    }
    if !(-100.0..=100.0).contains(&magnitude) {
        return Err(SassError::typed(
            ErrorKind::Channel,
            format!(
                "${name}: Expected {} to be within -100% and 100%.",
                value.to_css()
            ),
        ));
    }
    Ok(magnitude)
}

/// `adjust` 写一个 `rgb` 通道时用的实参：一个 `0–255` 的无单位数字（`color.adjust(#abcdef, $red: 200)`，
/// `adjust/rgb.hrx::red/above_max`）。
///
/// 百分比那一条臂**在 legacy 字节侧没有 fixture**（现量：`adjust/rgb.hrx` 里没有 `[0-9]%`，
/// `adjust/units.hrx` 里没有任何 `$red`/`$green`/`$blue` 行）；带 `%` 的字节通道只出现在自带空间那侧
/// （如 `adjust/display_p3.hrx:80` 的 `color.adjust(color(display-p3 0.2 0.5 0.7), $green: 40%)`），而那一侧走
/// [`written_channel`]，不走这里。
///
/// 它答不了一个 `none`：`change` 的缺席走 [`written_byte`]，而这个读法只从 shift 分支（`writing == Shift`）
/// 加上 [`written_byte`] 那处已经守住的下落被触到——所以这里的 `none` 就像任何其他用不了的值一样，得到语料
/// 的类型消息。这个函数早先的形状里有一个 `Set`+`none` 臂，它会拒掉缺通道那个情形；等 [`written_byte`] 到位之后
/// 它已经不可达，所以两者都删了，而不是留成一个问题的第二个答案。
pub(super) fn byte(value: &Value, name: &str, writing: Writing) -> Result<f64, SassError> {
    match value {
        Value::Number(number, unit) if unit.single() == Some("%") => Ok(number * 255.0 / 100.0),
        Value::Number(number, _) => Ok(*number),
        // 其余任何东西——一个 `px` 量、那个关键词 `none`——都没有可写的字节；`c` 是语料的类型消息，而这个词本身
        // 由 `crate::color::is_missing_word` 判，那是本 crate 唯一的一个答案。
        _ => Err(not_a_number(name, value, writing)),
    }
}

/// 一个数字实参的数值，或者那个族自己对「不是数字的值」说的话。
///
/// **这里的 `Set` + `none` 臂是活的，而且与字节那一侧意思不同**：这个读法服务从 `axis` 触到的轴与 alpha 实参
/// （`hue`、`saturation`、`lightness`、`whiteness`、`blackness`、`alpha`），而 `hsl`/`hwb` 的轴三元组与 `Color`
/// 的 `alpha` 都还带不了缺席——所以 `color.change(hsl(0deg 100% 50%), $hue: none)`（`change/hsl.hrx::hue/none`）
/// 与 `color.change(hwb(180deg 30% 50%), $whiteness: none)`（`change/hwb.hrx::whiteness/none`）保住自己的文本，
/// `color.adjust(red, $alpha: none)` → `$alpha: none is not a number.` 也一样
/// （`adjust/error/type.hrx::none`、`scale/error/type.hrx::none`）。decline 是回答的*缺席*，不是归档那句消息
/// ——把缺席写进轴是 `tasks.md` 9.25 剩下的那五行。
fn number(name: &str, value: &Value, writing: Writing) -> Result<f64, SassError> {
    match value {
        Value::Number(number, _) => Ok(*number),
        _ if crate::color::is_missing_word(value) && writing == Writing::Set => Err(decline()),
        _ => Err(not_a_number(name, value, writing)),
    }
}

fn is_percent(value: &Value) -> bool {
    matches!(value, Value::Number(_, unit) if unit.single() == Some("%"))
}

/// `change` 族的字节量，它**可以是缺席**：`$red: none` 写一个缺通道，而语料三种都有——
/// `color.change(black, $red: none)` → `rgb(none 0 0)`、`$green: none` → `rgb(0 none 0)`、`$blue: none` →
/// `rgb(0 0 none)`（`change/rgb.hrx::{red,green,blue}/none`）。
///
/// `adjust` 与 `scale` 从来不走这里。它们的字节读法是 [`byte`]，如今对 `none` 一个情形都没有，也没有任何
/// fixture 把一个 `none` 当作**通道**实参交给 shift 或 scale
/// （`grep -rEn "color\\.(adjust|scale)\\([^)]*\\\$[a-z-]+: none" sass-spec/spec/core_functions/color/` 只
/// 命中那两行 `$alpha`（按 case 计），那是 alpha 读法的事——见 [`number`] 的注）。用两个读法而不是一个带标志位的，
/// 是因为“写缺席”与“根本没有这种量”是两条不同的律。
pub(super) fn written_byte(value: &Value, name: &str) -> Result<Option<f64>, SassError> {
    if crate::color::is_missing_word(value) {
        return Ok(None);
    }
    byte(value, name, Writing::Set).map(Some)
}

/// `color.change` 作用在一个**自带空间**的颜色上时的一个通道实参，按那个空间存下来的尺度读——或者 `None`，
/// 那正是归档的 `none`（`change/lch.hrx::chroma/none` → `lch(50% none 30deg)`、
/// `change/display_p3.hrx::red/none` → `color(display-p3 none 0.5 0.7)`）。
///
/// 下面四条是取值读法，各有自己的 fixture（最后一条不是读法，而是拒答）：
/// * 一个百分比除以**那个通道自己的**底数——`change/srgb.hrx::red/percent`（`50%` → `0.5`）与
///   `change/lab.hrx::a/percent`（`-40%` → `-50`）是同一类实参落在不同底数上，这就是
///   [`crate::color::Space::percent_divisor`] 逐通道的原因；
/// * 任何**其他单位**都点名拒收：`color.change(color(srgb 0.2 0.5 0.7), $red: 0.5px)` →
///   `$red: Expected 0.5px to have unit "%" or no units.`（`change/error/units/srgb.hrx::red`，这句措辞归档在
///   `change/error/units/` 下用了 37 次）。把 `0.5px` 读成数值 `0.5` 是这个臂最初的形状，它把 27 行归档叫错误的东西
///   答掉了；
/// * 一个 hue 是**角度**，折进一圈：`change/lch.hrx::hue/negative` 把 `-20deg` 读成 `340deg`，而
///   `…::hue/above_max` 把 `400deg` 读成 `40deg`——用的就是 `hsl` 色轴那个 [`crate::color::angle_of`]。它上面带
///   百分比时是语料那句角度错误（`change/error/units/lch.hrx::hue`，`$hue: 20%` →
///   `Expected 20% to have an angle unit (deg, grad, rad, turn).`），这也正是
///   [`crate::color::Space::percent_divisor`] 故意留空的那一格；
/// * 其余一切是照写的数值，**不夹紧**：`change/xyz.hrx::x/out_of_range` 保住给它的那个 `1.2`，而
///   `change/oklab.hrx::a/out_of_range` 保住 `1`；
/// * 而一个根本不是数字的值是**decline**，不用类型消息回答它，理由写在这个函数的函数体上。
pub(super) fn written_channel(
    value: &Value,
    name: &str,
    space: Space,
    channel: usize,
) -> Result<Option<f64>, SassError> {
    if crate::color::is_missing_word(value) {
        return Ok(None);
    }
    // 一个既不是数字也不是 `none` 的值是**decline**，不给一句消息作答：每一条 `change/error/type.hrx` 行都是一个
    // legacy 颜色，所以没有任何 fixture 说一个*空间*颜色对 `c` 该答什么。在这里抬起那句类型消息（这个臂最初就
    // 这么干）把三行变成了归档从没要求过的错误（`change/unusual_numbers::{nan,infinity,negative_infinity}/linear`，
    // 其中 `color.change(lab(50% 0 0), $a: calc(nan))` 要的是 `calc()` 求值之后把颜色原样还回去），并把意外
    // 错误计数从 19 抬到 22；把它量回来的是这道 decline。
    let (magnitude, units) = match value {
        Value::Number(magnitude, units) => (*magnitude, units),
        _ => return Err(decline()),
    };
    let unit = units.single();
    if space.is_hue(channel) {
        // 一个 hue 按角度读，由 `hsl` 色轴用的同一个 [`crate::color::angle_of`] 折进一圈
        // （`change/lch.hrx::hue/{negative,above_max}`）；任何不是角度的东西——包括百分比——就是语料那句角度消息
        // （`change/error/units/lch.hrx::hue`）。
        let angle = match unit {
            None => true,
            Some(unit) => crate::unit::is_angle(unit),
        };
        if !angle {
            return Err(expected_angle(name, value));
        }
        return crate::color::angle_of(value)
            .map(Some)
            // 对一个数字来说不可达：上面的 match 已经把它收成一种形状。
            .ok_or_else(decline);
    }
    // 一个既不是 `%` 也不是完全无单位的单位会被点名拒收：那句措辞在 `change/error/units/*.hrx` 里出现 37 次，而
    // 这个臂最初的形状把其中的 27 条用一个值答掉了（可复取：
    // `cargo run -q -p ledger --bin shift -- change-writes-absence-v4 change-in-carried-space` 打
    // `error_expectation=27`，同一对打到 `change-in-carried-space-v6` 则是 0）。
    if unit.is_some_and(|unit| unit != "%") {
        return Err(expected_percent_or_none(name, value));
    }
    if unit != Some("%") {
        return Ok(Some(magnitude));
    }
    space
        .percent_divisor(channel)
        .map(|divisor| Some(magnitude / divisor))
        // 在这个形状里不可达：底数被留空的那个通道只有 hue，而上面的 hue 分支已经用语料的角度错误答了 `%`。
        // 它保留成 decline 而不是编一个数字，是为了让将来一行“hue 上带百分比”不能悄悄变成算术。
        .ok_or_else(decline)
}

/// 把这次调用**在颜色自己的空间里**点名的每个通道都写一遍，按左到右的顺序，所以后面的名字盖不掉前面的：
/// `color.change(color(srgb 0.2 0.5 0.7), $red: 0.7, $green: 0.4, $blue: 0.2)` → `color(srgb 0.7 0.4 0.2)`
/// （`change/srgb.hrx::all`）。而 `change/lab.hrx::all` 的输入是
/// `color.change(lab(50% 20 -30), $lightness: 20%, $a: -30, $b: 40)`，打的是 `lab(20% -30 40)`——输入的三元组被
/// 逐通道替掉，不是原样保住。
///
/// 只有 `change` 用：读法是 [`written_channel`]，而“一个名字不属于这个空间时保住自己的文本”那几条臂记在
/// `tasks.md` 9.25 里，不用这一刀还没读到的错误去答。
pub(super) fn write_carried(
    colour: &Color,
    given: &[Given],
    space: Space,
    owned: &[(&str, usize)],
) -> Result<Color, SassError> {
    owned
        .iter()
        .try_fold(colour.clone(), |colour, (name, channel)| {
            let argument = Given::find(given, name).ok_or_else(decline)?;
            let value = written_channel(argument, name, space, *channel)?;
            colour
                .with_space_channel(*channel, value)
                .ok_or_else(decline)
        })
}

/// 一个 scale 要写的值：`amount` 按“到符号所指那一侧界限还剩那段距离的百分比”读，所以结果总在 `floor` 与
/// `ceiling` 之间。两条都由 `scale/rgb.hrx` 的金样定下：`::red/low` 是
/// `color.scale(lightcoral, $red: -33%)` → `rgb(63.0588235294%, …)`，即
/// `scale_toward(240, -33.0, 0.0, 255.0)` = 160.8 = 63.0588235294% × 255；`::red/high` 是
/// `color.scale(turquoise, $red: 86%)` → `rgb(89.5137254902%, …)`，即
/// `scale_toward(64, 86.0, 0.0, 255.0)` = 228.26 = 89.5137254902% × 255。
pub(super) fn scale_toward(current: f64, amount: f64, floor: f64, ceiling: f64) -> f64 {
    let share = amount / 100.0;
    if share >= 0.0 {
        current + (ceiling - current) * share
    } else {
        current - (current - floor) * -share
    }
}

/// 一次调用为某个轴点名的偏移，没点名时为零——`adjust` 会把这些加起来，`change` 从不。
pub(super) fn step(reads: &[(&str, f64)], name: &str) -> f64 {
    reads
        .iter()
        .find(|(read, _)| *read == name)
        .map_or(0.0, |(_, value)| *value)
}

/// 某个轴要写的值，或者当调用没点名它时颜色当前的那个。
pub(super) fn keep(writes: &[(&str, f64)], name: &str, current: f64) -> f64 {
    writes
        .iter()
        .find(|(written, _)| *written == name)
        .map_or(current, |(_, value)| *value)
}

/// 每个 **legacy 组**会答哪些通道名：它自己的名字、那些**决定**它的名字，以及它拥有的名字。这张表不是 Color-4
/// 那张——一个带着 `srgb`/`xyz`/`lch` 的颜色在 `crate::color::channels` 里说自己那些名字，而这里的 `rgb` 行
/// 恰好拼出同样三个词，因为 legacy 字节三元组*就是*一个 srgb 三元组（`change/rgb.hrx` 与 `change/srgb.hrx`
/// 一样用 `$red`）。*只属于这张表*的是中间那一列——哪些名字决定这个组——而那才是 `mixed_channel` 报的那条律。
///
/// `rgb` 只拥有它的三个字节：`color.change(red, $red: 1, $hue: 1)` 会按
/// `$hue: Color space rgb doesn't have a channel with this name.` 被拒
/// （`change/error/mixed_formats.hrx::red_and_hue`），所以色相**不是**字节空间的一个
/// 通道，尽管 `change/hsl.hrx::all` 把它写在轴中间——一个名字属于一个空间，不属于一次调用。
///
/// 色相什么都不决定，因为 `hsl` 与 `hwb` 都拥有它（`change/hsl.hrx::hue/*` 把它按 hsl 写，
/// `change/hwb.hrx::all` 把它与 whiteness、blackness 一起写，同形的另三行是 `…::{alpha_input,alpha_arg,named}`，
/// 输入都是 `color.change(blue, $hue: 150, $whiteness: 20%, $blackness: 40%)` 一族）。让一个共享名字去挑空间是我
/// 在这里的第一版尝试，它把那种调用按
/// `$whiteness: Color space hsl doesn't have a channel with this name.` 拒掉——那是错的拒法，因为两个轴组同属
/// 一个颜色时该由 `hwb` 接。
const SPACE_CHANNELS: [(&str, &[&str], &[&str]); 3] = [
    ("rgb", &["red", "green", "blue"], &["red", "green", "blue"]),
    (
        "hsl",
        &["saturation", "lightness"],
        &["hue", "saturation", "lightness"],
    ),
    (
        "hwb",
        &["whiteness", "blackness"],
        &["hue", "whiteness", "blackness"],
    ),
];

/// 语料在一次调用点名**两个空间**的通道时的拒语：那个决定性的组说出这次调用在写哪个空间，而来自另一个空间的
/// 第一个名字被点名拒收（`{change,adjust,scale}/error/mixed_formats.hrx` —— 21 行，每行点名一对）。
///
/// 这条规则不覆盖、并且继续 decline 的：一个任何空间都不拥有的名字
/// （`color.adjust(red, $chroma: 50%)` —— `…/incompatible_channel.hrx::legacy_space`），以及一个对**现代**颜色
/// 陌生的名字（`color.adjust(color(srgb 0.2 0.5 0.7), $whiteness: 50%)` →
/// `Color space srgb doesn't have a channel with this name.`，`…::legacy_channel`），因为那个颜色在这里根本读不了
/// ——报出它的空间等于声称一个。
///
/// `$alpha:` 从不出现在这里：它不是一个通道，而每个族都会把它与任意空间一起写（`scale/no_space.hrx::alpha/*`）。
/// 两个陌生的名字里先报哪一个跟着签名顺序，因为没有任何一行同时点名三个组。
pub(super) fn mixed_channel(named: &[&str]) -> Option<SassError> {
    let (space, _, own) = SPACE_CHANNELS
        .into_iter()
        .find(|(_, decides, _)| named.iter().any(|name| decides.contains(name)))?;
    let foreign = named
        .iter()
        .find(|name| !own.contains(name))
        .copied()
        .unwrap_or_default();
    (!foreign.is_empty()).then(|| {
        SassError::typed(
            ErrorKind::Channel,
            format!("${foreign}: Color space {space} doesn't have a channel with this name."),
        )
    })
}

/// `${parameter}: Expected ${value} to have unit "%".`，值以 CSS 形式渲染，所以 `1` 与 `1px` 被读成语料写的样子
/// （`scale/error/units/rgb.hrx`、`…/hsl.hrx`、`…/hwb.hrx`）。
fn expected_percent(name: &str, value: &Value) -> SassError {
    SassError::typed(
        ErrorKind::WrongUnits,
        format!("${name}: Expected {} to have unit \"%\".", value.to_css()),
    )
}

/// 与 [`expected_percent`] 同一个要求，只是用于一个也可以裸写的通道：对自带空间的颜色，`change` 只拒一个既不是
/// `%` 也不是无单位的单位——`color.change(color(srgb 0.2 0.5 0.7), $red: 0.5px)` →
/// `$red: Expected 0.5px to have unit "%" or no units.`。现量：`change/error/units/` 下这句共 37 次，除 `hwb.hrx`
/// 以外每个空间的文件都有它（`hwb` 走 [`expected_percent`] 那句不带 “or no units” 的措辞；`lch`/`oklch` 各少的一行
/// 是 hue 的角度错误）。
fn expected_percent_or_none(name: &str, value: &Value) -> SassError {
    SassError::typed(
        ErrorKind::WrongUnits,
        format!(
            "${name}: Expected {} to have unit \"%\" or no units.",
            value.to_css()
        ),
    )
}

/// 一个 hue 只收角度，所以连单位那句抱怨的*形状*都不同：`color.change(lch(50% 30 50deg), $hue: 20%)` →
/// `$hue: Expected 20% to have an angle unit (deg, grad, rad, turn).`（`change/error/units/lch.hrx::hue`，
/// 以及 `oklch` 的孪生行）。
fn expected_angle(name: &str, value: &Value) -> SassError {
    SassError::typed(
        ErrorKind::WrongUnits,
        format!(
            "${name}: Expected {} to have an angle unit (deg, grad, rad, turn).",
            value.to_css()
        ),
    )
}

/// 类型消息，用发出调用的那个族自己的措辞：`change` 会点名它也接受的那个 `none`，并保住带引号 `"none"` 的引号
/// （`change/error/type.hrx::quoted_none` → `Error: $alpha: "none" is not a number or unquoted "none".`）。
fn not_a_number(name: &str, value: &Value, writing: Writing) -> SassError {
    let message = match writing {
        Writing::Set => format!(
            "${name}: {} is not a number or unquoted \"none\".",
            value.to_css()
        ),
        Writing::Shift | Writing::Scale => {
            format!("${name}: {} is not a number.", value.to_css())
        }
    };
    SassError::typed(ErrorKind::NotANumber, message)
}
