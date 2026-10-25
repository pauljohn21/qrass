//! Value — the **owned value** a declaration carries forward through the push stream, plus
//! the unit algebra that arithmetic produces.
//!
//! P0 laid the floor: a rendered token run became [`Value::Raw`] and
//! [`Value::to_css`] emitted it verbatim, so the value path was already shaped as
//! "structured value moves through operators, serialises once at the end" instead of
//! "string travels through the pipeline and gets re-parsed". P1 adds
//! [`Value::Number`] — a magnitude with its unit text — and the conversion table the
//! arithmetic in [`crate::expr`] folds through. The rest of the algebra still arrives
//! stage by stage (`Calculation`, `Color`, `List` with space/comma/slash separators,
//! insertion-ordered `Map`, `String` with quote state, `Bool`/`Null`/`Argument`) per
//! `openspec/changes/.../specs/value-types`.
//!
//! The type stays crate-internal (`mod value` is private in the crate root) — behaviour is
//! observable only through the sole `compile` exit, never by leaking a `pub` for tests.

use std::f64::consts::PI;

use crate::SassError;

/// Convertible units as `(unit, family, factor to the family base)`. Sass converts only
/// inside one family, and only these ones: `em`/`rem`/`vh`/`vw`/`vmin`/`vmax`/`ch`/`ex`
/// are relative to a context Sass cannot see, so each stands in its own family and
/// combines only with itself.
const UNITS: &[(&str, (&str, f64))] = &[
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

/// An owned declaration value flowing forward through the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Source text carried to CSS unchanged (anything P1 does not evaluate yet).
    Raw(String),
    /// A magnitude with its unit text; `""` is unitless, `%` is a percentage.
    Number(f64, String),
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
            Value::Number(value, unit) => format_number(*value) + unit,
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

    /// The unit law behind `+`/`-`: identical units operate directly, a unitless side adopts
    /// the counterpart's unit, convertible units are normalised into the **left** operand's
    /// unit (so `1px + 1in` is `97px`), and anything else is a Sass error.
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
        let unitless = match (left_unit.as_str(), right_unit.as_str()) {
            ("", other) | (other, "") => Some(other.to_string()),
            _ => None,
        };
        if let Some(unit) = unitless {
            return Ok(Value::Number(arithmetic(*left, *right), unit));
        }
        let (left_family, left_factor) = measure(left_unit);
        let (right_family, right_factor) = measure(right_unit);
        if left_family == right_family {
            let normalised = right * right_factor / left_factor;
            return Ok(Value::Number(
                arithmetic(*left, normalised),
                left_unit.clone(),
            ));
        }
        Err(SassError::new(format!(
            "Incompatible units: '{right_unit}' and '{left_unit}'."
        )))
    }
}

/// `(family, factor to the family base)`; an unknown or relative unit is its own family, so
/// it can only ever combine with an identical unit.
fn measure(unit: &str) -> (&str, f64) {
    UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map_or((unit, 1.0), |(_, (family, factor))| (*family, *factor))
}

/// Sass number output: integers lose the fraction, everything else keeps ten digits.
fn format_number(value: f64) -> String {
    if value == 0.0 || (value.fract() == 0.0 && value.abs() < 1e15) {
        return format!("{}", value as i64);
    }
    format!("{value:.10}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
