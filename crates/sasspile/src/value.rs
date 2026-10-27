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
use crate::calc::Content;
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
    /// A **calculation** whose content stays as the reduced form it was read into: `name` is the callee as
    /// written and `content` the summary from [`crate::calc`] — its printed text, its folded value when
    /// every step computed, and its precedence. Storing the summary rather than a string is what lets a
    /// bound calculation take part in arithmetic again
    /// (`values/numbers/units/multiple::division/cancels/numerator/once`).
    Calculation(String, Content),
    /// A colour **created by a function** (`rgb(18, 52, 86)` → itself, `rgb(#123, 1)` → `#112233`). A hex
    /// written straight into a declaration stays `Raw`, because the corpus keeps its source spelling
    /// (`values/colors/alpha_hex/initial_letter` prints `#AbCd`, capitals and all) — see [`crate::color`].
    Color(crate::color::Color),
    /// A **space-separated list**, the value form of `Expr::Space`: `rgb(18 52 86)` is one argument holding
    /// three numbers, not three arguments (`core_functions/color/rgb/one_arg/no_alpha.hrx`, and the same
    /// file's `$channels:` row shows a label binds this whole value). Components are owned values so a
    /// consumer reads them structurally instead of re-splitting text.
    Space(Vec<Value>),
    /// A **slash-separated list**, the value form of `Expr::Slash`: CSS Color 4's `rgb(18 52 86 / 0.5)` is
    /// the same call as `rgb(list.slash(0 255 127, 0))` for `rgb(0 255 127 / 0)` — the corpus pairs those two
    /// rows in `rgb/one_arg/alpha.hrx::{slash_list,in_gamut/transparent}` with one and the same output — so
    /// the alpha is a slash list's second element, not a division of the last channel.
    Slash(Vec<Value>),
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
            Value::Calculation(name, content) => format!("{name}({})", content.printed),
            Value::Color(color) => color.to_css(),
            // Components keep their own spelling, separated by one space each — `rgb(18 52 86)` read as a
            // list prints back as a list when nothing consumes it.
            Value::Space(items) => items
                .iter()
                .map(Value::to_css)
                .collect::<Vec<String>>()
                .join(" "),
            // One space either side of the slash, which is how the corpus prints an unconsumed slash list
            // (`values/lists/*`; `rgb(0 255 127 / none)` stays exactly that).
            Value::Slash(items) => items
                .iter()
                .map(Value::to_css)
                .collect::<Vec<String>>()
                .join(" / "),
        }
    }

    /// Whether this value can be printed **outside** a calculation. Only a number with a compound unit
    /// fails (`Unit::is_plain`) — the law this project read off
    /// `values/numbers/units/multiple::multiple_denominators` — so such a result never becomes a
    /// declaration value; see [`crate::exec::State::evaluated`].
    pub fn is_plain(&self) -> bool {
        match self {
            Value::Number(_, unit) => unit.is_plain(),
            // A list is only printable outside a calculation if every element is.
            Value::Space(items) | Value::Slash(items) => items.iter().all(Value::is_plain),
            _ => true,
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

    /// The number this value carries when it **is** one — including a calculation whose content folded all
    /// the way down: `math.div(calc(2px * 3px), 4px)` is `1.5px`
    /// (`values/calculation/calc/operator/units::multiplication`), so the folded value has to be reachable
    /// through the shell instead of only through its text.
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

    /// `a * b`: the magnitudes multiply and the units combine, so `px * em` is `px*em` while
    /// `px/em * em` is `px` — the cancellation is structural, in [`Unit::times`]. A convertible pair across
    /// the fraction cancels here too and carries its factor along: `calc(1px * 1rad / 1ms / 1Hz) * 1s` is
    /// `calc(1000px * 1rad / 1Hz)`, not `1px * 1rad * 1s / 1ms / 1Hz`
    /// (`values/numbers/units/multiple::multiplication/cancels/compatible`).
    pub fn times(&self, rhs: &Value) -> Result<Value, SassError> {
        self.cross(rhs, |left, right| left * right, Unit::times)
    }

    /// `a / b`: the magnitudes divide and the divisor's units flip sides, so `6 / 3px` is `2/px`.
    ///
    /// A zero divisor is **not** a special case here: f64 division yields infinity and
    /// `format_number` already spells it the way Sass does (`math.div(1, 0)` → `infinity`,
    /// `values/calculation/calc/constant::infinity`), so no panic and no invented `NaN`.
    pub fn divided_by(&self, rhs: &Value) -> Result<Value, SassError> {
        self.cross(rhs, |left, right| left / right, Unit::over)
    }

    /// `*` and `/` share this: build the raw result, then cancel every **convertible pair across the
    /// fraction** and fold its factor into the magnitude — `96px / 1in` is `1`
    /// (`values/numbers/units/multiple::division/cancels/compatible`) and `1s / 1ms` is `1000`
    /// (`…::multiplication/cancels/compatible`). Both operators need it, so it lives in one place rather
    /// than being remembered per call site.
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

    /// The shape `*` and `/` share: both sides must be numbers, the magnitudes go through one operation
    /// and the units through one combination. Unlike `+`/`-` there is no conversion or adoption step —
    /// `1px * 1s` is a compound unit, not an error.
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
pub(crate) fn format_number(value: f64) -> String {
    if value.is_infinite() {
        // Sass spells the infinities `infinity` / `-infinity`; Rust's `Display` gives `inf`, which CSS
        // would not read (`values/calculation/calc/constant::infinity/math/unsimplified/from_variable`).
        return if value > 0.0 {
            "infinity".to_string()
        } else {
            "-infinity".to_string()
        };
    }
    if value == 0.0 {
        // An **exact** negative zero keeps its sign, because the corpus prints one: `math.sin(-0.0)` → `-0`
        // (`core_functions/math/sin.hrx::negative_zero`) and `math.atan(-0.0)` → `-0deg`
        // (`…/atan.hrx::negative_zero`). A value that merely *cuts* to zero — the `-1.1e-15` a colour channel
        // gets back from `color.change(red, $lightness: 0.5%)`, whose row wants `rgb(1%, 0%, 0%)` — is
        // collapsed by the branch below, so the two laws do not fight.
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
    // A number whose significant digits already fit in ten is printed as it stands: that is what keeps
    // `0.000000000000001` (one significant digit) and a `f64::MAX` literal intact.
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
    // Past that, the cut is **ten digits after the decimal point**, not ten significant ones. A colour's
    // percentage conversion shows the difference: `rgb(74.7%, 173, 93%)` prints
    // `rgb(74.7%, 67.8431372549%, 93%)` — twelve significant digits, because `173 × 100 / 255` has fourteen
    // decimals and ten of them survive (`core_functions/color/rgb/three_args/percents::unitless/green`).
    // It agrees with the arithmetic rows already pinned here: `2 * 3 / 5 * 7 / 11` → `0.7636363636` and
    // `1 / (2 * 3)` → `0.1666666667` (`operator/precedence::simplified/*`), and a computed `0.246 × 100`
    // trims back to `24.6` (`core_functions/math/percentage.hrx::small`).
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
    // A cut that lands on `-0` prints as plain zero: `color.change(red, $lightness: 0.5%)` leaves a channel
    // of `-1.1e-15` — float dust for zero — and the corpus prints `rgb(1%, 0%, 0%)`, not `-0%`
    // (`change/hsl.hrx::lightness/fraction`). It is the printing half of the rule `Channel::clamped` applies
    // to a written `-0` (`rgb/three_args::clamped/red/minus_zero`).
    match cut.as_str() {
        "-0" => "0".to_string(),
        _ => cut,
    }
}
