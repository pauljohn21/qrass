//! `math` 域（`core_functions/math/**`）。这里只有函数体：契约、登记表条目与 arity 诊断住在
//! [`crate::builtins`]，因为语料对每个函数的说法都一样，而把它们按域各写一份就是它们漂移的方式。
//! 需要第二个文件的函数体是 [`trig`]（七个三角函数）、[`exponents`]（`pow`/`sqrt`/`log`）与
//! [`combining`]（`clamp`/`hypot`）；三者都是本模块的子模块，所以都读下面的实参助手，而不自带一套。

pub(super) mod combining;
pub(super) mod exponents;
pub(super) mod trig;

use crate::builtins::{Builtin, missing};
use crate::unit::Unit;
use crate::value::Value;
use crate::{ErrorKind, SassError};

/// 本域的登记表，按文件定义它们的顺序。
pub(super) static MATHS: &[&Builtin] = &[
    &MATH_DIV,
    &MATH_ROUND,
    &MATH_ABS,
    &MATH_CEIL,
    &MATH_FLOOR,
    &MATH_MIN,
    &MATH_MAX,
    &MATH_UNIT,
    &MATH_IS_UNITLESS,
    &MATH_PERCENTAGE,
    &MATH_COMPATIBLE,
    &trig::MATH_SIN,
    &trig::MATH_COS,
    &trig::MATH_TAN,
    &trig::MATH_ASIN,
    &trig::MATH_ACOS,
    &trig::MATH_ATAN,
    &trig::MATH_ATAN2,
    &exponents::MATH_POW,
    &exponents::MATH_SQRT,
    &exponents::MATH_LOG,
    &combining::MATH_CLAMP,
    &combining::MATH_HYPOT,
];

/// `math` 模块自己的常数，按 `math.$pi` 寻址（`core_functions/math/variables.hrx`）。每个值都是从打印它的
/// 那一行取来的，因为凭记忆写下的数字串正是本项目被烧过的地方：
/// * `math.$pi * 1e15` → `3141592653589793`，`math.$e * 1e15` → `2718281828459045`，
///   `math.$epsilon * 1e31` → `2220446049250313` —— 这三个 `f64` 常数本身；
/// * `math.$max-safe-integer` → `9007199254740991`，`math.$min-safe-integer` → 它的负数；
/// * `math.$max-number` → 最大有限 double 的 309 位数字，而 `math.$min-number * 1e300 * 1e39` →
///   `4940656458412465`，这让 `$min-number` 成为**最小的 denormal**（`f64::from_bits(1)` =
///   4.9406564584124654e-324），不是 `MIN_POSITIVE`。
///
/// `infinity` 与 `nan` 就是**计算**面读的那两个 double（`calc(infinity)`、`calc(NaN)` ——
/// `values/calculation/calc/constant.hrx`）；它们住在这里而不是第二张表里，好让这个 crate 里没有一个数字被
/// 写两遍。**没有被证实**的是这两个常数的模块变量写法：范围内没有任何一行写 `math.$infinity` 或
/// `math.$NaN`（在 `sass-spec/spec` 上量过：零个文件），所以它们经由 [`super::module_value`] 的可达只是这张
/// 共享表的副作用，不是一个已交付的特征。
pub(crate) fn constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "epsilon" => f64::EPSILON,
        "infinity" => f64::INFINITY,
        "nan" => f64::NAN,
        "max-safe-integer" => 9_007_199_254_740_991.0,
        "min-safe-integer" => -9_007_199_254_740_991.0,
        "max-number" => f64::MAX,
        "min-number" => f64::from_bits(1),
        _ => return None,
    })
}

/// 一个必须带**无单位**的参数的拒语：`$number: Expected 1px to have no units.`。逐字带着这一行的文件现量是
/// `core_functions/math/{atan,asin,sqrt,log}.hrx`；`log.hrx::error/base_has_units` 与
/// `pow/arguments.hrx::error/{base,exponent}_has_units` 用同一套措辞点名 `$base:`/`$exponent:`。读它的是三角
/// 反函数、`math.sqrt`、`math.log` 与 `math.pow`，所以措辞住在一处——一个单位也照样用复数“units”是语料自己的
/// 文本（`percentage.hrx::error/unit`）。
pub(super) fn unitless(name: &str, value: f64, unit: Unit) -> Result<f64, SassError> {
    if unit.is_unitless() {
        return Ok(value);
    }
    Err(SassError::typed(
        ErrorKind::WrongUnits,
        format!(
            "${name}: Expected {} to have no units.",
            Value::Number(value, unit).to_css()
        ),
    ))
}

/// `math.div(6, 3)` → `2`：只收数字，配上金样写明的单位定律——同单位相消（`6px / 3px` → `2`）、无单位的
/// 除数保留分子的单位（`6px / 3` → `2px`）、可换算的单位先换算（`6in / 3px` → `192`，这也把 `1in = 96px`
/// 钉成语料唯一支持的因子），其余一切都变成一个**有理单位**而不是拒收：`math.div(6, 3px)` 是 `2/px`，
/// 乘 `* 1px` 能除回 `2`（`div.hrx::unit/denominator`），`math.div(6in, 3s)` 是 `2in/s`
/// （`…::unit/incompatible`），`math.div(6c, 3d)` 是 `2c/d`——未知的单位只合并不换算（`…::unit/unknown`）。
/// 上面 `6px / 3px`、`2/px`、`2in/s` 这类写法是单位代数的示意，不是归档里的逐字行；可逐字取回的是各 key 下的
/// 调用式与输出行。
static MATH_DIV: Builtin = Builtin {
    name: "math.div",
    params: &["number1", "number2"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| match args.as_slice() {
        [from, by] => {
            let (
                Some(Value::Number(from_value, from_unit)),
                Some(Value::Number(by_value, by_unit)),
            ) = (from.number(), by.number())
            else {
                return Err(SassError::new("not a number"));
            };
            // 一个 calculation 实参在它的内容折叠之后就是一个数字：`math.div(calc(2px * 3px), 4px)`
            // 是 `1.5px`（`values/calculation/calc/operator/units::multiplication`）。
            let (left, right) = (
                Value::Number(from_value, from_unit.clone()),
                Value::Number(by_value, by_unit.clone()),
            );
            if from_unit == by_unit {
                Ok(Value::Number(from_value / by_value, Unit::unitless()))
            } else if by_unit.is_unitless() {
                Ok(Value::Number(from_value / by_value, from_unit.clone()))
            } else {
                left.divided_by(&right)
            }
        }
        _ => Err(SassError::new("not a number")),
    },
};

/// 把一个单位放到它那个族的基准上的因子，读自 [`crate::unit`] 里唯一的那张表——`math.div`、色彩的色相、
/// `math.compatible` 与 `math.hypot` 都走它。这些数字过去在这里有一份本地副本，写成十位小数的字面量；而
/// `hypot.hrx::compatible_units`（`5q * 40` 混在厘米与英寸之间 → `11.6189500386cm`）需要 `q` 与精确的“毫米的四分之一”
/// 关系，那正是第二张表给不出的东西。
fn to_base(unit: &str) -> Option<f64> {
    crate::unit::conversion(unit).map(|(_, factor)| factor)
}

/// `math.unit(1px)` → `"px"`，`math.unit(1)` → `""`，以及语料给出的复合形态（`"px*em*rad"`、`"px/em"`、
/// `"px^-1"`、`"px*em/(rad*s)"`）——它们全部出自 [`Unit::to_css`]，那是单位变成文本的唯一一处。
static MATH_UNIT: Builtin = Builtin {
    name: "math.unit",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        number_arg("number", &args).map(|(_, unit)| Value::Raw(format!("\"{}\"", unit.to_css())))
    },
};

/// `math.is-unitless(1px)` → `false`——那个文件叫 `unitless.hrx`，但语料调用的函数是 `math.is-unitless`；
/// 照着文件命名就会实现出一个什么都做不成的东西。
static MATH_IS_UNITLESS: Builtin = Builtin {
    name: "math.is-unitless",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| number_arg("number", &args).map(|(_, unit)| Value::Bool(unit.is_unitless())),
};

/// `math.percentage(0.246)` → `24.6%`。实参上带单位时用的是语料自己的消息
/// （`$number: Expected 1% to have no units.`——一个单位也照样写复数“units”，照原文），而结果只在 Sass 对
/// 算出的数字的十位有效数字输出里才合得上。
static MATH_PERCENTAGE: Builtin = Builtin {
    name: "math.percentage",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (value, unit) = number_arg("number", &args)?;
        if unit.is_unitless() {
            Ok(Value::Number(value * 100.0, Unit::named("%")))
        } else {
            Err(SassError::typed(
                ErrorKind::WrongUnits,
                format!(
                    "$number: Expected {} to have no units.",
                    Value::Number(value, unit).to_css()
                ),
            ))
        }
    },
};

/// `math.compatible(1px, 2in)` → `true`：两个数字在带同一单位、任一侧无单位、或两个单位都属于语料证实过的
/// 某个量纲时可比。一个未知的单位**decline**而不是答 `false`，因为 `px` 对 `q` 不是 fixture 说过的东西。每个
/// 参数都在自己的名字下被校验，正如 `comparable.hrx::error/type/*` 要求的（`$number1: c is not a number.`、
/// `$number2: c is not a number.`）。
static MATH_COMPATIBLE: Builtin = Builtin {
    name: "math.compatible",
    params: &["number1", "number2"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (Some(first), Some(second)) = (args.first(), args.get(1)) else {
            return Err(missing("number2".to_string()));
        };
        let (_, first_unit) = number_arg("number1", std::slice::from_ref(first))?;
        let (_, second_unit) = number_arg("number2", std::slice::from_ref(second))?;
        if first_unit == second_unit || first_unit.is_unitless() || second_unit.is_unitless() {
            return Ok(Value::Bool(true));
        }
        // 比较走的是**量纲签名**——分数两侧各自的量纲多重集——所以一个倒数单位会答而不是 decline：
        // `math.compatible(1px, 1/1px)` 是 `false`（`comparable.hrx::unit/to_inverse`），而 `1px` 对 `2em`
        // 仍是 `false`（`…::unit/to_different`），那张证实过的表之外的任何单位依旧 decline。
        match (dimension(&first_unit), dimension(&second_unit)) {
            (Some(left), Some(right)) => Ok(Value::Bool(left == right)),
            _ => Err(SassError::new("unit dimension not proven")),
        }
    },
};

/// `math.max(1px, 1in, 1cm)` → `1in`：实参按单位定律比较，而**原始**值胜出（它不会被改写成某个公共单位）。
///
/// 长度的因子来自归档自己（`libsass/units/conversion/size.hrx`：`1in` → `96px`、`1cm` →
/// `37.7952755906px`、`1mm` → `3.7795275591px`、`1pc` → `16px`、`1pt` → `1.3333333333px`），而
/// `max.hrx::error/incompatible_units` 说 `px` 与 `s` 是不同量纲。两个**已知的**不同量纲产出语料那句消息；
/// 一个没列出的单位则退回逐字文本而不是发明那次错误，因为 `1px and 2q have incompatible units.` 不是
/// fixture 断言过的东西，而一个未知单位不是不相容的证据。
static MATH_MIN: Builtin = Builtin {
    name: "math.min",
    params: &[],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| extremum(Ordering::Less, args),
};

static MATH_MAX: Builtin = Builtin {
    name: "math.max",
    params: &[],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| extremum(Ordering::Greater, args),
};

/// 一个极值往哪个方向挑。一个具名的选择让 `min` 与 `max` 共用一个函数体，而不是两份会互相漂移的副本。
#[derive(Clone, Copy, PartialEq)]
enum Ordering {
    Less,
    Greater,
}

/// 把实参折叠到最小或最大，逐对按单位定律比较。
fn extremum(wants: Ordering, args: Vec<Value>) -> Result<Value, SassError> {
    let numbers = args
        .into_iter()
        .map(|value| match value {
            Value::Number(number, unit) => Ok((number, unit)),
            other => Err(SassError::typed(
                ErrorKind::NotANumber,
                format!("{} is not a number.", other.to_css()),
            )),
        })
        .collect::<Result<Vec<(f64, Unit)>, SassError>>()?;
    let (head, tail) = numbers.split_first().ok_or_else(super::at_least_one)?;
    tail.iter()
        .try_fold(head.clone(), |best, candidate| {
            comparable(&best, candidate).map(|(left, right)| {
                match (wants, left.partial_cmp(&right)) {
                    (Ordering::Less, Some(core::cmp::Ordering::Greater)) => candidate.clone(),
                    (Ordering::Greater, Some(core::cmp::Ordering::Less)) => candidate.clone(),
                    _ => best,
                }
            })
        })
        .map(|(number, unit)| Value::Number(number, unit))
}

/// 要比较的两个大小，处于同一个尺度上；或者它们根本不能比较的理由。每个会给数字排序的函数都读这一个读数，
/// 好让定律不会在 `min`/`max`、`math.compatible`、`math.clamp` 与 `math.hypot` 之间漂移。
enum Comparison {
    /// 按原样可比，两个大小都在调用方已有的尺度里。
    Ordered(f64, f64),
    /// 恰好一侧无单位——`min`/`max` 照样比它们（`max.hrx::units/and_unitless` 对 `math.max(2px, 1)` 给出 `2px`），
    /// 而 `clamp`/`hypot` 拒收：`… have incompatible units (one has units and the other doesn't).`
    /// （`clamp.hrx::error/some_unitless/*`）。
    PartlyUnitless,
    /// 两个**已知的**不同族：`1px and 2s have incompatible units.`（`max.hrx::error/incompatible_units`）。
    Clash,
    /// [`crate::unit::conversion`] 之外的一个族：没有任何东西证实 `1px` 对 `1c`，所以调用方 decline 而不是
    /// 发明一次冲突。
    Unproven,
}

fn compare(a: &(f64, Unit), b: &(f64, Unit)) -> Comparison {
    if a.1 == b.1 || (a.1.is_unitless() && b.1.is_unitless()) {
        return Comparison::Ordered(a.0, b.0);
    }
    if a.1.is_unitless() != b.1.is_unitless() {
        return Comparison::PartlyUnitless;
    }
    match (dimension(&a.1), dimension(&b.1)) {
        (Some(left), Some(right)) if left == right => match (
            a.1.single().and_then(to_base),
            b.1.single().and_then(to_base),
        ) {
            (Some(from_a), Some(from_b)) => Comparison::Ordered(a.0 * from_a, b.0 * from_b),
            // 一个同族的复合单位（`1px*em / 1em` 那种形状）没有一个可用于缩放的单一因子。
            _ => Comparison::Unproven,
        },
        (Some(_), Some(_)) => Comparison::Clash,
        _ => Comparison::Unproven,
    }
}

/// 要比较的两个大小，处于同一个尺度上：同单位（或任一侧无单位）按原样比，已知可换算的同族单位被缩放，而已知
/// 的不同族就是语料那句 `1px and 2s have incompatible units.`——其余一律 decline。`min`/`max` 读这个；
/// `clamp`/`hypot` 读 [`compare_named`]，它把两个参数名都留在消息里。
fn comparable(a: &(f64, Unit), b: &(f64, Unit)) -> Result<(f64, f64), SassError> {
    match compare(a, b) {
        Comparison::Ordered(left, right) => Ok((left, right)),
        // 一个无单位的邻居在这里按原样可比，这正是 `math.max(2px, 1)` → `2px` 需要的。
        Comparison::PartlyUnitless => Ok((a.0, b.0)),
        Comparison::Clash => Err(SassError::typed(
            ErrorKind::IncompatibleUnits,
            format!(
                "{} and {} have incompatible units.",
                Value::Number(a.0, a.1.clone()).to_css(),
                Value::Number(b.0, b.1.clone()).to_css()
            ),
        )),
        Comparison::Unproven => Err(SassError::new("unit comparison not modelled yet")),
    }
}

/// [`compare`] 的拒语里带上两个参数名，照 `clamp.hrx::error/*` 与 `hypot.hrx::error/*` 给出的写法——左边的
/// 先出现：`$number: 1px and $min: 1deg have incompatible units.`，以及
/// `$max: 2px and $min: 0 have incompatible units (one has units and the other doesn't).`
pub(super) fn compare_named(
    left: (&str, &(f64, Unit)),
    right: (&str, &(f64, Unit)),
) -> Result<(f64, f64), SassError> {
    let (left_name, (left_value, left_unit)) = left;
    let (right_name, (right_value, right_unit)) = right;
    let named = |clause: &str| {
        SassError::typed(
            ErrorKind::IncompatibleUnits,
            format!(
                "${left_name}: {} and ${right_name}: {} have incompatible units{clause}.",
                Value::Number(*left_value, left_unit.clone()).to_css(),
                Value::Number(*right_value, right_unit.clone()).to_css()
            ),
        )
    };
    match compare(
        &(*left_value, left_unit.clone()),
        &(*right_value, right_unit.clone()),
    ) {
        Comparison::Ordered(scaled_left, scaled_right) => Ok((scaled_left, scaled_right)),
        Comparison::PartlyUnitless => Err(named(" (one has units and the other doesn't)")),
        Comparison::Clash => Err(named("")),
        Comparison::Unproven => Err(SassError::new("unit comparison not modelled yet")),
    }
}

/// 一个单位的量纲签名：每个名字所属的族，写在分数两侧，保持声明顺序。每个族都出自那张共享表
/// （[`crate::unit::conversion`]）；`em` 在这里点名，是因为 `comparable.hrx::unit/to_different` 说
/// `math.compatible(1px, 2em)` → `false`，而 `em` 没有一个可带的换算因子——没有别的相对单位有这样一行，
/// 所以这里也不加。`px` 对 `c`、`q`、`rem` 等等会让签名停留在未证实状态，每个本要判定的函数都 decline。
pub(super) fn dimension(unit: &Unit) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    let group = |name: &str| match name {
        "em" => Some("em"),
        _ => crate::unit::conversion(name).map(|(family, _)| family),
    };
    let (numerator, denominator) = unit.names();
    Some((
        numerator
            .iter()
            .map(|name| group(name))
            .collect::<Option<Vec<&'static str>>>()?,
        denominator
            .iter()
            .map(|name| group(name))
            .collect::<Option<Vec<&'static str>>>()?,
    ))
}

/// 取整一族共用一个参数名与一个类型契约，两句都是语料自己说的：`Missing argument $number.` 与
/// `$number: c is not a number.`（`core_functions/math/{round,abs,ceil,floor}.hrx::error/*`）。它们的单位被
/// 保留（`7px / 4em` 那种情形需要复合单位，够不着），而这里每个无单位的金样都被逐字对上：`round(2.2)` → `2`、
/// `round(-5.6)` → `-6`、`round(0.2)` → `0`、`abs(-17)` → `17`、`ceil(-1.2)` → `-1`、`floor(-1.2)` → `-2`。
static MATH_ROUND: Builtin = Builtin {
    name: "math.round",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        number_arg("number", &args).map(|(value, unit)| {
            // 一个**本身不是**零的值取整落到零时，语料把它打成普通的 `0`：`math.round(-0.2)` → `0`
            // （`core_functions/math/round.hrx::up/to_zero`）。而一个超越函数拿到 `-0.0` 时保留符号
            // （`math.sin(-0.0)` → `-0`，`sin.hrx::negative_zero`）——所以这个区别住在运算里，不在打印机里。
            let rounded = value.round();
            Value::Number(if rounded == 0.0 { 0.0 } else { rounded }, unit)
        })
    },
};

static MATH_ABS: Builtin = Builtin {
    name: "math.abs",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| number_arg("number", &args).map(|(value, unit)| Value::Number(value.abs(), unit)),
};

static MATH_CEIL: Builtin = Builtin {
    name: "math.ceil",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        number_arg("number", &args).map(|(value, unit)| Value::Number(value.ceil(), unit))
    },
};

static MATH_FLOOR: Builtin = Builtin {
    name: "math.floor",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        number_arg("number", &args).map(|(value, unit)| Value::Number(value.floor(), unit))
    },
};

/// 一个必须是数字的参数。消息是归档自己的，而值以 CSS 形式渲染，所以一个带引号的字符串保住它的引号
/// （`$number: "0" is not a number.`），不带引号的标识符则不带（`$number: c is not a number.`）。
///
/// 不是每个内置函数都用它：`math.div` 拿到非数字时发出的是一条 deprecation **warning** 并且仍然产出一个值
/// （`div.hrx::non_numeric/numerator`），所以它的函数体是 decline 而不是失败——这个差别住在函数体里，因为
/// 语料正是在那里分岔。
fn number_arg(param: &str, bound: &[Value]) -> Result<(f64, Unit), SassError> {
    // 两种失败是两句不同的消息，所以它们保持两个分支：一个缺席的实参是 `Missing argument $number.`
    // （`core_functions/math/round.hrx::error/too_few_args`），一个在场却不是数字的实参是
    // `$number: c is not a number.`（`…::error/type`）。一个内容已经折叠起来的 calculation，
    // 算作它到达的那个数字（`Value::number`）。
    let Some(bound) = bound.first() else {
        return Err(SassError::typed(
            ErrorKind::MissingArgument,
            format!("Missing argument ${param}."),
        ));
    };
    match bound.number() {
        Some(Value::Number(value, unit)) => Ok((value, unit)),
        _ => Err(SassError::typed(
            ErrorKind::NotANumber,
            format!("${param}: {} is not a number.", bound.to_css()),
        )),
    }
}
