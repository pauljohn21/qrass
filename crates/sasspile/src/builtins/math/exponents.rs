//! `math` 域的指数那一半：`core_functions/math/{sqrt,log}.hrx` 与 `pow/` 那个目录
//! （`arguments.hrx` 与 `base_*.hrx`）。
//!
//! 三个都要求自己的实参**不带单位** —— 拒语是 [`super::unitless`]，与三角反函数读的是同一个，而它会点名失败
//! 的那个参数：`math.pow(1px, 0)` → `$base: Expected 1px to have no units.`，`math.pow(0, 1px)` →
//! `$exponent: …`（`pow/arguments.hrx::error/{base,exponent}_has_units`），而 `math.log(1, 1px)` → `$base: …`
//! （`log.hrx::error/base_has_units`）。
//!
//! fixture 在那之上补充的：
//! * `math.pow(2, 3)` → `8`，而 `math.pow($base: 2, $exponent: 3)` → `8`（`pow/arguments.hrx::named_args`）；
//!   那些 `pow/base_*/…` 文件是 IEEE 的零/符号情形 —— `math.pow(0, -2)` → `calc(infinity)`，
//!   `math.pow(-0.0, -3)` → `calc(-infinity)`，而它那个模糊的邻居 `-3.000000000001` 又回到
//!   `calc(infinity)`（`pow/base_negative_zero.hrx::with_exponent/*`）；
//! * `math.sqrt(-0.0)` → `-0`（`sqrt.hrx::negative_zero`）—— 一个被携带的符号，不是算出来的 —— 而
//!   `math.sqrt(-1)` 与那个模糊的 `-0.000000000001` 是 `calc(NaN)`；
//! * `math.log` 接受一个可选的 `$base`，而 `math.log(2, null)` 就是自然对数：语料用一个写成 `null` 的第二个
//!   实参表示“缺席”（`log.hrx::base/null`），而 `math.log(2, 10)` → `0.3010299957`（`log.hrx::base/positive`）。

use super::{missing, number_arg, unitless};
use crate::builtins::Builtin;
use crate::unit::Unit;
use crate::value::Value;

pub(super) static MATH_POW: Builtin = Builtin {
    name: "math.pow",
    params: &["base", "exponent"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (base, base_unit) = number_arg("base", &args)?;
        let Some(exponent) = args.get(1) else {
            // `math.pow(0)` 由登记表拒掉 —— `Missing argument $exponent.`
            // （`pow/arguments.hrx::error/one_arg`）；这一臂只是让这次读取保持完整，因为一个定长签名不可能
            // 缺着第二个参数进到函数体里。
            return Err(missing("exponent".to_string()));
        };
        let (exponent, exponent_unit) = number_arg("exponent", std::slice::from_ref(exponent))?;
        let base = unitless("base", base, base_unit)?;
        let exponent = unitless("exponent", exponent, exponent_unit)?;
        Ok(Value::Number(base.powf(exponent), Unit::unitless()))
    },
};

pub(super) static MATH_SQRT: Builtin = Builtin {
    name: "math.sqrt",
    params: &["number"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        number_arg("number", &args)
            .and_then(|(value, unit)| unitless("number", value, unit))
            .map(|value| Value::Number(value.sqrt(), Unit::unitless()))
    },
};

pub(super) static MATH_LOG: Builtin = Builtin {
    name: "math.log",
    params: &["number"],
    optional: &["base"],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (value, unit) = number_arg("number", &args)?;
        let value = unitless("number", value, unit)?;
        // 语料用写下来的 `null` 表示“没有 base”，所以它是被丢掉而不是被读取：`math.log(2, null)` →
        // `0.6931471806`，与 `math.log(2)` 是同一个数。
        let base = match args.get(1) {
            Some(held) if held.to_css() == "null" => None,
            Some(held) => {
                let (base, unit) = number_arg("base", std::slice::from_ref(held))?;
                Some(unitless("base", base, unit)?)
            }
            None => None,
        };
        Ok(Value::Number(
            match base {
                Some(base) => value.ln() / base.ln(),
                None => value.ln(),
            },
            Unit::unitless(),
        ))
    },
};
