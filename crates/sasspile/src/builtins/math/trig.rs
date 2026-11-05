//! `math` 域的三角函数那一半 ——
//! `core_functions/math/{sin,cos,tan,asin,acos,atan}.hrx` 与 `…/atan2/*`。
//!
//! 从父模块里拆出来，是因为这七个共享两条别的 `math` 函数体都不读的契约 —— 要么输入一个**角度**，要么输入
//! 一个**比值**、输出一个**度** —— 也因为父文件是一般性的数字/单位定律，不是一张函数表。
//!
//! fixture 定下的事：
//! * `sin`/`cos`/`tan` 拿到一个无单位的实参时，它**就是弧度**：`math.cos(1)` 与 `math.cos(1rad)` 都给出
//!   `0.5403023059`（`cos.hrx::{unitless,rad}`），而 `1deg` → `0.9998476952`、`1grad` → `0.9998766325` ——
//!   角度的因子就是 `crate::unit` 为色彩色相已经带着的那几个；
//! * 任何其他单位都被点名拒收：`$number: Expected 1px to have an angle unit (deg, grad, rad, turn).`
//!   （`cos.hrx::error/unit`，`sin`/`tan` 里同一行）；
//! * 反函数那三个**完全不要单位**（`$number: Expected 1px to have no units.`，`asin.hrx::error/units`），
//!   并以度作答（`math.atan(1)` → `45deg`、`math.asin(-0.5)` → `-30deg`、`math.acos(1)` → `0deg`）；
//! * `math.atan2($y, $x)` 要求两个实参同属一个量纲 —— `math.atan2(1cm, -10mm)` → `135deg` 是先换算的
//!   （`atan2/arguments.hrx::compatible_units`），它也以度作答。

use super::{comparable, number_arg, unitless};
use crate::ErrorKind;
use crate::SassError;
use crate::builtins::Builtin;
use crate::unit::Unit;
use crate::value::Value;

/// 一次 `sin`/`cos`/`tan` 调用的角度，以弧度计。无单位的实参本身就是弧度了，角度单位经由色相访问器用的那张表
/// 换算（`1grad` = `0.9deg`），其余一切都是语料自己的拒语 —— 而那是一条**致命**消息，所以 `math.cos(1px)`
/// 不会打印出来：`Error: $number: Expected 1px to have an angle unit (deg, grad, rad, turn).`
fn radians(args: &[Value]) -> Result<f64, SassError> {
    let (value, unit) = number_arg("number", args)?;
    if unit.is_unitless() {
        return Ok(value);
    }
    let angle = unit
        .single()
        .filter(|name| crate::unit::is_angle(name))
        .map(|name| (value * crate::unit::measure(name).1).to_radians());
    angle.ok_or_else(|| {
        SassError::typed(
            ErrorKind::WrongUnits,
            format!(
                "$number: Expected {} to have an angle unit (deg, grad, rad, turn).",
                Value::Number(value, unit).to_css()
            ),
        )
    })
}

/// 反函数要的无单位比值：它们以度作答，所以拿一个角度进去根本不是语料问过的问题
/// （`asin.hrx::error/units` 就是 `$number: Expected 1px to have no units.`）。
fn ratio(args: &[Value]) -> Result<f64, SassError> {
    let (value, unit) = number_arg("number", args)?;
    unitless("number", value, unit)
}

/// 一个正三角函数：角度进、普通数字出（`math.sin(1deg)` → `0.0174524064` —— Sass 的十位有效数字，而
/// [`crate::value::format_number`] 对一个算出的值本来就是这样给的）。
fn direct(value: f64, take: fn(f64) -> f64) -> Result<Value, SassError> {
    Ok(Value::Number(take(value), Unit::unitless()))
}

/// 一个反三角函数：比值进、度出（`math.atan(-1)` → `-45deg`）。一个越界的比值（`math.asin(2)`，甚至
/// `math.asin(1.000000000001)`）是 `NaN` —— 语料把它打成 `calc(NaN * 1deg)`，那层壳属于本步还够不着的
/// 非有限数打印律；而这里给出的值，正是越界作答所要的那一个。
fn inverse(value: f64, take: fn(f64) -> f64) -> Result<Value, SassError> {
    Ok(Value::Number(take(value).to_degrees(), Unit::named("deg")))
}

pub(super) static MATH_SIN: Builtin = Builtin {
    name: "math.sin",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| radians(&args).and_then(|angle| direct(angle, f64::sin)),
};

pub(super) static MATH_COS: Builtin = Builtin {
    name: "math.cos",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| radians(&args).and_then(|angle| direct(angle, f64::cos)),
};

pub(super) static MATH_TAN: Builtin = Builtin {
    name: "math.tan",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| radians(&args).and_then(|angle| direct(angle, f64::tan)),
};

pub(super) static MATH_ASIN: Builtin = Builtin {
    name: "math.asin",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| ratio(&args).and_then(|value| inverse(value, f64::asin)),
};

pub(super) static MATH_ACOS: Builtin = Builtin {
    name: "math.acos",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| ratio(&args).and_then(|value| inverse(value, f64::acos)),
};

pub(super) static MATH_ATAN: Builtin = Builtin {
    name: "math.atan",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| ratio(&args).and_then(|value| inverse(value, f64::atan)),
};

/// `math.atan2($y, $x)`：两个实参必须同属一个量纲 —— 在 `atan2/arguments.hrx::compatible_units` 里它们是
/// **长度**（`1cm` 对 `-10mm` → `135deg`，所以尺度相消、只有比值要紧），任一侧可以无单位，而两句拒语不同：
/// 两个量纲时是 `$x: 1deg and $y: 1px have incompatible units.`，只有一侧带单位时是
/// `$x: 1 and $y: 1px have incompatible units (one has units and the other doesn't).`。两句里都是 `x` 先被
/// 点名，无论调用顺序如何（`…::arguments.hrx::error/*`）。
pub(super) static MATH_ATAN2: Builtin = Builtin {
    name: "math.atan2",
    params: &["y", "x"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (Some(y), Some(x)) = (args.first(), args.get(1)) else {
            return Err(crate::builtins::missing("y".to_string()));
        };
        let (y, y_unit) = number_arg("y", std::slice::from_ref(y))?;
        let (x, x_unit) = number_arg("x", std::slice::from_ref(x))?;
        let (y, x) = comparable(&(y, y_unit.clone()), &(x, x_unit.clone())).map_err(|why| {
            // 那条通用比较消息只把值写出来、不点名参数（`1px and 2s have incompatible units.`，见 `max.hrx`）；
            // `atan2` 则按参数陈述它，`x` 在前（`atan2(1px, 1deg)` 拿到的是
            // `$x: 1deg and $y: 1px have incompatible units.`）。
            match why.kind {
                ErrorKind::IncompatibleUnits => SassError::typed(
                    ErrorKind::IncompatibleUnits,
                    format!(
                        "$x: {} and $y: {} have incompatible units.",
                        Value::Number(x, x_unit.clone()).to_css(),
                        Value::Number(y, y_unit.clone()).to_css()
                    ),
                ),
                _ => why,
            }
        })?;
        if y_unit.is_unitless() != x_unit.is_unitless() {
            return Err(SassError::typed(
                ErrorKind::IncompatibleUnits,
                format!(
                    "$x: {} and $y: {} have incompatible units (one has units and the other doesn't).",
                    Value::Number(x, x_unit).to_css(),
                    Value::Number(y, y_unit).to_css()
                ),
            ));
        }
        Ok(Value::Number(y.atan2(x).to_degrees(), Unit::named("deg")))
    },
};
