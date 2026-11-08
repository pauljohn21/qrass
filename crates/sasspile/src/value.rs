//! Value —— 一个 declaration 沿 push 流往前携带的**owned 值**，外加算术产出的单位代数。
//!
//! P0 打下地基：一个渲染完的 token run 成为 [`Value::Raw`]，由 [`Value::to_css`] 逐字发出，于是值这条路
//! 从那时起就被塑成「结构化值穿过算子、最后序列化一次」，而不是「字符串穿过管线再被重新解析」。P1 加上
//! [`Value::Number`] —— 一个量级加一个 [`crate::unit::Unit`]（自 1.4d 起是有理（复合）单位，而不是一个
//! 名字）—— 以及 [`crate::expr`] 里算术折叠所用的换算表。代数的其余部分分阶段到来（`Color`、带
//! 空格/逗号/斜杠分隔符的 `List`、插入序的 `Map`、带引号状态的 `String`、`Null`/`Argument`/`Module`），
//! 见 `openspec/changes/.../specs/value-types`。
//!
//! 类型保持 crate 内部（crate 根里 `mod value` 是私有）—— 行为只经唯一的 `compile` 出口可观测，
//! 绝不靠泄漏一个 `pub` 给测试用。

use crate::SassError;
use crate::calc::Content;
use crate::unit::{Unit, measure};

/// 一个沿管线往前流的 owned declaration 值。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 原样带到 CSS 的源码文本（P1 还不求值的任何东西）。
    Raw(String),
    /// 一个带单位的量级；无单位数带 [`Unit::unitless`]，`%` 是百分比，而复合单位（`px*em/(rad*s)`）
    /// 是 `*` 与 `/` 的产物，而不是一个词素的产物。
    Number(f64, Unit),
    /// Sass 的布尔值，打成裸词 `true` / `false`（`math.is-unitless`、`math.compatible`，以及后来的
    /// `@if` truthiness）。单列一个变体，好让布尔值永远不会从文本里被重新解析出来。
    Bool(bool),
    /// 一个 **calculation**，其内容保持被读入时的已归约形式：`name` 是照写的被调名，`content` 是
    /// [`crate::calc`] 的摘要 —— 它打印的文本、每步都可算时折出的值，以及它的优先级。存摘要而不是
    /// 存字符串，才让一个有界的 calculation 能重新参与算术
    /// （`values/numbers/units/multiple::division/cancels/numerator/once`）。
    Calculation(String, Content),
    /// 一个由函数**造出**的颜色（`rgb(18, 52, 86)` → 它自己，`rgb(#123, 1)` → `#112233`）。直接写进
    /// declaration 的 hex 保持 `Raw` —— 实现选型，不是语料律：`values/colors/alpha_hex/initial_letter`
    /// 的 `four-digit` 属性在 input.scss 是 `#AbCd`，而 output.css 逐字要 `rgba(170, 187, 204, 0.8666666667)`，
    /// 那行仍红 —— 见 [`crate::color`]。
    Color(crate::color::Color),
    /// 一个**空格分隔列表**，`Expr::Space` 的值形式：`rgb(18 52 86)` 是一个装三个数的实参，不是三个实参
    /// （`core_functions/color/rgb/one_arg/no_alpha.hrx`，同一文件的 `$channels:` 行表明一个标签绑的是
    /// 这整个值）。成员是 owned 值，所以消费方结构化地读它们，而不是重新切分文本。
    Space(Vec<Value>),
    /// 一个**斜杠分隔列表**，`Expr::Slash` 的值形式：CSS Color 4 的 `rgb(18 52 86 / 0.5)` 与
    /// `rgb(0 255 127 / 0)` 写成 `rgb(list.slash(0 255 127, 0))` 是同一次调用 —— 语料在
    /// `rgb/one_arg/alpha.hrx::{slash_list,in_gamut/transparent}` 把这两行配成对、输出逐字相同 —— 所以
    /// alpha 是斜杠列表的第二个元素，而不是对最后一个通道做除法。
    Slash(Vec<Value>),
}

impl Value {
    /// 收养一个已渲染好的 declaration 值（lexer 的 token run 是一个产出者；
    /// 另一个是求值表达式的 [`Value::Number`]）。
    pub fn from_text(text: String) -> Self {
        Value::Raw(text)
    }

    /// 把一个值序列化成 CSS 文本 —— 一个值变成字符串的唯一地方。
    pub fn to_css(&self) -> String {
        match self {
            Value::Raw(text) => text.clone(),
            Value::Number(value, unit) => {
                // 非有限数没有 plain CSS 拼法，所以被包起来：`calc(NaN)`、`calc(infinity)`、`calc(-infinity)`，
                // 而单位乘一搭在包装里 —— `math.asin(2)` → `calc(NaN * 1deg)`
                // （`core_functions/math/asin.hrx::greater_than_one`）、`math.pow(0, -2)` → `calc(infinity)`
                // （`core_functions/math/pow/base_positive_zero.hrx::with_exponent/*`），
                // 而倒数单位取除法形（`calc(-infinity / 1px)`，
                // `values/numbers/degenerate::minus_infinity/denominator_unit`）。
                // 在 calculation 里面同一个形状已经由 [`Unit::calc_form`] 给出，所以这里是一次读法，不是第二次。
                if value.is_finite() {
                    format_number(*value) + &unit.to_css()
                } else {
                    format!("calc({})", unit.calc_product(&format_number(*value)))
                }
            }
            Value::Bool(value) => value.to_string(),
            Value::Calculation(name, content) => format!("{name}({})", content.printed),
            Value::Color(color) => color.to_css(),
            // 成员各保自己的拼法，彼此以单空格分隔 —— 读成列表的 `rgb(18 52 86)` 在没人消费它时按列表打回。
            Value::Space(items) => items
                .iter()
                .map(Value::to_css)
                .collect::<Vec<String>>()
                .join(" "),
            // 斜杠两侧各一个空格，这是语料打印未被消费的斜杠列表的方式
            // （`values/lists/*`；`rgb(0 255 127 / none)` 就原样保持）。
            Value::Slash(items) => items
                .iter()
                .map(Value::to_css)
                .collect::<Vec<String>>()
                .join(" / "),
        }
    }

    /// 这个值能否打印在 calculation **之外**。只有带复合单位的数会失败（`Unit::is_plain`）——
    /// 本条律是本项目从 `values/numbers/units/multiple::multiple_denominators` 读出来的 ——
    /// 所以这样的结果永远不会成为 declaration 值；见 [`crate::exec::State::evaluated`]。
    pub fn is_plain(&self) -> bool {
        match self {
            Value::Number(_, unit) => unit.is_plain(),
            // 列表只有在每个元素都可打印时才能打印在 calculation 之外。
            Value::Space(items) | Value::Slash(items) => items.iter().all(Value::is_plain),
            _ => true,
        }
    }

    /// Sass 里的 `a + b`：把 `b` 换算成 `a` 的单位，无单位的一侧采用对侧的单位。
    pub fn plus(&self, rhs: &Value) -> Result<Value, SassError> {
        self.combine(rhs, |left, right| left + right)
    }

    /// Sass 里的 `a - b`，单位律与加法相同。
    pub fn minus(&self, rhs: &Value) -> Result<Value, SassError> {
        self.combine(rhs, |left, right| left - right)
    }

    /// 当这个值**是**一个数时它携带的那个数 —— 包括内容一路折到底的 calculation：
    /// `math.div(calc(2px * 3px), 4px)` 是 `1.5px`（`values/calculation/calc/operator/units::multiplication`），
    /// 所以折出的值必须能穿过壳取到，而不是只能穿过它的文本。
    pub fn number(&self) -> Option<Value> {
        match self {
            Value::Number(..) => Some(self.clone()),
            Value::Calculation(_, content) => content
                .folded
                .clone()
                .map(|(value, unit)| Value::Number(value, unit)),
            _ => None,
        }
    }

    /// `a * b`：量级相乘、单位结合，所以 `px * em` 是 `px*em` 而 `px/em * em` 是 `px`
    /// —— 抵消是结构性的，在 [`Unit::times`]。分式两侧可换算的一对也在这里抵消并带上因子：
    /// `calc(1px * 1rad / 1ms / 1Hz) * 1s` 是 `calc(1000px * 1rad / 1Hz)`，而不是
    /// `1px * 1rad * 1s / 1ms / 1Hz`（`values/numbers/units/multiple::multiplication/cancels/compatible`）。
    pub fn times(&self, rhs: &Value) -> Result<Value, SassError> {
        self.cross(rhs, |left, right| left * right, Unit::times)
    }

    /// `a / b`：量级相除、除数的单位换边，所以 `6 / 3px` 是 `2/px`。
    ///
    /// 除数是零在这里**不是**特例：f64 除法产出 infinity，而 `format_number` 已经按 Sass 的方式拼它
    /// （`math.div(1, 0)` → `infinity`、`values/calculation/calc/constant::infinity`），所以既不 panic
    /// 也不发明 `NaN`。
    pub fn divided_by(&self, rhs: &Value) -> Result<Value, SassError> {
        self.cross(rhs, |left, right| left / right, Unit::over)
    }

    /// `*` 与 `/` 共用这个：先造出原始结果，再把**分式两侧每一对可换算的**抵消掉、把因子折进量级 ——
    /// `96px / 1in` 是 `1`（`values/numbers/units/multiple::division/cancels/compatible`），
    /// `1s / 1ms` 是 `1000`（`…::multiplication/cancels/compatible`）。两个算子都需要它，
    /// 所以它住在一个地方，而不是每个调用点各记一遍。
    fn cross(
        &self,
        rhs: &Value,
        magnitude: impl Fn(f64, f64) -> f64,
        units: impl Fn(&Unit, &Unit) -> Unit,
    ) -> Result<Value, SassError> {
        match self.operate(rhs, magnitude, units)? {
            Value::Number(value, unit) => {
                let (scaled, reduced) = unit.cancel_convertible(value);
                Ok(Value::Number(scaled, reduced))
            }
            other => Ok(other),
        }
    }

    /// `*` 与 `/` 共用的形状：两侧都必须是数，量级过一次运算、单位过一次结合。与 `+`/`-` 不同，
    /// 这里没有换算或采用步骤 —— `1px * 1s` 是一个复合单位，不是一个错误。
    fn operate(
        &self,
        rhs: &Value,
        magnitude: impl Fn(f64, f64) -> f64,
        units: impl Fn(&Unit, &Unit) -> Unit,
    ) -> Result<Value, SassError> {
        let (Some(left), Some(right)) = (self.number(), rhs.number()) else {
            return Err(SassError::new("Undefined operation."));
        };
        let (Value::Number(left_value, left_unit), Value::Number(right_value, right_unit)) =
            (&left, &right)
        else {
            return Err(SassError::new("Undefined operation."));
        };
        Ok(Value::Number(
            magnitude(*left_value, *right_value),
            units(left_unit, right_unit),
        ))
    }

    /// `+`/`-` 背后的单位律：相同单位直接运算；无单位的一侧采用对侧的单位；可换算的单一单位被归一到
    /// **左**操作数的单位（所以 `1px + 1in` 是 `97px`）；其余任何情况 —— 包括一个没有单一单位对侧
    /// 可与之对齐的复合单位 —— 都是 Sass 错误。
    fn combine(
        &self,
        rhs: &Value,
        arithmetic: impl Fn(f64, f64) -> f64,
    ) -> Result<Value, SassError> {
        let (Value::Number(left, left_unit), Value::Number(right, right_unit)) = (self, rhs) else {
            return Err(SassError::new("Undefined operation."));
        };
        if left_unit == right_unit {
            return Ok(Value::Number(arithmetic(*left, *right), left_unit.clone()));
        }
        let adopted = match (left_unit.is_unitless(), right_unit.is_unitless()) {
            (true, _) => Some(right_unit.clone()),
            (_, true) => Some(left_unit.clone()),
            _ => None,
        };
        if let Some(unit) = adopted {
            return Ok(Value::Number(arithmetic(*left, *right), unit));
        }
        // 换算是单一单位的事：表以单位名建键，所以复合的一侧没有可证的因子，操作数保持不兼容，
        // 而不是被换算一半。
        let convertible = match (left_unit.single(), right_unit.single()) {
            (Some(left_name), Some(right_name)) => {
                let (left_family, left_factor) = measure(left_name);
                let (right_family, right_factor) = measure(right_name);
                (left_family == right_family).then(|| right * right_factor / left_factor)
            }
            _ => None,
        };
        if let Some(normalised) = convertible {
            return Ok(Value::Number(
                arithmetic(*left, normalised),
                left_unit.clone(),
            ));
        }
        Err(SassError::new(format!(
            "Incompatible units: '{}' and '{}'.",
            right_unit.to_css(),
            left_unit.to_css()
        )))
    }
}

/// Sass 数字输出：写出来**小数点后不超过十位**的数照原样打印，更长的在第十位小数处截断
/// （`1in + 1px` 是 `97px`，`2 * 3 / 5 * 7 / 11` 是 `0.7636363636`，而解析出的 `f64::MAX` 字面量保留
/// 标识它的那 309 位 —— 整数没有可截的小数位）。`values/numbers/precision.hrx` 在两侧钉住了边界：
/// `math.pow(10, -10)` → `0.0000000001` 而 `2 * math.pow(10, -11)` → `0`；`10 + math.pow(10, -10)` →
/// `10.0000000001` 而 `10 + 2 * math.pow(10, -11)` → `10`。
///
/// 没有“有效数字已经很短”的豁免：早先的版本有，于是 `math.sin(-0.000000000001)` 被打成
/// `-0.000000000001`，而 `core_functions/math/sin.hrx::negative_zero_fuzzy` 要 `0` —— 量级低于
/// 第十位小数的数，无论它有几个数字，都截成零。
pub(crate) fn format_number(value: f64) -> String {
    if value.is_infinite() {
        // Sass 把无穷拼成 `infinity` / `-infinity`；Rust 的 `Display` 给 `inf`，CSS 读不了它
        // （`values/calculation/calc/constant::infinity/math/unsimplified/from_variable`）。
        return if value > 0.0 {
            "infinity".to_string()
        } else {
            "-infinity".to_string()
        };
    }
    if value == 0.0 {
        // **精确**的负零保住符号，因为语料会打印一个：`math.sin(-0.0)` → `-0`
        // （`core_functions/math/sin.hrx::negative_zero`）、`math.atan(-0.0)` → `-0deg`
        // （`…/atan.hrx::negative_zero`）。而只是*截*到零的值 —— `color.change(red, $lightness: 0.5%)`
        // 让一个颜色通道拿回的 `-1.1e-15`，那一行要 `rgb(1%, 0%, 0%)` —— 被下面的分支归零，
        // 所以两条律不打架。
        return if value.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let natural = format!("{value}");
    // 过了十位小数，截断是**小数点后十位**，不是十位有效数字。颜色的百分比换算显示了这个差别：
    // `rgb(74.7%, 173, 93%)` 打 `rgb(74.7%, 67.8431372549%, 93%)` —— 十二位有效数字，因为
    // `173 × 100 / 255` 有十四位小数而其中十位存活（`core_functions/color/rgb/three_args/percents::unitless/green`）。
    // 它与此处已钉住的算术行一致：`2 * 3 / 5 * 7 / 11` → `0.7636363636`、
    // `1 / (2 * 3)` → `0.1666666667`（`operator/precedence::simplified/*`），而算出的 `0.246 × 100`
    // 修回 `24.6%`（`core_functions/math/percentage.hrx::small`）。
    let decimals = natural
        .split_once('.')
        .map_or(0, |(_, tail)| tail.chars().count());
    if decimals <= 10 {
        return natural;
    }
    let cut = format!("{value:.10}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();
    // 截断落成 `-0` 时按普通零打印：`color.change(red, $lightness: 0.5%)` 留下一个 `-1.1e-15` 的通道
    // —— 零的浮尘 —— 而语料打 `rgb(1%, 0%, 0%)`，不是 `-0%`（`change/hsl.hrx::lightness/fraction`）。
    // 它是 `Channel::clamped` 施加给写下的 `-0` 那条律的打印半边
    // （`rgb/four_args/clamped.hrx::red/negative_zero`：`rgb(-0, 0, 0, 0.5)` → `rgba(0, 0, 0, 0.5)`）。
    match cut.as_str() {
        "-0" => "0".to_string(),
        _ => cut,
    }
}
