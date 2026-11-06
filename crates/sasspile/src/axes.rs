//! hsl 轴的算术，与 [`crate::color`] 分开住，因为它是 `f64` 进、`f64` 出的纯函数。
//!
//! 它住在自己的文件里而不是颜色类型内部，有两个理由：这个往返是 hsl 轴那条路线上唯一一处颜色离开自己
//! 空间的地方（自带空间的颜色走 `space.rs` 的 [`crate::color::Space`] 路线：`Color::converted`、
//! `Color::as_legacy_rgb`、`Color::as_legacy_hsl`，都不经过这里），而把它分开才能让*换算*对着金样被测试
//! 而不混进打印规则 —— 打印规则（整字节打数字或 hex，中间值打百分比）是另一条律，属于 [`crate::color`]。
//!
//! 公式是 CSS Color 4 的；下面的读数则是本地语料钉住的东西。作用域得写清楚：带 case key 的那些常数是
//! fixture 行；**不**是 fixture 行而是推导的有 `76.5`（= 30% × 255）、`127.5`（= 50% × 255）、
//! `1.01 / 1.41` 与 `0.7163120567…`（归一的算式）、`71.4285714286%`（夹到 100% 才会得到的反事实）与
//! `720`（两个已折色相之和的上界）—— 前四个在本仓代码里能被函数自身的算式复取，后两个只作为方向说明。

/// 给定三个 `0–255` 通道值时的 `[色相以度计, 饱和度以 % 计, 亮度以 % 计]`。
///
/// * `red` = `(255, 0, 0)` → `[0, 100, 50]` —— 那就是 `color.grayscale(red)` → `rgb(50%, 50%, 50%)` 背后的行
///   （`core_functions/color/grayscale.hrx`：亮度 50 活下来，饱和度去 0）。
/// * `#633736` = `(99, 55, 54)` → 亮度是 255 的 `30%`，即 76.5，这就是
///   `color.grayscale(#633736)` 打 `rgb(30%, 30%, 30%)` 的原因（`grayscale.hrx`）。
/// * `purple` = `(128, 0, 128)` → `[300, 100, 25.098039215686]`，即 `values/colors/equality.hrx` 里的等式
///   `purple == hsl(300, 100%, 25.098039215686%)`。
/// * 一个灰（`span == 0`）色相是 `0`、饱和度是 `0` —— `color.grayscale(#494949)` 仍是 `#494949`。
pub(crate) fn rgb_to_axes(red: f64, green: f64, blue: f64) -> [f64; 3] {
    let [red, green, blue] = [red / 255.0, green / 255.0, blue / 255.0];
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) / 2.0;
    let span = max - min;
    if span == 0.0 {
        return [0.0, 0.0, lightness * 100.0];
    }
    let saturation = span / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = match max {
        v if v == red => ((green - blue) / span).rem_euclid(6.0) * 60.0,
        v if v == green => ((blue - red) / span + 2.0) * 60.0,
        _ => ((red - green) / span + 4.0) * 60.0,
    };
    [hue, saturation * 100.0, lightness * 100.0]
}

/// `[色相以度计, 饱和度以 % 计, 亮度以 % 计]` 的那三个 `0–255` 通道值。
///
/// 有意不取整：`red` = `(255, 0, 0)` 的轴是 `[0, 100, 50]`，把饱和度归零后得到 `(127.5, 127.5, 127.5)`，
/// 而*那个*就是 `color.grayscale(red)` 打成 `rgb(50%, 50%, 50%)` 的那个读法
/// （`core_functions/color/grayscale.hrx::legacy/max_saturation`；`127.5` 是 50% × 255 的推导，不是归档行）。
/// 在这里取整会把一行百分比变成一行数字，并悄悄改掉输出。
pub(crate) fn axes_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let saturation = saturation / 100.0;
    let lightness = lightness / 100.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    // 色相是周期的，所以扇区由它在一圈*里的位置*定：一个超过 360 才到达这次换算的角，属于
    // `hue mod 360` 那个扇区，而拿原始商去匹配会把 450 送到最后一臂而不是 90 的那臂。语料在**三行**里
    // 用同一个答说出这个 wrap，它们都拿 540 答 `aqua`（`adjust/hsl.hrx::hue/arg_above_max`、
    // `change/hsl.hrx::hue/above_max`、`adjust_hue/above_max.hrx`），又在**三行**里用别的数说它
    // （`change/lch.hrx::hue/above_max` 的 400deg → 40deg、`change/lch.hrx::hue/negative` 的 -20deg → 340deg、
    // `change/oklch.hrx::hue/above_max` 同理；同名 key 在这些文件里值不同，所以都写全名）。它们全都在进来的
    // 路上由 `color::angle_of` 把角折过（它用 `rem_euclid(360.0)`），所以没有一个把**这个**函数需要的那个
    // 超圈值交进来：真会交进来的调用是 [`crate::builtins::color::modify`] 的 whiteness/blackness 分支，
    // 它把两个已经折过的色相相加，逼近 720 这个上界（严格小于 720）。
    let sector = (hue / 60.0).rem_euclid(6.0);
    // 六个扇区，每 `second` 两个：先把一圈折过来在这里不改变任何东西，因为 6 是 2 的倍数 ——
    // 对各调用者过去交进来的每个色相，那个读法都一样。
    let second = sector.rem_euclid(2.0) - 1.0;
    let intermediate = chroma * (1.0 - second.abs());
    let matched = match sector {
        h if h < 1.0 => [chroma, intermediate, 0.0],
        h if h < 2.0 => [intermediate, chroma, 0.0],
        h if h < 3.0 => [0.0, chroma, intermediate],
        h if h < 4.0 => [0.0, intermediate, chroma],
        h if h < 5.0 => [intermediate, 0.0, chroma],
        _ => [chroma, 0.0, intermediate],
    };
    let offset = lightness - chroma / 2.0;
    matched.map(|component| (component + offset) * 255.0)
}

/// 一个颜色在一对超出它的值之后**存下**的 whiteness 与 blackness：它们被自己的和缩放，而不是夹紧 ——
/// `hwb(0, 101%, 40%)` 是范围的 `1.01 / 1.41`，而夹到 100% 会在语料打 `71.6312056738%` 的地方打成
/// `71.4285714286%`（`hwb/four_args::whiteness/above_max`，它的 output.css 逐字是
/// `  b: hsla(0, 0%, 71.6312056738%, 0.5);`）。这条律只有一处住所，由 [`hwb_to_hsl`]、
/// [`Color::hwb`](crate::color::Color::hwb) 与 [`Color::with_hwb`](crate::color::Color::with_hwb) 共用 ——
/// 一对副本就是上次同样一条律被写下两遍时两边飘开的成因。
pub(crate) fn normalise_hwb(whiteness: f64, blackness: f64) -> (f64, f64) {
    if whiteness + blackness > 100.0 {
        let span = whiteness + blackness;
        (whiteness / span * 100.0, blackness / span * 100.0)
    } else {
        (whiteness, blackness)
    }
}

/// 一个 **whiteness/blackness** 颜色的 hsl 轴，形如 `[色相以度计, 饱和度以 % 计, 亮度以 % 计]`。
///
/// 三个读法，每一个都是 `core_functions/color/hwb/**` 的一行。中间那个正是“为什么这不是夹紧”的理由：
/// * `hwb(0deg, 30%, 40%)` → `[0, 33.3333333333, 45]`（`three_args/units::hue/deg`）—— whiteness *就是*
///   最低那个通道，而 `1 - blackness` 是最高那个，所以亮度是它们的均，饱和度则走与 [`rgb_to_axes`]
///   同一条规则；
/// * `hwb(0, 101%, 40%)` → `[0, 0, 71.6312056738]`（`four_args::whiteness/above_max`）—— 两个加起来超出该
///   颜色，而它们**按自己的和被归一**：`1.01 / 1.41 = 0.7163120567…`。要是先把 whiteness 夹到 100% 就会打
///   `71.4285714286%`，所以这一行分辨得出两种读法并选了这一个；
/// * `hwb(0, -1%, 40%)` → `[0, 103.3898305085, 29.5]`（`four_args::whiteness/below_min`）—— 一个负的
///   whiteness 也**不**夹；它把亮度推到 `29.5%`、把饱和度推过 100%。
pub(crate) fn hwb_to_hsl(hue: f64, whiteness: f64, blackness: f64) -> [f64; 3] {
    let (low, high) = normalise_hwb(whiteness, blackness);
    let (low, high) = (low / 100.0, high / 100.0);
    let (min, max) = (low, 1.0 - high);
    let lightness = (min + max) / 2.0;
    let span = max - min;
    // whiteness 与 blackness 把整个范围填满就留下了零 span：一个灰，它的饱和度是 0 而色相被保住，
    // 那正是上面 `hwb(0, 101%, 40%)` 打成 `hsl(0, 0%, …)` 的原因。
    let saturation = if span == 0.0 {
        0.0
    } else {
        span / (1.0 - (2.0 * lightness - 1.0).abs())
    };
    [hue, saturation * 100.0, lightness * 100.0]
}

/// 一个 `rgb` 空间颜色的 whiteness 与 blackness（`0–100`），给对它的一次 `hwb` 读法用。
///
/// 两个逐字调用各自整行写（同一个 key 拆到两行就 grep 不回归档原文）：
/// `color.channel(pink, "whiteness", $space: hwb)` → `75.2941176471%`；
/// `color.channel(pink, "blackness", $space: hwb)` → `0%`。它们的出处是 `channel/hwb.hrx` 的 `foreign/*`
/// 家族：**同文件三个 case**（hue / whiteness / blackness），而 `grep -c` 数到 6 个 marker 段是因为
/// 每个 case 都有 input 与 output 两段。`pink` 是 `(255, 192, 203)`，所以 whiteness 是最低那个通道
/// 除以 255，而 blackness 是最高那个通道离满值剩下的那部分。
pub(crate) fn rgb_to_hwb(red: f64, green: f64, blue: f64) -> [f64; 2] {
    let (red, green, blue) = (red / 255.0, green / 255.0, blue / 255.0);
    [
        red.min(green).min(blue) * 100.0,
        (1.0 - red.max(green).max(blue)) * 100.0,
    ]
}
