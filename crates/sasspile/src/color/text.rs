//! 一个 [`Color`] 如何变成 CSS 文本 —— 表示的律，与 [`super`] 里的数据与轴算术分开，因为它是语料最常反驳的那一半。
//!
//! 规则，各自带 fixture：
//! * 一个颜色保住它被创建时的形体：无单位通道 `rgb(18, 52, 86)`、任意一处有百分比 `rgb(7.1%, 20.4%, 33.9%)`、
//!   hex `#112233`、hsl `hsl(60, 100%, 50%)`；
//! * **取整属于打印，不属于换算**：通道不是整字节的那个颜色打百分比（`color.grayscale(#633736)` →
//!   `rgb(30%, 30%, 30%)`），整字节的打 hex 或数字（`color.grayscale(#494949)` → `#494949`、
//!   `darken(rgba(red, 0.2), 100%)` → `rgba(0, 0, 0, 0.2)`）；
//! * hex 造出的颜色在不透明时打 hex，一旦带透明度就改函数形（`rgb(#123, 0.5)` → `rgba(17, 34, 51, 0.5)`）；
//!   **本文件的颜色打印机（hex 臂）从不打八位 hex** —— 限定很重要：归档里确实有八位串（如
//!   `core_functions/color/ie_hex_str.hrx` 打 `#FFDADDEE`），但那是 `ie-hex-str()` 的输出，而那个函数本编译器
//!   还未实现（现量：`grep -rn "ie.hex.str" crates/sasspile/src` 只命中本注释自己；同一个量法查已实现的
//!   `"grayscale"` 则命中 `builtins/color/mod.rs` 的登记表）；
//! * 百分比用 Sass 自己的数字形（`173` 写出 `67.8431372549%`）。

use super::{Channel, Color, Representation, Space};

/// 每一个 CSS 颜色关键词，**只以拼写形式存在**。
///
/// 这里不写任何 RGB 值：每个拼写都经 `color::parse_color` 解出，所以这张表里的错字表现为一个解析不出来的名字
/// （由 `tests/color.rs` 钉住），而不是一个错颜色；少一个拼写则降级为打 hex —— 那是安全的方向。这张表是反向
/// 查找所需的，因为语料按**颜色是怎么造出来的**区分它们：
/// * `lighten(red, 0%)` → `red`、`grayscale(white)` → `white`、`desaturate(plum, 0%)` → `plum`；
/// * `complement(red)` → `aqua`（一个来自关键词的颜色被移到另一个关键词的字节上，就拿那个名字），而
///   `complement(turquoise)` → `#e04050`（移到不命名的字节上）打 hex；
/// * 而一个从 `rgb(0, 0, 0)` 造出的颜色打 `rgb(0, 0, 0)` —— **不是** `black`，在 `core_functions/color/rgb/**` 下
///   六个 fixture 里如此，那正是排除“对所有颜色只做按值的查找”的依据。
const NAMES: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

impl Color {
    /// 一个字节三元组所代表的关键词，经供出那些数字的同一个依赖解出来。不做缓存：查找表就是共享可变态，
    /// 而一次编译里颜色只打几次，所以这个扫描是更便宜的纪律。
    fn keyword(bytes: [u32; 3]) -> Option<&'static str> {
        NAMES.iter().copied().find(|name| {
            color::parse_color(name)
                .map(|colour| {
                    let rgb = colour.to_alpha_color::<color::Srgb>().to_rgba8();
                    [u32::from(rgb.r), u32::from(rgb.g), u32::from(rgb.b)] == bytes
                })
                .unwrap_or(false)
        })
    }

    /// CSS 文本。
    pub fn to_css(&self) -> String {
        // 一个自带色彩空间的颜色**就在那个空间里打，不在别处**：下面每一条律都是 srgb 时代的东西，让它们去定
        // 一个 `lab` 或 `display-p3` 的颜色，等于回答一个语料从未问过的问题。它是这个 `match` 的一臂而不是上面的
        // 一道守卫，这样新增一个表示就不会悄悄落到一个本不该认识它的打印机上。形体由
        // `core_functions/color/to_space/srgb/*.hrx` 钉住：`color(display-p3 …)`、`lab(44.1210142293% … )`、
        // `lch(… 280.5032243387deg)`、一个 `/` 后的 alpha、powerless 通道用 `none`。
        let byte = |value: u32| format!("{value:02x}");
        match self.representation {
            Representation::InSpace(carried) => print(carried.space, carried.channels, self.alpha),
            // **带未书写通道的颜色打空格分隔的形**，因为 legacy 的逗号形没有 `none` 这个拼法：
            // `color.to-space(rgb(none 20 30), rgb)` 答 `rgb(none 20 30)`（`to_space/rgb/rgb.hrx::missing/red`），
            // 而同样的名字与分隔符加一个 `/` 后的 alpha，正是语料对 `color.change(red, $alpha: none)` →
            // `rgb(255 0 0 / none)`（`change/no_space.hrx::alpha/none`）所打的 —— 那一行留下的是 **alpha** 未书写
            // 而不是某个通道，所以它佐证的是形体与那个 `/`。每个已书写的通道保住它进来时那个尺度。
            // **在这条臂负责的那族（四个存字节的表示）里，没有任何 fixture 把一个未书写通道与 alpha 或与一个
            // gamut 外的孪生值配对**，所以那两个组合是推广；它只覆盖这条臂 —— 自带空间那一族里确实有携 alpha
            // 的缺通道（`core_functions/color/color/alpha.hrx::none/slash/blue` 写 `color(srgb 0.1 0.2 none / 0.4)`），
            // 但那一族由 [`print`] 打，不走这里。alpha 那半边在 `tasks.md` **9.24** 按
            // “半边有旁证、半边无行”记录；gamut 那半边本批现量（口径：`output.css` 段里以 `rgb(`/`rgba(` 开头且含通道
            // `none` 的值，其中任一分量 > 255 的行 = **0**；阳性对照：同一个判据喂人造串 `rgb(none 0 500)` 会响），
            // 未另立记录，不得当作已立项。注意另有一堆 `to_gamut/space.hrx::clip/*` 的 `rgb(none 255 none)` 形：
            // 那是调用点写了域外值、打印前已夹回 255，不反驳本句（本句按打印出来的值判）。它排在 out-of-gamut
            // 那臂之上，因为那一臂会用调用者从未写下的那个 `0` 重推 hsl 轴来回答 `none`。
            representation if representation.writes_bytes() && self.has_unwritten_channel() => {
                self.modern()
            }
            // **一个 gamut 外的颜色打 hsl 形**，不论它在哪个空间里被创建：`color.change(black, $red: 500)` →
            // `hsl(0, 5000%, 98.0392156863%)`，`color.change(black, $green: -50)` →
            // `hsl(120, 100%, -9.8039215686%)`（`core_functions/color/change/rgb.hrx::{red,green}/out_of_range`）。
            // 字节进来的路上保持不取整 —— 这就是保住 `grayscale(red)` 是一行百分比的同一条律，只是从另一侧看：
            // 在这里取整或夹紧会回答另一个问题。
            representation if representation.writes_bytes() && self.out_of_gamut() => {
                // 有的话就是调用者点名的那个三元组：从字节去推它，会在蕴含的通道离开 gamut 时旋转色相并翻掉
                // 饱和度。写成 `match` 而不是 `unwrap_or_else`，因为铁律闸说得对：一个值路径里的 fallback 值得被
                // 看一眼，而这里的 fallback 是一次重推，不是一次被吞下的失败。
                match self.authored {
                    Some(axes) => self.hsl_text(axes),
                    // 这一臂只有四个存字节的表示会走到（空间色在上面那臂就退了），所以字节律在这里就是全部读法 ——
                    // 没有 `None` 会被说过去。
                    None => self.hsl_text(self.axes()),
                }
            }
            Representation::Hex => {
                // 通道不是整字节的颜色打百分比，不打 hex 也不打数字：`grayscale(#633736)` → `rgb(30%, 30%, 30%)`，
                // 而 `grayscale(#494949)` → `#494949`（`core_functions/color/grayscale.hrx`）。
                if self.fractional() {
                    return self.percent_form();
                }
                // 不透明的颜色以 hex 回来，半透明的取 legacy 的函数形：`rgb(#123, 1)` → `#112233`，而
                // `rgb(#123, 0.5)` → `rgba(17, 34, 51, 0.5)`（`rgb/two_args::opaque_to/{opaque,partial}`）——
                // 这里从不打八位 hex。
                let bytes = self.channels.map(|channel| channel.byte().round() as u32);
                if self.alpha < 1.0 {
                    return self.legacy(self.channels, false);
                }
                format!("#{}{}{}", byte(bytes[0]), byte(bytes[1]), byte(bytes[2]))
            }
            Representation::Legacy => {
                if self.fractional() {
                    return self.percent_form();
                }
                self.legacy(self.channels, false)
            }
            Representation::Percent => self.percent_form(),
            // 一个由关键词造出的颜色：还能命名时就是那个关键词，否则用 hex 颜色同样的那些退路
            // （`complement(turquoise)` → `#e04050`、`desaturate(plum, 100%)` → 百分比、
            // `rgb(transparent, 0.5)` → `rgba(0, 0, 0, 0.5)`）。
            Representation::Named => {
                if self.fractional() {
                    return self.percent_form();
                }
                let bytes = self.channels.map(|channel| channel.byte().round() as u32);
                if self.alpha < 1.0 {
                    return self.legacy(self.channels, false);
                }
                match Self::keyword(bytes) {
                    Some(name) => name.to_string(),
                    None => format!("#{}{}{}", byte(bytes[0]), byte(bytes[1]), byte(bytes[2])),
                }
            }
            // 色相是一个裸数字，另两个永远带 `%`；名字仍严格按 alpha 定，正如 `hsl/four_args/alpha.hrx` 那些行
            // 所示（alpha `100%` → `hsl(…)`，`0%` → `hsla(…, 0)`）。
            Representation::Hsl => self.hsl_text(self.stored_axes()),
            // 一个 `hwb` 颜色按语料给它的 hsl 形打：`hwb(0deg, 30%, 40%)` → `hsl(0, 33.3333333333%, 45%)`
            // （`hwb/three_args/units::hue/deg`），而值继续保留 whiteness 与 blackness 供
            // `color.channel(.., $space: hwb)` 读回。
            //
            // **一个字节为整、且字节能命名一个关键词的 `hwb` 颜色，打那个关键词**：`color.hwb(0, 0%, 0%, 100%)` →
            // `red`，不是 `hsl(0, 100%, 50%)`（`hwb/four_args::alpha/percent/{max,above_max}`）。那就是
            // [`Representation::Named`] 所循的反向命名律，而它*不是*一个 `rgb` 调用做的事（`rgb(0, 0, 0)` 在六个
            // fixture 里打它自己）—— 两个起源为何不同，语料里没有哪一处说，所以这里按 fixture 逐条记录而不作推广，
            // 而 `mix`/`adjust` 的结果保住它们自己的起源，不去承继它。
            Representation::Hwb => {
                let axes = self.hwb_axes();
                let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
                let whole = rgb.iter().all(|value| value.fract() == 0.0);
                let bytes = rgb.map(|value| value.round() as u32);
                match (self.alpha >= 1.0 && whole)
                    .then_some(bytes)
                    .and_then(Self::keyword)
                {
                    Some(name) => name.to_string(),
                    None => self.hsl_text(axes),
                }
            }
        }
    }

    /// 这个颜色是否落在 gamut 之外，而那会把一个 `rgb` 族的颜色送到 hsl 形：
    /// `color.change(black, $red: 500)` → `hsl(0, 5000%, 98.0392156863%)` 与
    /// `color.change(red, $lightness: 120%)` → `hsl(0, 100%, 120%)`（`change/rgb.hrx::red/out_of_range`、
    /// `change/hsl.hrx::lightness/above_range`）。
    ///
    /// 一个轴被某次调用**写过**的颜色，就在那些轴上量，因为那才是调用点名的东西：
    /// `color.adjust(plum, $saturation: 53%)` 加到 100.2868217054% 而打 hsl
    /// （`adjust/hsl.hrx::saturation/max_remaining`），尽管它蕴含的那个字节（算出的 255.19，不是归档文本）取整
    /// 仍会回到 `0–255` 内。色相不参与 —— 它被读时已折进 `0–360`，而没有 fixture 把一个角当 gamut
    /// （现量口径：`output.css` 段里 hsl 第一项大于 360 的行 = 0；阳性对照：同一管线改阈值为 100 交出 350 行）。
    ///
    /// 浮尘的上界是这里能印出的最后一位（第十位小数的半步，即 [`Channel::whole`] 用的那个容差），而不是半个字节：
    /// `color.change(red, $lightness: 0.5%)` 蕴含一个（算出的，非归档文本）`-1.1e-15` 的蓝，而语料打
    /// `rgb(1%, 0%, 0%)`（**`change/hsl.hrx::lightness/fraction`** —— 注意 `adjust/hsl.hrx` 里同名 key 打的是
    /// `rgb(100%, 1%, 1%)`，两个归档不通用，所以这里不用省略号）；要是在比较前先取整，也会把上面的 255.19 吞掉。
    ///
    /// 低端在两个轴之间**不对称**，两边都由行说出：饱和度低于零仍是一个打自己字节的颜色
    /// （`color.change(plum, $saturation: -20%)` → `rgb(69.6470588235%, 79.7647058824%, 69.6470588235%)`，
    /// `change/hsl.hrx::saturation/below_range`），而亮度低于零不是（`color.change(red, $lightness: -20%)` →
    /// `hsl(0, 100%, -20%)`，`…::lightness/below_range`）。
    fn out_of_gamut(&self) -> bool {
        const DUST: f64 = 5e-11;
        match self.authored {
            Some([_, saturation, lightness]) => {
                saturation > 100.0 + DUST || lightness < -DUST || lightness > 100.0 + DUST
            }
            None => self
                .channels
                .iter()
                .any(|channel| !(0.0..=255.0).contains(&channel.byte().round())),
        }
    }

    /// 三个轴的 `hsl`/`hsla` 拼法 —— 与 [`Representation::Hwb`] 共用，因为它也打 hsl。
    fn hsl_text(&self, [hue, saturation, lightness]: [f64; 3]) -> String {
        let channels = format!(
            "{}, {}%, {}%",
            crate::value::format_number(hue),
            crate::value::format_number(saturation),
            crate::value::format_number(lightness)
        );
        let name = if self.alpha >= 1.0 { "hsl" } else { "hsla" };
        if self.alpha >= 1.0 {
            format!("{name}({channels})")
        } else {
            format!(
                "{name}({channels}, {})",
                crate::value::format_number(self.alpha)
            )
        }
    }

    /// 是否有任一通道不是整字节 —— 那就是改打百分比而不是数字的触发条件（`grayscale(red)` →
    /// `rgb(50%, 50%, 50%)`、`grayscale(#494949)` → `#494949`）。由 [`Channel::whole`] 判，所以一次往返的浮尘
    /// 不会把一个整字节变成一行百分比。
    fn fractional(&self) -> bool {
        self.channels.iter().any(|channel| !channel.whole())
    }

    /// 同一个颜色写成 `rgb(…, …%, …)`：每个通道换算到它对 255 的份额。未书写的通道重标后仍未书写 ——
    /// 拿人造的 `rgb(0% none 30%)` 说（归档里没有这种形），那里没有可换算的数字。
    fn percent_form(&self) -> String {
        let channels = self.channels.map(|channel| Channel {
            value: channel.value.map(|_| channel.percent_form()),
            percent: true,
        });
        self.legacy(channels, true)
    }

    /// 带未书写通道的颜色必须用的那个空格分隔拼法；理由在 [`Color::to_css`] 里选中它的那一臂上。
    fn modern(&self) -> String {
        let channels = self
            .channels
            .iter()
            .map(|channel| match channel.value {
                None => "none".to_string(),
                Some(value) => format!(
                    "{}{}",
                    crate::value::format_number(value),
                    if channel.percent { "%" } else { "" }
                ),
            })
            .collect::<Vec<String>>()
            .join(" ");
        if self.alpha >= 1.0 {
            return format!("rgb({channels})");
        }
        format!(
            "rgb({channels} / {})",
            crate::value::format_number(self.alpha)
        )
    }

    /// legacy 的函数形：不透明时 `rgb(r, g, b)`，一旦有 alpha 就 `rgba(…)`（那正是每一行
    /// `four_args/clamped` 所打的），而通道用这个表示所采用的那个尺度。定为本文件私有：取整与打印的整条律 ——
    /// [`Color::fractional`]、[`Color::percent_form`] 与这一个 —— 都是从父模块搬过来的，所以外面不需要再伸进来。
    fn legacy(&self, printed: [Channel; 3], percent: bool) -> String {
        let channels = printed
            .iter()
            .map(|channel| {
                let number = crate::value::format_number(if percent {
                    super::missing_as_zero(channel.value)
                } else {
                    super::missing_as_zero(channel.value).round()
                });
                format!("{number}{}", if percent { "%" } else { "" })
            })
            .collect::<Vec<String>>()
            .join(", ");
        let name = if self.alpha >= 1.0 { "rgb" } else { "rgba" };
        if self.alpha >= 1.0 {
            return format!("{name}({channels})");
        }
        format!(
            "{name}({channels}, {})",
            crate::value::format_number(self.alpha)
        )
    }
}

/// 打印出来的一个通道：powerless 的那个是 `none`，其余是语料的打印机得出的那个数。
fn channel(value: Option<f64>, suffix: &str) -> String {
    value.map_or_else(
        || "none".to_string(),
        |number| format!("{}{suffix}", crate::value::format_number(number)),
    )
}

/// 一个在自己空间里携带的颜色的 CSS 文本，形体由语料逐字钉住：
/// `color(display-p3 0.2498513331 0.3952400722 0.77356175)`（`to_space/srgb/display_p3.hrx::middle`）、
/// `lab(44.1210142293% 10.9543336098 -59.0857381981)`、`lch(44.1210142293% 60.0926108873 280.5032243387deg)`
/// 与一个 `/` 后的 alpha（后两条与 `::alpha/partial` 来自 `to_space/srgb/{lab,lch}.hrx::middle`；通配不能写
/// `to_space/srgb/*.hrx::middle`，因为同目录的 `srgb.hrx` 根本**没有** `middle` 段，现量该段计数 = 0）。
///
/// 住在这里而不是换算表旁边，是本文件一直画的那同一条缝：空间定一个颜色**是什么**，这里定它**读作什么**。
pub(super) fn print(space: Space, channels: [Option<f64>; 3], alpha: f64) -> String {
    // 那些后缀是**从通道表里读的**，不是逐空间重列：哪个下标是亮度（打百分比）、哪个是色相（打一个角），
    // 在 [`super::channels::suffix`] 里只说一次，所以一个没有色相通道的空间不可能在这里意外长出一个 `deg`。
    let suffixes: [&str; 3] = [0, 1, 2].map(|index| super::channels::suffix(space, index));
    let body = [0, 1, 2]
        .iter()
        .map(|index| {
            // 百分比通道按空间自己的尺度写回 —— `oklab` 的 `L = 1` 是 `100%`，`lab` 的 `L = 44.1210142293` 是
            // `44.1210142293%` —— 经的都是读入时做除法的同一个 [`Space::percent_full`]，所以一条律的两个方向
            // 不能再各自漂移。
            let scaled = channels[*index].map(|value| {
                if suffixes[*index] == "%" {
                    value * space.percent_full()
                } else {
                    value
                }
            });
            channel(scaled, suffixes[*index])
        })
        .collect::<Vec<String>>()
        .join(" ");
    let alpha_part = if alpha < 1.0 {
        format!(" / {}", crate::value::format_number(alpha))
    } else {
        String::new()
    };
    if space.is_functional() {
        format!("color({} {body}{alpha_part})", space.css())
    } else {
        format!("{}({body}{alpha_part})", space.css())
    }
}
