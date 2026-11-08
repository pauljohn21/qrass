//! 一个 **legacy 通道**：写下来的那个数、写它所用的尺度，以及不属于这两件事的那些律（夹紧，以及算术怎么读一个
//! 未写的通道）。
//!
//! 从父模块里拆出来，是因为颜色级的构造子与打印分派已经长过了本仓库的行数预算，而代码早就画好的那道接缝就是
//! 这里：[`Color`] 决定*一个颜色是什么*；这里的一个通道决定*一个写下的数怎么行为*。本文件里没有
//! 任何东西知道色彩空间、表示或打印。

use crate::value::Value;

/// 一个值**是不是 `none` 这个词** —— 整个 crate 里这个问题的唯一一种拼法。它在三处被问（`Color::new` 读一个
/// 写下的通道、`color(…)` 构造子的通道读法，以及 `change` 族读一个实参），而它们过去在大小写上不一致：两处用
/// `eq_ignore_ascii_case` 比，一处比 `== "none"`，于是 `rgb(NONE 0 0)` 是一个缺通道而
/// `color.change(x, $red: NONE)` 不是。**没有任何一行语料演练任一种大小写变体**
/// （`grep -rn "\bNONE\b" sass-spec/spec/core_functions/color/` 为空），所以上述大小写规则是一个声明过的选型——
/// 忽略大小写，跟空间名已有的读法一致——而不是归档打出来的一条律。
/// `change/hwb.hrx::whiteness/none` 是确立那个裸词本身的行。
pub(crate) fn is_missing_word(value: &Value) -> bool {
    matches!(value, Value::Raw(text) if text.eq_ignore_ascii_case("none"))
}

/// 算术怎么读一个没被写的通道：当作 `0`。这条律只在这里说一次 —— 下面字节尺度的读法与
/// [`super::space::InSpace::filled`] 都走它，因为一个缺席的通道是一个事实，而它的两种拼法在本仓库已经分开漂移过
/// 一次。
///
/// *答案*打什么是另一个问题，由 [`Color::converted`](super::Color::converted) 底部那条索引对齐的规则与
/// [`super::text::print`] 定；这里不猜它。
pub(super) fn missing_as_zero(value: Option<f64>) -> f64 {
    value.unwrap_or(0.0)
}

/// 一个写下的数的**负零**怎么回来：作为 `0`。语料把它折叠 —— `color.change(lab(50% 0 0), $a: -0)` 答
/// `lab(50% 0 0)`（`change/unusual_numbers.hrx::negative_zero`），而 `rgb(-0, 0, 0, 0.5)` 答
/// `rgba(0, 0, 0, 0.5)`（`rgb/four_args/clamped.hrx::red/negative_zero`）—— 而 `f64` 会把符号穿过每一步算术保住，
/// 打出 `-0`。这条律只有这一处说法；下面的字节尺度夹紧与
/// [`Color::with_space_channel`](crate::color::Color::with_space_channel) 都读这里。
pub(super) fn without_negative_zero(value: f64) -> f64 {
    value + 0.0
}

/// 一个通道：照写的值，以及它是否以百分比写出。`None` 就是一个 `none` —— 一个调用没写的通道，而语料在一次
/// 恒等换算里**让它继续没写**（`to_space/rgb/rgb.hrx::missing/red` 对 `color.to-space(rgb(none 20 30), rgb)` 答
/// `rgb(none 20 30)`）而不是代一个 `0`，所以缺席是数据、不是一个哨兵数字。它住在通道自己身上，因为那就是全部
/// 事实：在三元组旁边再加一份 masks，就是同一个东西的第二个住所。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Channel {
    pub(super) value: Option<f64>,
    pub(super) percent: bool,
}

impl Channel {
    /// `0–255`，不管它以哪个尺度到达。
    pub(super) fn byte(self) -> f64 {
        let value = missing_as_zero(self.value);
        if self.percent {
            value * 255.0 / 100.0
        } else {
            value
        }
    }

    /// 百分比形式，给那些有任何百分比通道的颜色用。
    pub(super) fn percent_form(self) -> f64 {
        let value = missing_as_zero(self.value);
        if self.percent {
            value
        } else {
            value * 100.0 / 255.0
        }
    }

    /// 这个通道**打印**出来是不是一个整字节，而一个颜色打 hex、还是打数字/百分比，就由它定。
    ///
    /// 那个容差不是随意的：它是 `format_number` 能打的最末一位的一半（小数点后十位，所以 `5e-11`）。没有它，就是
    /// 浮尘在决定它打成哪一种形制 —— `color.change(#993333, $blackness: 80%)` 蕴含一个 `51.000000000000014` 的红
    /// 通道，而语料打 `#333333`，不是 `rgb(20%, 20%, 20%)`（`change/hwb.hrx::blackness/high`）。同一个理由，
    /// [`super::text`] 里的 gamut 测试也是先舍入再比：**舍入属于打印**，两个方向都是。
    pub(super) fn whole(self) -> bool {
        (self.byte() - self.byte().round()).abs() < 5e-11
    }

    /// 夹进它自己的尺度：百分比夹 `0–100`，普通数字夹 `0–255`。一个负零变成零，这就是
    /// `rgb/four_args/clamped.hrx::red/negative_zero` 所示（`rgb(-0, 0, 0, 0.5)` → `rgba(0, 0, 0, 0.5)`）。
    ///
    /// 一个**被夹过的百分比会失去它的百分比拼法**，并作为它被夹到的那个字节回来 —— `rgb(100.1% 0 0)` →
    /// `rgb(255, 0, 0)`、`rgb(0 -0.1% 0)` → `rgb(0, 0, 0)`，而 `rgb(0% 100% 50%)` 保住给它的每个 `%`
    /// （`rgb/one_arg/no_alpha.hrx::{percents/clamped/*,percents/boundaries}`）。只有现代的单参数行说这条律；
    /// 没有 legacy 的逗号 fixture 带着一个范围外的百分比，这就是它此前不可见的原因。一个恰在顶上的通道不算被夹，
    /// 所以 `100%` 上面仍是 `100%`。
    pub(super) fn clamped(self) -> Self {
        // 一个从未被写的通道没有可夹的值，而把它夹到 `0` 会把语料的 `rgb(none 20 30)` 变成 `rgb(0 20 30)` ——
        // 在归档看来那是两个不同的颜色（`to_space/rgb/rgb.hrx::missing/red`）。
        let Some(value) = self.value else {
            return self;
        };
        let ceiling = if self.percent { 100.0 } else { 255.0 };
        let bound = without_negative_zero(if value.is_nan() || value < 0.0 {
            0.0
        } else {
            value.min(ceiling)
        });
        match (self.percent, bound == value) {
            (true, false) => Channel {
                value: Some(bound * 255.0 / 100.0),
                percent: false,
            },
            _ => Channel {
                value: Some(bound),
                ..self
            },
        }
    }
}
