//! Two functions that order several numbers against each other: `math.clamp`
//! (`core_functions/math/clamp.hrx`) and `math.hypot` (`…/hypot.hrx`).
//!
//! Both read [`super::compare_named`] instead of inventing a second comparison, because their refusals name
//! **both** parameters while `min`/`max` name neither (`math.max.hrx::error/incompatible_units` is
//! `1px and 2s have incompatible units.`):
//! * `math.clamp(1deg, 1px, 1turn)` → `$number: 1px and $min: 1deg have incompatible units.`,
//!   `math.clamp(1deg, 1turn, 1px)` → `$max: 1px and $min: 1deg …` (`clamp.hrx::error/incompatible_units/*`)
//!   — the middle-against-lower comparison is checked first, then the upper against the lower, which is what
//!   decides *which* pair gets named;
//! * `math.hypot(1deg, 1px, 1turn)` → `$numbers[2]: 1px and $numbers[1]: 1deg …`
//!   (`hypot.hrx::error/incompatible_units/*`), each later argument compared against the first.

use super::{compare_named, missing, number_arg, to_base};
use crate::builtins::Builtin;
use crate::value::Value;
use crate::{ErrorKind, SassError};

/// `math.clamp($min, $number, $max)` folds **inward from the upper bound, then out from the lower**, which
/// is the only order the rows agree on: `clamp(0, 2, 1)` → `1` and `clamp(1, 2, 0)` → `1`
/// (`clamp.hrx::{chooses_max,min_greater_than_max}`) — a maximum below the minimum still yields the minimum,
/// so a plain chain of independent comparisons would return `0` for the second row.
///
/// The winner is returned **as written**, not rewritten into a common unit, and the three
/// `clamp.hrx::preserves_units/*` rows fix which side wins a tie: `clamp(180deg, 0.75turn, 360deg)` →
/// `0.75turn` (inside both bounds, so the number), `clamp(180deg, 1turn, 360deg)` → `360deg` (equal to the
/// upper bound, so the bound), `clamp(180deg, 0.5turn, 360deg)` → `180deg` (equal to the lower bound, so that
/// bound). Hence the two comparisons below use `<` and `<=` rather than the same operator twice.
pub(super) static MATH_CLAMP: Builtin = Builtin {
    name: "math.clamp",
    params: &["min", "number", "max"],
    optional: &[],
    variadic: false,
    labelled: None,
    apply: |args| {
        let (min, number, max) = match args.as_slice() {
            [min, number, max] => (min, number, max),
            // A short or long call is refused by the registry before the body sees it
            // (`clamp.hrx::error/{zero_args,too_many_args}`); this arm only keeps the read total.
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
        // `<=`, not `<`: `clamp(180deg, 0.5turn, 360deg)` returns the **minimum** (`180deg`), so a number
        // equal to the lower bound is replaced by it (`…::preserves_units/min`).
        let chosen = if on_base(chosen.0, &chosen.1) <= on_base(min.0, &min.1) {
            min
        } else {
            chosen
        };
        Ok(Value::Number(chosen.0, chosen.1))
    },
};

/// `math.hypot($numbers…)`: the root of the sum of squares, computed in the family's base and returned in
/// **the first argument's unit** — `math.hypot(3cm, 4mm * 10, 5q * 40, math.div(6in, 2.54),`
/// `7px * math.div(96, 2.54))` → `11.6189500386cm` (`hypot.hrx::units`), the row that sent the length table
/// in `math.rs` to the shared one in [`crate::unit`] (`q` is only in the latter).
///
/// Its type message names no parameter, exactly as `min`/`max` do (`math.hypot("0", 1px, 1px)` →
/// `"0" is not a number.`), while an infinite argument makes the whole root infinite
/// (`math.hypot(math.div(1, 0), 1, 1)` → `calc(infinity)`, `hypot.hrx::infinite/*`).
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
                    // 1-based, as the oracle counts them: `$numbers[2]`, `$numbers[1]`.
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
