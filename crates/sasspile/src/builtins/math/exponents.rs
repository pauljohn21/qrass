//! The exponent half of the `math` domain: `core_functions/math/{pow,sqrt,log}.hrx`.
//!
//! All three require their arguments to carry **no unit** — the refusal is [`super::unitless`], the same one
//! the inverse trigonometric functions read, and it names the parameter that failed:
//! `math.pow(1px, 0)` → `$base: Expected 1px to have no units.`, `math.pow(0, 1px)` → `$exponent: …`
//! (`pow/arguments.hrx::error/*_has_units`), and `math.log(1, 1px)` → `$base: …`
//! (`log.hrx::error/base_has_units`).
//!
//! What the fixtures add on top of that:
//! * `math.pow(2, 3)` → `8` and `math.pow($base: 2, $exponent: 3)` → `8` (`pow/arguments.hrx::named_args`),
//!   and the `pow/base_*/…` files are the IEEE zero/sign cases — `math.pow(0, -2)` → `calc(infinity)`,
//!   `math.pow(-0.0, -3)` → `calc(-infinity)` while its fuzzy neighbour `-3.000000000001` is
//!   `calc(infinity)` again (`pow/base_negative_zero.hrx::with_exponent/*`);
//! * `math.sqrt(-0.0)` → `-0` (`sqrt.hrx::negative_zero`) — a carried sign, not a computed one — while
//!   `math.sqrt(-1)` and the fuzzy `-0.000000000001` are `calc(NaN)`;
//! * `math.log` takes an optional `$base` and `math.log(2, null)` is the natural log: the corpus writes a
//!   `null` second argument for "absent" (`log.hrx::base/null`), and `math.log(2, 10)` → `0.3010299957`
//!   (`log.hrx::base/positive`).

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
            // `math.pow(0)` is refused by the registry — `Missing argument $exponent.`
            // (`pow/arguments.hrx::error/one_arg`); this arm only keeps the read total, since a fixed
            // signature cannot reach the body without its second parameter.
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
        // A written `null` is how the corpus passes "no base", so it is dropped rather than read:
        // `math.log(2, null)` → `0.6931471806`, the same number as `math.log(2)`.
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
