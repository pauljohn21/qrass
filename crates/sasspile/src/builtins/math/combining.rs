//! 两个会把若干数字互相排序的函数：`math.clamp`（`core_functions/math/clamp.hrx`）与
//! `math.hypot`（`…/hypot.hrx`）。
//!
//! 两者都读 [`super::compare_named`]，而不是再发明第二次比较，因为它们的拒语点名**两个**参数，而 `min`/`max`
//! 一个都不点（`max.hrx::error/incompatible_units` 是 `1px and 2s have incompatible units.`）：
//! * `math.clamp(1deg, 1px, 1turn)` → `$number: 1px and $min: 1deg have incompatible units.`，
//!   `math.clamp(1deg, 1turn, 1px)` → `$max: 1px and $min: 1deg …`（`clamp.hrx::error/incompatible_units/*`）
//!   —— 先查“中间对下界”那次比较，再查“上界对下界”，这决定了*哪一对*被点名；
//! * `math.hypot(1deg, 1px, 1turn)` → `$numbers[2]: 1px and $numbers[1]: 1deg …`
//!   （`hypot.hrx::error/incompatible_units/*`），每个后来的实参都与第一个比。

use super::{compare_named, missing, number_arg, to_base};
use crate::builtins::Builtin;
use crate::value::Value;
use crate::{ErrorKind, SassError};

/// `math.clamp($min, $number, $max)` 的折叠顺序是**先从上界往里、再从下界往外**，这是那些行唯一达成一致
/// 的顺序：`clamp(0, 2, 1)` → `1` 与 `clamp(1, 2, 0)` → `1`
/// （`clamp.hrx::{chooses_max,min_greater_than_max}`）—— 一个低于最小值的最大值仍然给出最小值，所以一条各自
/// 独立的比较链会在第二行返回 `0`。
///
/// 胜出者**按原样**返回，不会被改写成某个公共单位；而 `clamp.hrx::preserves_units/*` 那三行定住了平局时
/// 哪一侧胜出：`clamp(180deg, 0.75turn, 360deg)` → `0.75turn`（在两个界限之内，所以是那个数），
/// `clamp(180deg, 1turn, 360deg)` → `360deg`（等于上界，所以是上界），`clamp(180deg, 0.5turn, 360deg)` →
/// `180deg`（等于下界，所以是下界）。所以下面两次比较用的是 `<` 与 `<=`，而不是同一个算子用两遍。
pub(super) static MATH_CLAMP: Builtin = Builtin {
    name: "math.clamp",
    params: &["min", "number", "max"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (min, number, max) = match args.as_slice() {
            [min, number, max] => (min, number, max),
            // 一次过短或过长的调用在函数体看见它之前就被登记表拒掉了
            // （`clamp.hrx::error/{zero_args,too_many_args}`）；这一臂只是让这次读取保持完整。
            _ => return Err(missing("min".to_string())),
        };
        let min = number_arg("min", std::slice::from_ref(min))?;
        let number = number_arg("number", std::slice::from_ref(number))?;
        let max = number_arg("max", std::slice::from_ref(max))?;
        compare_named(("number", &number), ("min", &min))?;
        compare_named(("max", &max), ("min", &min))?;
        let on_base =
            |value: f64, unit: &super::Unit| value * unit.single().and_then(to_base).unwrap_or(1.0);
        let chosen = if on_base(number.0, &number.1) < on_base(max.0, &max.1) {
            number
        } else {
            max
        };
        // 是 `<=` 不是 `<`：`clamp(180deg, 0.5turn, 360deg)` 返回的是**最小值**（`180deg`），所以一个等于下界
        // 的数会被它换掉（`…::preserves_units/min`）。
        let chosen = if on_base(chosen.0, &chosen.1) <= on_base(min.0, &min.1) {
            min
        } else {
            chosen
        };
        Ok(Value::Number(chosen.0, chosen.1))
    },
};

/// `math.hypot($numbers…)`：平方和的根，在该族的基准上算出，并以**第一个实参的单位**返回 ——
/// `math.hypot(3cm, 4mm * 10, 5q * 40, math.div(6in, 2.54), 7px * math.div(96, 2.54))` → `11.6189500386cm`
/// （`hypot.hrx::compatible_units`），就是这一行把 `math.rs` 里的长度表交给了 [`crate::unit`] 里那张共享表
/// （`q` 只在后者里）。
///
/// 它的类型消息不点名任何参数，与 `min`/`max` 完全一样（`math.hypot("0", 1px, 1px)` → `"0" is not a number.`），
/// 而一个无穷大的实参会让整个根变成无穷大（`math.hypot(math.div(1, 0), 1, 1)` → `calc(infinity)`，
/// `hypot.hrx::infinity/*`）。
pub(super) static MATH_HYPOT: Builtin = Builtin {
    name: "math.hypot",
    params: &[],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| {
        let numbers = args
            .into_iter()
            .map(|value| match value {
                Value::Number(number, unit) => Ok((number, unit)),
                other => Err(SassError::typed(
                    ErrorKind::NotANumber,
                    format!("{} is not a number.", other.to_css()),
                )),
            })
            .collect::<Result<Vec<(f64, super::Unit)>, SassError>>()?;
        let (first_value, first_unit) = numbers
            .first()
            .cloned()
            .ok_or_else(crate::builtins::at_least_one)?;
        let on_base =
            |value: f64, unit: &super::Unit| value * unit.single().and_then(to_base).unwrap_or(1.0);
        let squared = numbers
            .iter()
            .enumerate()
            .try_fold(0.0f64, |sum, (index, candidate)| {
                if index > 0 {
                    // 从 1 开始数，照归档的数法：`$numbers[2]`、`$numbers[1]`。
                    let name = format!("numbers[{}]", index + 1);
                    compare_named(
                        (name.as_str(), candidate),
                        ("numbers[1]", &(first_value, first_unit.clone())),
                    )?;
                }
                let scaled = on_base(candidate.0, &candidate.1);
                Ok(sum + scaled * scaled)
            })?;
        let root = squared.sqrt();
        Ok(Value::Number(
            root / first_unit.single().and_then(to_base).unwrap_or(1.0),
            first_unit,
        ))
    },
};
