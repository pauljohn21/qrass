//! 从一个颜色里读**一个通道** —— 访问器与 `color.channel`。从 [`super`] 里拆出来，是因为空间要紧的正是这一半：
//! 同一个“读某通道”的问题在不同空间里有三种答法 —— `#abcdef` 的 red 在 `rgb` 里是 `171`、`pink` 的 green 在
//! `srgb` 里是 `0.7529411765`、`pink` 的 hue 在 `hsl`/`hwb` 里是 `349.5238095238deg`，而一次调用可以问哪一个
//! 又是它自己的一条律（`channel/*.hrx`），与一个颜色怎么被造出来或打印分开。

use super::{Channel, Color, Representation};
use crate::value::Value;

impl Color {
    /// 一个 `0–255` 的通道，给访问器用。存在**另一个空间**里的颜色返回 `None`，而不是一个错数。
    ///
    /// 语料里没有一行与这份小心相矛盾，而测量也说这不是一个该靠推理填的缺口：
    /// `grep -rlE 'color\.(red|green|blue)\([^)]*hsl\(' sass-spec/spec/` 找到的是**零个文件**（对照
    /// `grep -rl 'color\.red(' sass-spec/spec/` 命中 6 个）—— hsl 空间的 fixture 读的是 hsl 轴
    /// （`channel/hsl.hrx`），那由 [`Self::hsl_channel`] 服务。所以 `hsl` → `rgb` 没有可照着写的金样，而反方向有，
    /// 也已实现。
    pub fn byte(&self, index: usize) -> Option<f64> {
        match self.representation {
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => self
                .channels
                .get(index)
                .map(|channel: &Channel| channel.byte().round()),
            Representation::Hsl | Representation::Hwb => None,
            // 一个在自己空间里的颜色，在有谁把它换算之前没有 srgb 字节，所以一个裸的
            // `color.red(color(srgb …))` 退回自己的文本，而不是拿那个空间颜色留空的三元组作答。该服它的读法是
            // `color.channel(…, "red", $space: srgb)`，而那需要把换算路线接进访问器 —— 不在这里猜。
            Representation::InSpace(..) => None,
        }
    }

    /// 颜色自己的 alpha：`color.alpha(rgba(red, 0.42))` → `0.42`（`alpha.hrx::color/middle`）。
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// 一个通道，按 `color.channel` 点它的方式命名，在某个空间里读。
    ///
    /// `space` 是调用写下的 `$space:`（`hsl`、`rgb`、`srgb`）；`None` 意思是**颜色自己的空间**，而没有标签时语料
    /// 只钉住这个：`color.channel(#abcdef, "red")` → `171`，`color.channel(hsl(10 40% 60%), "hue")` → `10deg`
    /// （`channel/rgb.hrx::local/*`、`channel/hsl.hrx::local/*`）。要在两个空间之间读，只有在标签说了话时才答
    /// （`channel/hsl.hrx::foreign/hue` 是 `color.channel(pink, "hue", $space: hsl)` → `349.5238095238deg`）；
    /// 不待标签就代表颜色换算一次，会发明一条没有 fixture 说过的规则。
    ///
    /// 单位也来自语料：一个 `rgb`/`srgb` 通道是一个裸数字（`171`，以及
    /// `color.channel(pink, "green", $space: srgb)` → `0.7529411765` —— 就是字节除 255），色相带 `deg`，
    /// 饱和度与亮度带 `%`。
    pub fn channel(&self, name: &str, space: Option<&str>) -> Option<Value> {
        let number = |value: f64| Value::Number(value, crate::unit::Unit::unitless());
        let percent = |value: f64| Value::Number(value, crate::unit::Unit::named("%"));
        // 哪些表示拿着 `0–255` 字节，由 [`Color::writes_bytes`] 判一次；在这里重列一次变体清单，就是会让以后新加
        // 的变体去答它并不持有的字节。
        let rgb_space = self.writes_bytes();
        match name {
            "red" | "green" | "blue" => {
                let index = match name {
                    "red" => 0,
                    "green" => 1,
                    _ => 2,
                };
                match space {
                    Some("srgb") => Some(number(self.byte(index)? / 255.0)),
                    Some("rgb") => Some(number(self.byte(index)?)),
                    None if rgb_space => Some(number(self.byte(index)?)),
                    _ => None,
                }
            }
            // 色相在 `hsl` 与 `hwb` 里是同一个数（`channel/hwb.hrx::foreign/hue` 从 `pink` 读出
            // `349.5238095238deg`），而 [`Self::axes_of`] 为每个被存下的空间都能推出它。
            "hue" => match space {
                Some("hsl") | Some("hwb") => Some(Value::Number(
                    self.hsl_channel(0)?,
                    crate::unit::Unit::named("deg"),
                )),
                None if matches!(
                    self.representation,
                    Representation::Hsl | Representation::Hwb
                ) =>
                {
                    Some(Value::Number(
                        self.hsl_channel(0)?,
                        crate::unit::Unit::named("deg"),
                    ))
                }
                _ => None,
            },
            "saturation" | "lightness" => {
                let axis = usize::from(name == "lightness") + 1;
                match space == Some("hsl")
                    || (space.is_none() && self.representation == Representation::Hsl)
                {
                    true => self.hsl_channel(axis).map(percent),
                    false => None,
                }
            }
            // whiteness 与 blackness 只存在于 `hwb`：一个由 `hwb` 造出的颜色存着它们，而一个 `rgb` 族的颜色通过
            // [`crate::axes::rgb_to_hwb`] 对这个标签作答（`color.channel(pink, "whiteness", $space: hwb)` →
            // `75.2941176471%`，`…"blackness"…` → `0%`，`channel/hwb.hrx::foreign/*`）。
            "whiteness" | "blackness" => {
                let index = usize::from(name == "blackness");
                let own = self.representation == Representation::Hwb;
                let read = if own {
                    // 一个 `hwb` 颜色还拿不住一个未写的通道（它的构造子仍对 `none` decline），所以这一次读没有
                    // `None` 可以越过；这个助手在这里，是为了让它真的能拿住的那天，算术的那个答案就是
                    // `missing_as_zero` 里那一条律。
                    super::missing_as_zero(self.channels[index + 1].value)
                } else if rgb_space {
                    let [red, green, blue] = self.channels.map(|channel| channel.byte());
                    crate::axes::rgb_to_hwb(red, green, blue)[index]
                } else {
                    return None;
                };
                (own && space.is_none() || space == Some("hwb")).then(|| percent(read))
            }
            // 其余每个空间（`lab`、`lch`、`oklab`、`oklch`，以及那些设备空间）需要一个本编译器还没实现的换算。
            _ => None,
        }
    }

    /// 颜色的 `[hue, whiteness, blackness]`，按 [`Color::with_hwb`] 收回来的那种形式：`hwb` 颜色是存着的，
    /// 其余情况由字节推出（`color.channel(pink, "whiteness", $space: hwb)` → `75.2941176471%` 读的就是同一个推导，
    /// `channel/hwb.hrx::foreign/*`）。对一个由 `hsl` 造出的颜色是 `None`，它的 whiteness 与 blackness 没有
    /// 一行会问。
    pub fn hwb_of(&self) -> Option<[f64; 3]> {
        let hue = self.hue()?;
        match self.representation {
            Representation::Hwb => Some([
                hue,
                super::missing_as_zero(self.channels[1].value),
                super::missing_as_zero(self.channels[2].value),
            ]),
            _ if self.writes_bytes() => {
                let [red, green, blue] = self.channels.map(|channel| channel.byte());
                let [white, black] = crate::axes::rgb_to_hwb(red, green, blue);
                Some([hue, white, black])
            }
            _ => None,
        }
    }

    /// 以度计的色相：`hsl(359, 50%, 50%)` 给 `359deg`，`hsl(540, …)` 给 `180deg`，因为色相在创建时就被折进了
    /// `0–360`（`color/hue.hrx`），而 `#abcdef` 给 `210deg` —— 从 `rgb` 通道经 [`Self::axes_of`] 读出
    /// （`modules/color/hue.hrx`）。
    pub fn hue(&self) -> Option<f64> {
        self.hsl_channel(0)
    }

    /// 饱和度，给 `color.saturation`：`100%`、`0%`、`50%`、`0.5%`、`42%`（`color/saturation.hrx`），以及 `#abcdef`
    /// 的 `68%`（`modules/color/saturation.hrx`）。
    pub fn saturation(&self) -> Option<f64> {
        self.hsl_channel(1)
    }

    /// 亮度，给 `color.lightness`：同样的五个读数在 L 轴上（`color/lightness.hrx`），以及 `#abcdef` 的
    /// `80.3921568627%`（`modules/color/lightness.hrx`）。
    pub fn lightness(&self) -> Option<f64> {
        self.hsl_channel(2)
    }

    /// 颜色的一个轴，不论它存在哪里：一个 `hsl` 颜色就是通道本身，存在另一个 legacy 空间里的颜色走
    /// [`crate::axes`] 的往返。
    ///
    /// 对一个在自己色彩空间里的颜色是 `None` —— 语料在那里要的是一个错误，不是一个数字
    /// （`hue.hrx::error/non_legacy`、`saturation.hrx::error/non_legacy`、`lightness.hrx::error/non_legacy`），
    /// 而这一拒答由 [`Color::axes_of`] 说一次，不靠这个访问器记得去问。
    ///
    /// 对 legacy 那几族，换算而来的读法正是语料逐行要的：`color.lightness(#abcdef)` → `80.3921568627%` 是 255
    /// 的 `(239 + 171)/2`，而 `color.saturation(#abcdef)` → `68%` 是 `68/(510-410)`，两边都出自
    /// `modules/color/*.hrx`。就是这些行让一次往返在这里是安全的；一个 `hsl` 颜色自己的轴已经存着，所以不为它
    /// 走往返。
    pub(super) fn hsl_channel(&self, index: usize) -> Option<f64> {
        self.axes_of().and_then(|axes| axes.get(index).copied())
    }
}
