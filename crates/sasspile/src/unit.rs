//! Unit —— 一个数的 **有理（复合）单位**，表示为两个保序多重集，这正是让 `1px * 1em / 1rad / 1s`
//! 成为一个值而不是字符串的东西：乘法与除法变成对两侧的运算、同名抵消是结构性的（绝不是后续的清理
//! 步骤），打印形式由数据推出，而不是重新扫描文本。
//!
//! ## 顺序是语料的，不是字母序的
//!
//! `core_functions/math/unit.hrx` 打印 **输入顺序**：`math.unit(1px * 1em * 1rad)` → `"px*em*rad"`
//! 与 `math.unit(1px * 1em / 1rad / 1s)` → `"px*em/(rad*s)"`。给多重集排序会得到 `"em*px*rad"` 而
//! 两条全错，所以表示保留插入顺序，只有抵消是结构性的。（当初的方案记着"构造时按名字排序"，fixture
//! 推翻了它 —— 见 `openspec/…/tasks.md` 1.4d 的修正。）
//!
//! ## 打印形式，每条都引自一个 golden
//!
//! | 单位 | 输出 | fixture |
//! |---|---|---|
//! | 无 | `""` | `unit::none` |
//! | `px` | `px` | `unit::one_numerator` |
//! | `px`、`em`、`rad` | `px*em*rad` | `unit::multiple_numerators` |
//! | 分母 `px`、无分子 | `px^-1` | `unit::one_denominator` |
//! | 分母 `px`、`em`、`rad`、无分子 | `(px*em*rad)^-1` | `unit::multiple_denominators` |
//! | `px` 在上 `em` 在下 | `px/em` | `unit::numerator_and_denominator/single` |
//! | `px`、`em` 在上，`rad`、`s` 在下 | `px*em/(rad*s)` | `unit::numerator_and_denominator/multiple` |
//!
//! 一个 **重复** 单位（`px * px`）没有打印形的 golden：`math.unit(1px * 1px)` 全语料零命中，而串
//! `1px * 1px` 只活在 `libsass-closed-issues/issue_783.hrx` 的注释行（该目录不入 harness 基线）与
//! `values/calculation/{max,min,clamp}.hrx::error/complex_unit`、
//! `values/calculation/calc/error/known_incompatible/complex.hrx` 的 error golden（等的是
//! `isn't compatible with CSS calculations`，不是单位串的打印）—— 所以本模型照写重复项（`px*px`），
//! 而不是为它发明幂形式。这是一个选择，而 `unit.hrx` 自己的 README 说该函数
//! "is intended for debugging and as such is not required to have totally consistent output across
//! different implementations"。

use std::f64::consts::PI;

/// 可换算单位表：`(单位, 族, 到族基准的因子)`。Sass 只在同族内换算，而且只有以下这些：
/// `em`/`rem`/`vh`/`vw`/`vmin`/`vmax`/`ch`/`ex` 相对的是 Sass 看不见的上下文，所以各自独占一族、
/// 只与自身结合。
pub const UNITS: &[(&str, (&str, f64))] = &[
    ("px", ("length", 1.0)),
    ("cm", ("length", 96.0 / 2.54)),
    ("mm", ("length", 96.0 / 25.4)),
    ("in", ("length", 96.0)),
    ("pt", ("length", 96.0 / 72.0)),
    ("pc", ("length", 96.0 / 6.0)),
    ("q", ("length", 96.0 / 25.4 / 4.0)),
    ("deg", ("angle", 1.0)),
    ("grad", ("angle", 0.9)),
    ("rad", ("angle", 180.0 / PI)),
    ("turn", ("angle", 360.0)),
    ("s", ("time", 1.0)),
    ("ms", ("time", 0.001)),
    ("hz", ("frequency", 1.0)),
    ("khz", ("frequency", 1000.0)),
    ("dpi", ("resolution", 1.0)),
    ("dpcm", ("resolution", 1.0 / 2.54)),
    ("dppx", ("resolution", 96.0)),
];

/// 表里某个单位的条目 —— `(族, 到族基准的因子)` —— 单位不在表里时是 `None`。`em`/`rem`/`vh`/`ch`…
/// 不在表里：每个各自代表一个上下文相关的族，所以 `math.compatible(1px, 2em)` 是 `false`
/// （`core_functions/math/comparable.hrx::unit/to_different`），而 `1em` 对 `2em` 按原样比较。比较
/// 辅助函数先问这里，好让未列出的单位 decline 而不是宣称撞单位（`math.max.hrx` 证实 `px` 对 `s`；
/// `px` 对 `c` 全语料没有证据）。
///
/// 这是本项目唯一一张可换算单位表，`math.div`、`math.compatible`、颜色 hue 与 `math.hypot` 都经这里
/// 读它 —— `math.rs` 曾自带一份长度字面量表，直到 `q` 成为必需（1q = 1/4 mm；
/// `core_functions/math/hypot.hrx::compatible_units` 与
/// `values/calculation/hypot.hrx::units/compatible` 两处换算都要它），表才归一到此处。
pub fn conversion(unit: &str) -> Option<(&'static str, f64)> {
    UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, entry)| *entry)
}

/// `(族, 到族基准的因子)`；未知或相对单位自成一族，所以它只能与完全相同的单位结合。族名借自提供
/// 它的那一侧 —— 表里的 `'static` 名字或调用方自己的 —— 所以这里不钉死任何生命周期。
pub fn measure(unit: &str) -> (&str, f64) {
    UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map_or_else(|| (unit, 1.0), |(_, (family, factor))| (*family, *factor))
}

/// 一个单位名是不是本表携带的角度单位之一（`deg`、`grad`、`rad`、`turn`）。有三处来问 —— 颜色 hue
/// 读、三角函数实参、`color.change` 的 hue 通道 —— 在这里一次答完，所以"哪些单位算角度"在它们之间
/// 不会打架。未知单位（如 `%` 或 `px`）按 [`measure`] 的规律自成一族，所以不是角度。
pub fn is_angle(unit: &str) -> bool {
    measure(unit).0 == "angle"
}

/// 有理单位的一侧：出现在那里的每个单位名，按出现的顺序。
type Names = Vec<String>;

/// 一个单位表示为 `numerator / denominator`，两侧在构造时互相抵消。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    numerator: Names,
    denominator: Names,
}

impl Unit {
    /// 无单位的数。
    pub fn unitless() -> Self {
        Self {
            numerator: Vec::new(),
            denominator: Vec::new(),
        }
    }

    /// 一个词素的单位文本：`""` 是无单位、`px` 是一个分子、`%` 是百分比。从源码读到的单位绝不复合
    /// （复合单位只作为 `*` 或 `/` 的产物存在），所以文本整体取用，而不是按 `*` 或 `/` 重新切分。
    pub fn named(name: &str) -> Self {
        Self {
            numerator: (!name.is_empty())
                .then(|| name.to_string())
                .into_iter()
                .collect(),
            denominator: Vec::new(),
        }
    }

    pub fn is_unitless(&self) -> bool {
        self.numerator.is_empty() && self.denominator.is_empty()
    }

    /// 带这个单位的数能否打印在 calculation **之外**：什么都没有、`%`、或一个裸名字。复合单位
    /// （`px*em`、倒过来的 `1/em`）是 calculation 专有的语法，所以 `calc(1px / 2px)` 塌成 `0.5`
    /// （`calc/operator/units::division` 要 `0.5`），而 `calc(1 / 1px / 1rad)` 留住外壳
    /// （`values/numbers/units/multiple::multiple_denominators`）。
    pub fn is_plain(&self) -> bool {
        self.is_unitless() || self.single().is_some()
    }

    /// 当整个单位恰好是「一个分子、没有分母」时给出它 —— 可换算路径读的 `unit.single().and_then(to_base)`
    /// 与「这个单位是不是 `%`/`deg`」这类逐单位判定（`unit.single() == Some("%")`），取的都是这一个形状。
    pub fn single(&self) -> Option<&str> {
        match (self.numerator.as_slice(), self.denominator.as_slice()) {
            ([only], []) => Some(only),
            _ => None,
        }
    }

    /// 原样交出两侧，给需要判断整个分式、而不是单个名字的读者：`builtins::dimension` 按**量纲签名**
    /// 比较两个单位（`builtins/math.rs` 里它读 `unit.names()`）。
    pub fn names(&self) -> (&[String], &[String]) {
        (&self.numerator, &self.denominator)
    }

    /// 消掉分式两侧的**一个可换算对**，并把它的因子搬进量值：分子上的 `px` 与分母上的 `in` 不是留下两个
    /// 名字，而是把 `96` 除掉 —— 语料把 `calc(96px * 1rad / 1ms / 1Hz) / 1in` 消到
    /// `calc(1rad / 1ms / 1Hz)`（`values/numbers/units/multiple::division/cancels/compatible`）。同族名字
    /// 按书写顺序从头一对一对上；配不上的留在原地。
    pub fn cancel_convertible(&self, value: f64) -> (f64, Unit) {
        let mut denominator = self.denominator.clone();
        let mut magnitude = value;
        let mut numerator = self.numerator.clone();
        // 每个分子名字都与同族、且尚不是同一单位的分母中**书写序最前**的那一个配对（同名的两份已由
        // `Unit::from_parts` 先消掉）；这一对离开分式，而「分子因子 / 分母因子」折进量值。
        numerator.retain(|top| {
            let (family, top_factor) = measure(top);
            let pair = denominator
                .iter()
                .position(|below| measure(below).0 == family && measure(below).1 != top_factor);
            match pair {
                Some(index) => {
                    magnitude *= top_factor / measure(&denominator[index]).1;
                    denominator.remove(index);
                    false
                }
                None => true,
            }
        });
        (magnitude, Unit::from_parts(numerator, denominator))
    }

    /// 由两侧直接给出的单位，构造时即已抵消。抵消是多重集求交：每个两侧都出现的名字从各侧各去掉一份，
    /// 幸存者保留自己的顺序。
    pub fn from_parts(numerator: Names, denominator: Names) -> Self {
        let (numerator, denominator) = cancel(numerator, denominator);
        Self {
            numerator,
            denominator,
        }
    }

    /// `a * b`：两侧合并，所以 `px/em * em` 就是 `px` —— 构造做的结构性抵消，作用在两个既有单位上。
    pub fn times(&self, rhs: &Unit) -> Unit {
        Unit::from_parts(
            self.numerator
                .iter()
                .chain(&rhs.numerator)
                .cloned()
                .collect(),
            self.denominator
                .iter()
                .chain(&rhs.denominator)
                .cloned()
                .collect(),
        )
    }

    /// `a / b`：除数的分子落进分母、除数的分母升到分子，所以倒数的倒数直接翻回分子：
    /// `calc(1 / (1 / 1px / 1rad))` 得到 `calc(1px * 1rad)`
    /// （`values/numbers/units/multiple::division/by/multiple_denominators`）。
    pub fn over(&self, rhs: &Unit) -> Unit {
        Unit::from_parts(
            self.numerator
                .iter()
                .chain(&rhs.denominator)
                .cloned()
                .collect(),
            self.denominator
                .iter()
                .chain(&rhs.numerator)
                .cloned()
                .collect(),
        )
    }

    /// 本模块开头那张表给出的单位 CSS 写法。无单位就什么都不打（数后面跟空的），纯倒数用语料写明的
    /// `^-1` 形（`core_functions/math/unit.hrx::one_denominator` 打 `"px^-1"`）。
    pub fn to_css(&self) -> String {
        let numerator = self.numerator.join("*");
        match (self.numerator.is_empty(), self.denominator.as_slice()) {
            (true, []) => String::new(),
            (true, [only]) => format!("{only}^-1"),
            (true, many) => format!("({})^-1", many.join("*")),
            (_, []) => numerator,
            (false, [only]) => format!("{numerator}/{only}"),
            (_, many) => format!("{numerator}/({})", many.join("*")),
        }
    }

    /// **计算壳里**的一个数：量值只写一次，其余每个单位各自带一个 `1` —— 复合单位没有纯 CSS 拼法，
    /// 语料打的是乘积形（`values/numbers/units/multiple.hrx`：`calc(1px * 1rad)`、`calc(1 / 1px / 1rad)`、
    /// `calc(1px * 1rad / 1ms / 1Hz)`）。
    ///
    /// **非有限**量值也没有纯 CSS 拼法，所以它同样走 [`crate::unit::Unit::calc_product`]：把 `infinity` 直接黏到单位上
    /// 会打出 `infinitypx`，那不是 CSS 数；语料写的是 `calc(1% + infinity * 1px)`
    /// （`values/calculation/calc/constant.hrx::precedence/after_plus`）与 `calc(NaN * 1deg)`
    /// （`core_functions/math/asin.hrx::greater_than_one`）。
    ///
    /// 直说给下一位读者、免得读过头：**颜色**的打印不经过这里。`Color::legacy`/`Channel`/`text::print`
    /// 直接拼 `format_number(…) + "%"`，所以裸 `infinity%` 仍出现在那一侧 —— 现量
    /// `core_functions/color/to_space/hwb/hsl::white` 得 `hsl(0, infinity%, 100%)`，归档是
    /// `hsl(0, 0%, 100%)`。「一条律只住一个家」在那一侧仍是待做的工作。
    pub fn calc_form(&self, value: f64) -> String {
        let magnitude = crate::value::format_number(value);
        if !value.is_finite() {
            return self.calc_product(&magnitude);
        }
        let with = |prefix: &str, unit: &str| format!("{prefix}{unit}");
        let head = match self.numerator.split_first() {
            Some((first, rest)) => std::iter::once(with(&magnitude, first))
                .chain(rest.iter().map(|unit| with("1", unit)))
                .collect::<Vec<String>>()
                .join(" * "),
            // 纯倒数（或已无单位的数）：量值自己站在最前面，后面不跟单位。
            None => magnitude,
        };
        head + &self
            .denominator
            .iter()
            .map(|unit| format!(" / 1{unit}"))
            .collect::<String>()
    }

    /// 同一种乘积形，用在**根本没有纯 CSS 拼法**的量值上 —— `NaN` 或 infinity —— 此时连单个单位
    /// 也得自带 `1`：`calc(NaN * 1deg)`（`core_functions/math/asin.hrx::greater_than_one`）、
    /// `calc(-infinity / 1px)`（`values/numbers/degenerate::minus_infinity/denominator_unit`）与
    /// `calc(infinity * 1px * 1em)`（`values/numbers/degenerate::infinity/multiple_numerator_units`）。
    /// [`Unit::calc_form`] 自己的有限臂说不出它：那条写法把第一个分子单位黏在量值后（`<magnitude>px`，
    /// 对 `NaN` 会读成 `NaNdeg`），所以非有限数在那里改道到这里。
    pub fn calc_product(&self, magnitude: &str) -> String {
        let (numerator, denominator) = self.names();
        magnitude.to_string()
            + &numerator
                .iter()
                .map(|unit| format!(" * 1{unit}"))
                .collect::<String>()
            + &denominator
                .iter()
                .map(|unit| format!(" / 1{unit}"))
                .collect::<String>()
    }
}

/// 两侧互相抵消：每个分母也持有的名字，在分母去掉一份、在分子去掉一份，所以 `px*em / em*rad` 变成
/// `px / rad`。两侧各自剩下的保持构造时的顺序，语料打的就是这个顺序。
fn cancel(mut numerator: Names, mut denominator: Names) -> (Names, Names) {
    numerator.retain(
        |name| match denominator.iter().position(|held| held == name) {
            Some(at) => {
                denominator.remove(at);
                false
            }
            None => true,
        },
    );
    (numerator, denominator)
}
