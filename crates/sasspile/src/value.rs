//! Value — the **owned value** a declaration carries forward through the push stream, plus
//! the unit algebra that arithmetic produces.
//!
//! P0 laid the floor: a rendered token run became [`Value::Raw`] and
//! [`Value::to_css`] emitted it verbatim, so the value path was already shaped as
//! "structured value moves through operators, serialises once at the end" instead of
//! "string travels through the pipeline and gets re-parsed". P1 adds
//! [`Value::Number`] — a magnitude with a [`crate::unit::Unit`], which since 1.4d is a rational
//! (compound) unit rather than a name — and the conversion table that the arithmetic in
//! [`crate::expr`] folds through. The rest of the algebra still arrives
//! stage by stage (`Color`, `List` with space/comma/slash separators,
//! insertion-ordered `Map`, `String` with quote state, `Null`/`Argument`/`Module`) per
//! `openspec/changes/.../specs/value-types`.
//!
//! The type stays crate-internal (`mod value` is private in the crate root) — behaviour is
//! observable only through the sole `compile` exit, never by leaking a `pub` for tests.

use crate::SassError;
use crate::unit::{Unit, measure};

/// An owned declaration value flowing forward through the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Source text carried to CSS unchanged (anything P1 does not evaluate yet).
    Raw(String),
    /// A magnitude with its unit; a unitless number carries [`Unit::unitless`], `%` is a percentage, and
    /// a compound unit (`px*em/(rad*s)`) is the result of `*` and `/` rather than of a lexeme.
    Number(f64, Unit),
    /// Sass's boolean, printed as the bare word `true` / `false` (`math.is-unitless`, `math.compatible`,
    /// and later `@if`'s truthiness). Kept a separate variant so a boolean is never re-parsed out of text.
    Bool(bool),
    /// A **calculation** whose content this slice cannot simplify: `name` is the normalised (lowercase)
    /// function and `inner` the text of its argument list, already normalised at parse time
    /// (`crate::calculation`). The first slice of tasks 1.4 lands the *form* — the expression kept as
    /// written — so a `calc` that used to travel as one opaque `Raw` is now a value with a name and a
    /// body. Simplification (`+` between compatible units, then `*`/`/`) arrives with 1.4d's rational
    /// units, which is why no arithmetic path reads this variant yet: a calculation used as an operand
    /// declines, exactly as it did before.
    Calculation(String, String),
}

impl Value {
    /// Adopt an already-rendered declaration value (the lexer token run is one producer;
    /// the other is [`Value::Number`] from an evaluated expression).
    pub fn from_text(text: String) -> Self {
        Value::Raw(text)
    }

    /// Serialise one value to CSS text — the single place a value becomes a string.
    pub fn to_css(&self) -> String {
        match self {
            Value::Raw(text) => text.clone(),
            Value::Number(value, unit) => format_number(*value) + &unit.to_css(),
            Value::Bool(value) => value.to_string(),
            Value::Calculation(name, inner) => format!("{name}({inner})"),
        }
    }

    /// `a + b` in Sass: convert `b` into `a`'s unit, adopting a unitless side's counterpart.
    pub fn plus(&self, rhs: &Value) -> Result<Value, SassError> {
        self.combine(rhs, |left, right| left + right)
    }

    /// `a - b` in Sass, with the same unit laws as addition.
    pub fn minus(&self, rhs: &Value) -> Result<Value, SassError> {
        self.combine(rhs, |left, right| left - right)
    }

    /// `a * b`: the magnitudes multiply and the units combine, so `px * em` is `px*em` while
    /// `px/em * em` is `px` — the cancellation is structural, in [`Unit::times`].
    pub fn times(&self, rhs: &Value) -> Result<Value, SassError> {
        self.operate(rhs, |left, right| left * right, Unit::times)
    }

    /// `a / b`: the magnitudes divide and the divisor's units flip sides, so `6 / 3px` is `2/px`.
    ///
    /// A zero divisor is **not** a special case here: f64 division yields infinity and
    /// `format_number` already spells it the way Sass does (`math.div(1, 0)` → `infinity`,
    /// `values/calculation/calc/constant::infinity`), so no panic and no invented `NaN`.
    pub fn divided_by(&self, rhs: &Value) -> Result<Value, SassError> {
        self.operate(rhs, |left, right| left / right, Unit::over)
    }

    /// The shape `*` and `/` share: both sides must be numbers, the magnitudes go through one operation
    /// and the units through one combination. Unlike `+`/`-` there is no conversion or adoption step —
    /// `1px * 1s` is a compound unit, not an error.
    fn operate(
        &self,
        rhs: &Value,
        magnitude: impl Fn(f64, f64) -> f64,
        units: impl Fn(&Unit, &Unit) -> Unit,
    ) -> Result<Value, SassError> {
        let (Value::Number(left, left_unit), Value::Number(right, right_unit)) = (self, rhs) else {
            return Err(SassError::new("Undefined operation."));
        };
        Ok(Value::Number(
            magnitude(*left, *right),
            units(left_unit, right_unit),
        ))
    }

    /// The unit law behind `+`/`-`: identical units operate directly, a unitless side adopts
    /// the counterpart's unit, convertible single units are normalised into the **left** operand's
    /// unit (so `1px + 1in` is `97px`), and anything else — including a compound unit that has no
    /// single-unit counterpart to line up with — is a Sass error.
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
        // Conversion is a single-unit affair: the tables key on unit names, so a compound side has no
        // proven factor and the operands stay incompatible rather than being half-converted.
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

/// Sass number output: integers lose the fraction, everything else prints at the shortest
/// round-trip precision (so `1in + 1px` is `97px`, and a parsed `f64::MAX` literal keeps
/// the 17 digits that actually identify it).
fn format_number(value: f64) -> String {
    if value.is_infinite() {
        // Sass spells the infinities `infinity` / `-infinity`; Rust's `Display` gives `inf`, which CSS
        // would not read (`values/calculation/calc/constant::infinity/math/unsimplified/from_variable`).
        return if value > 0.0 {
            "infinity".to_string()
        } else {
            "-infinity".to_string()
        };
    }
    if value == 0.0 || (value.fract() == 0.0 && value.abs() < 1e15) {
        return format!("{}", value as i64);
    }
    let natural = format!("{value}");
    // Sass prints at most **10 significant digits** — a computed `0.246 × 100` must say `24.6`, not the
    // f64 tail `24.600000000000001` (`core_functions/math/percentage.hrx::small`). The clamp is applied
    // only when the natural rendering is actually longer than ten significant digits, so short values and
    // tiny ones like `0.000000000000001` (one significant digit) keep the text they already have.
    let significant = natural
        .trim_start_matches(['-', '+'])
        .replace('.', "")
        .trim_start_matches('0')
        .chars()
        .filter(|c| c.is_ascii_digit())
        .count();
    if significant <= 10 {
        return natural;
    }
    let exponent = value.abs().log10().floor().max(0.0) as i32;
    format!("{:.1$}", value, (10 - 1 - exponent).max(0) as usize)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
