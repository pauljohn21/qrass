//! 颜色的**轴算术**：让一个颜色穿过 hsl 轴移动，以及两个颜色求均。从 [`super`] 里拆出来，因为这是唯一一个
//! 颜色离开它被创建时那个空间的地方；把它分开，才能让 [`crate::axes`] 作为纯 `f64` 被测试，而这个文件只拥有
//! 夹紧、wrap 与结果保留哪种表示。
//!
//! 读这些结果的打印律（整字节打数字或 hex，中间值打百分比）是另一件事，住在 [`super::text`]。

use super::{Channel, Color, Representation};

impl Color {
    /// 改动过**hsl 轴**之后的同一个颜色，不论它存在哪里，带上 fixture 说的那两处夹紧与色相 wrap：
    /// * `saturate(plum, 100%)` 与 `saturate(plum, 53%)` 都给 `#ff7eff` —— 饱和度在 100 处夹紧，且落在整字节上
    ///   于是打 hex（`core_functions/color/saturate.hrx::two_args/{max,max_remaining}`）；
    /// * `desaturate(plum, 100%)` → `rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)` —— 亮度
    ///   `(221 + 160)/2 = 190.5` 对 255 不是整字节，所以打百分比（`desaturate.hrx::max`）；
    /// * `darken(red, 100%)` 把亮度夹在 0，而不是跑成负值。
    ///
    /// `color.grayscale` 就是同一个入口加上把饱和度钉在零，所以它不再需要自己的那条路径。
    pub fn transform_axes(&self, change: impl Fn([f64; 3]) -> [f64; 3]) -> Option<Self> {
        // 一个在自己空间里的颜色是**由这个判者**拒掉的，它对 `Representation::InSpace` 答 `None`：从它并不持有的
        // 字节三元组去推轴，会让 `complement(color(srgb …))` 用零作答；而把输入原样回声出去则声称那次操作已经
        // 发生了。语料要的 Color-4 规则是一次穿过命名空间的换算（`complement.hrx::space/missing/modern/different`
        // 写 `color.complement(color(srgb none 100 200), $space: oklch)` 并期望换算过的通道，而
        // `grayscale.hrx::modern/predefined` 期望 `color(xyz …)` 变过才出来），这一刀还没造。
        // 而“没有行会走到这里”是错的：`saturate.hrx::error/non_legacy` 就写 `saturate(color(srgb 1 1 1), 10%)`，
        // 金样要的是一句错误 —— `Error: saturate() is only supported for legacy colors.`（后面接
        // `Please use color.adjust() instead with an explicit $space argument.`），同形者还有 desaturate/darken/
        // lighten/fade_in/fade_out 与六个通道访问器：现量
        // `cargo run -q -p ledger -- failures <run> "error/non_legacy" --count` = **15 行仍红**。所以这里的 `None`
        // 只保证“不错答”，并不等于满足金样；那些错误消息还没造，`$space:` 成为 Color-4 访问器的参数是另
        // 一半前提（它现在在进 body 之前就被剔掉，那条记录住在 `tasks.md` 的 1.5 段，与
        // `mix/named::polar_space`/`explicit_method` 同一堆）。
        let [hue, saturation, lightness] = change(self.axes_of()?);
        let axes = [
            hue.rem_euclid(360.0),
            saturation.clamp(0.0, 100.0),
            lightness.clamp(0.0, 100.0),
        ];
        Some(match self.representation {
            Representation::Hsl => Self {
                channels: [
                    Channel {
                        value: Some(axes[0]),
                        percent: false,
                    },
                    Channel {
                        value: Some(axes[1]),
                        percent: true,
                    },
                    Channel {
                        value: Some(axes[2]),
                        percent: true,
                    },
                ],
                ..self.clone()
            },
            // 一个 `hwb` 颜色存不下一个 hsl 改动作为 whiteness 与 blackness，所以它作为自己会打的 `hsl` 值回来 ——
            // 而且是穿过 `rgb` 的往返，这正是语料显示的：
            // `color.grayscale(hwb(120deg 10% 20%))` → `hsl(0, 0%, 45%)`
            // （`grayscale.hrx::legacy/powerless/different`），色相是 `0` 因为一个灰没色相；而同一个操作作用在一个
            // `hsl` 颜色上则保住它的色相（`grayscale(hsl(120deg 10% 20%))` → `hsl(120, 0%, 20%)`）。
            Representation::Hwb => {
                let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
                let [hue, saturation, lightness] = crate::axes::rgb_to_axes(rgb[0], rgb[1], rgb[2]);
                Self {
                    channels: [
                        Channel {
                            value: Some(hue),
                            percent: false,
                        },
                        Channel {
                            value: Some(saturation),
                            percent: true,
                        },
                        Channel {
                            value: Some(lightness),
                            percent: true,
                        },
                    ],
                    alpha: self.alpha,
                    representation: Representation::Hsl,
                    // 一个穿过轴移动过的 `hwb` 颜色就是一个普通的 hsl 值；除了这些轴，它背后没有什么“照写的”
                    // 三元组，而 `axes_of` 已经把它们交出来了。
                    authored: None,
                }
            }
            // 具名而不是归进 `_`：上面那个 `?` 已经让它不可达，而以后新加的变体不得悄悄落进字节路径 —— 那份沉默
            // 就是一个空间的颜色打出 `rgb(0, 0, 0)` 的来历。
            Representation::InSpace(..) => return None,
            _ => Self::from_axes(self, axes),
        })
    }

    /// **把 hsl 轴写进去、不夹紧**的同一个颜色 —— 那是 `color.change` 的律，*不是* adjust 族的：
    /// `color.change(plum, $saturation: 120%)` 保住 120 并打 `hsl(300, 120%, 74.7058823529%)`
    /// （`change/hsl.hrx::saturation/above_range`），而 `saturate(plum, 100%)` 在 100 处夹紧并打 `#ff7eff`
    /// （`saturate.hrx::two_args/max`）。两种夹紧行为都由行说出，所以它们是两个入口，而不是一个需要调用者记得传的
    /// 标志位。
    ///
    /// 对一个由 `hwb` 造出的颜色是 `None`：它存 whiteness 与 blackness，而没有任何 fixture 说“把 hsl 轴写进它”
    /// 是什么意思，所以那次调用 decline，而不是发明一次往返。
    pub fn with_axes(&self, write: impl FnOnce([f64; 3]) -> [f64; 3]) -> Option<Self> {
        // 与 [`Self::transform_axes`] 同一个拒答，由 [`Color::axes_of`] 说一次：把 hsl 轴写到一个自带空间的颜色上，
        // 会把那个空间悄悄抛掉并打出 srgb 字节。
        let current = self.axes_of()?;
        match self.representation {
            Representation::Hwb => None,
            // 轴就是值，所以它们直接放回去 —— 色相是一个普通的度数字，另两个是百分比，与 [`Color::hsl`] 存它们的
            // 方式一模一样。
            Representation::Hsl => {
                let [hue, saturation, lightness] = write(current);
                Some(Self {
                    channels: [
                        Channel {
                            value: Some(hue),
                            percent: false,
                        },
                        Channel {
                            value: Some(saturation),
                            percent: true,
                        },
                        Channel {
                            value: Some(lightness),
                            percent: true,
                        },
                    ],
                    alpha: self.alpha,
                    representation: Representation::Hsl,
                    // 对一个 hsl 颜色，通道*就是*轴，所以没多什么要记住的：一个 out-of-gamut 的 hsl 值由
                    // `axes_of` 打，而它返回的正是这些。
                    authored: None,
                })
            }
            // 一个 `rgb` 族的颜色保住它自己的表示，并拿到新轴蕴含的那些字节；然后 `to_css` 在数字、百分比、hex、
            // 关键词之间定 —— 而对 gamut 之外的轴（正是它让 120% 看得见）打 hsl 形。
            _ => {
                let axes = write(current);
                let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
                Some(Self {
                    channels: rgb.map(|value| Channel {
                        value: Some(value),
                        percent: false,
                    }),
                    alpha: self.alpha,
                    representation: self.representation,
                    // 这次调用点名的轴，为 out-of-gamut 的打印留着：`with_axes` 是唯一一条路径，把一个 hsl 三元组写到一个
                    // 以别的方式造出的颜色上，而当那些字节落在 gamut 外时，再从它们推回三元组就会改掉它
                    // （`color.change(red, $lightness: 120%)` → `hsl(0, 100%, 120%)`，`change/hsl.hrx`）。
                    authored: Some(axes),
                })
            }
        }
    }

    /// **把 whiteness 与 blackness 写进去**的同一个颜色，保住调用方自己的表示：
    /// `color.change(#cc6666, $whiteness: 80%)` → `#cccccc`（`change/hwb.hrx::whiteness/high`），而一对落在
    /// gamut 外的值打它蕴含的那个 hsl 三元组 ——
    /// `color.change(#993333, $whiteness: 50%, $blackness: -20%)` → `hsl(0, 233.3333333333%, 85%)`
    /// （`…::out_of_range`），那就是 [`super::Color::authored`] 在干活。
    ///
    /// 一个由 `hsl` 造出的颜色不在这里答：没有任何 fixture 往它里写 whiteness 或 blackness，而发明一次穿过字节的
    /// 往返会是一条没人说过的规则。
    pub fn with_hwb(&self, hue: f64, whiteness: f64, blackness: f64) -> Option<Self> {
        let (white, black) = crate::axes::normalise_hwb(whiteness, blackness);
        let axes = crate::axes::hwb_to_hsl(hue, white, black);
        let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
        self.writes_bytes().then(|| Self {
            channels: rgb.map(|value| Channel {
                value: Some(value),
                percent: false,
            }),
            alpha: self.alpha,
            representation: self.representation,
            authored: Some(axes),
        })
    }

    /// 颜色的轴：当它以 hsl 持有就是存着的那些，一个 `hwb` 颜色是推出的，其余是它通道的往返
    /// （[`crate::axes`]）。定成 `pub(super)` 是因为 [`super::Color::hsl_channel`] 读的就是一个操作要写的那套轴 ——
    /// 一条路径，所以一个访问器与一个 `darken` 不会在一个颜色的亮度上分歧。
    ///
    /// **对一个在自己色彩空间里的颜色是 `None`**，而这正是那个变体的全部意思：这样一个颜色不存 hsl 轴，所以
    /// legacy 访问器没数字可交，只能保住自己的文本 —— 而**语料在这里要的是一句错误，不是保文本**：
    /// `hue.hrx::error/non_legacy` 对 `color.hue(lch(0% 0 0deg))` 逐字要
    /// `Error: color.hue() is only supported for legacy colors.`，所以那 15 行今天仍红（见
    /// [`Self::transform_axes`] 里同一件事的现量指针）。改去读那个空的字节三元组，就是那 15 行上曾经出现的
    /// `0%` 与 `rgb(85, 85, 85)` 的来历（`tasks.md` 9.9 逐字记着）。
    pub(super) fn axes_of(&self) -> Option<[f64; 3]> {
        match self.representation {
            Representation::Hsl => Some(self.stored_axes()),
            Representation::Hwb => Some(self.hwb_axes()),
            Representation::InSpace(..) => None,
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => Some(self.axes()),
        }
    }

    /// 一个颜色**照写存下的**那个三元组：对 `hsl` 是 hsl 轴，对 `hwb` 是 hue/whiteness/blackness。只说一次，
    /// 由 [`Color::axes_of`] 与打印机共用，所以两边都不重述这个读法。
    pub(super) fn stored_axes(&self) -> [f64; 3] {
        self.channels
            .map(|channel| super::missing_as_zero(channel.value))
    }

    /// 一个 `hwb` 颜色的 hsl 轴，是推出的、不是存的：`hwb(0deg, 30%, 40%)` 被要成
    /// `hsl(0, 33.3333333333%, 45%)`（`hwb/three_args/units::hue/deg`），而 `color.channel` 读回的就是同一个色相
    /// （`channel/hwb.hrx::local/hue`）。同一个理由，由 [`Color::axes_of`] 与打印机共用。
    pub(super) fn hwb_axes(&self) -> [f64; 3] {
        let [hue, whiteness, blackness] = self.stored_axes();
        crate::axes::hwb_to_hsl(hue, whiteness, blackness)
    }

    /// 一个 `rgb` 空间颜色的 hsl 轴，经 [`crate::axes`] —— 那个换算是纯 `f64` 并在那边单独测，所以这里只是把
    /// 通道值递给它。
    pub(super) fn axes(&self) -> [f64; 3] {
        let [red, green, blue] = self.channels.map(|channel| channel.byte());
        crate::axes::rgb_to_axes(red, green, blue)
    }

    /// 从 hsl 轴重建一个同种类的颜色，保持通道不取整，好让打印规则在数字、百分比与 hex 之间定。
    fn from_axes(source: &Color, [hue, saturation, lightness]: [f64; 3]) -> Self {
        let channels = crate::axes::axes_to_rgb(hue, saturation, lightness);
        Self {
            channels: channels.map(|value| Channel {
                value: Some(value),
                percent: false,
            }),
            alpha: source.alpha,
            representation: source.representation,
            // 轴会在需要时从这些字节重推，只要它们留在 gamut 内这就是精确的 —— 而 `transform_axes` 会夹，
            // 所以它们确实一直在内。
            authored: None,
        }
    }

    /// legacy 的 `mix`：一个 sRGB 里的**按 alpha 加权**的均，它的形状只有这个公式能解释。逐行对着
    /// `core_functions/color/mix/{unweighted,explicit_weight,alpha}.hrx` 核过：
    /// * `color.mix(#91e16f, #0144bf)` → `rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)` —— 两个 alpha
    ///   相等时权重就是一个普通的 0.5，给出对 255 的 `(145 + 1)/2 = 73`；
    /// * `color.mix(rgba(#91e16f, 0.8), rgba(#0144bf, 0.3))` → `rgba(42.7450980392%, …, 0.55)` —— `w = 0`、
    ///   `a = 0.5`，所以第一个颜色携带 `0.75`：`145 × 0.75 + 1 × 0.25 = 109`，而 alpha 是均值；镜像那行
    ///   （`0.4`/`0.9`）给 `0.25` → `37` → `14.5098039216%`；
    /// * `color.mix(#91e16f, transparent)` → `rgba(145, 225, 111, 0.5)` —— `a = 1` 让第一个颜色携带全部，而结果
    ///   仍是整字节，所以打数字；
    /// * 结果保住**第一个**颜色的表示，这正是最后一行在另一侧是 `transparent` 时打数字而不是关键词的原因。
    ///
    /// 一个存在 hsl 或 hwb 空间里的颜色没有可混的 `0–255` 通道，所以那次调用 decline，而不是发明一个换算过的答：
    /// 没有 fixture 说 `color.mix` 怎么处理那些空间，而把一个 `hwb` 颜色的 whiteness 与 blackness 当成字节去混，
    /// 会是一个 pass 数看不见的错值。
    pub fn mixed(first: &Color, second: &Color, weight: f64) -> Option<Self> {
        let span = first.alpha - second.alpha;
        let tilt = 2.0 * (weight / 100.0).clamp(0.0, 1.0) - 1.0;
        let ratio = if tilt * span == -1.0 {
            tilt
        } else {
            (tilt + span) / (1.0 + tilt * span)
        } / 2.0
            + 0.5;
        let channel = |colour: &Color, index: usize| {
            // [`Color::writes_bytes`] 是哪个表示存 `0–255` 字节的唯一判者，所以一个 `hsl`/`hwb` 颜色与一个在自己
            // 空间里的颜色都在这里答 `None`：去读后者留下的字节三元组，会答成 `rgb(0, 0, 0)` —— 一个 pass 数看不见的
            // 错值（`tests/color.rs::a_colour_in_an_unimplemented_space_is_not_answered_in_srgb`）。要在两个空间之间混，需要
            // [`crate::color::space`] 里那个由公共空间驱动的换算，那正是 `mix/named.hrx::polar_space` 与
            // `::rectangular_space` 要的；不是在这里猜。
            colour.writes_bytes().then(|| colour.channels[index].byte())
        };
        let [red, green, blue] = [0, 1, 2].map(|index| {
            let value = channel(first, index)? * ratio + channel(second, index)? * (1.0 - ratio);
            Some(Channel {
                value: Some(value),
                percent: false,
            })
        });
        Some(Self {
            channels: [red?, green?, blue?],
            alpha: (first.alpha + second.alpha) / 2.0,
            representation: first.representation,
            authored: None,
        })
    }
}
