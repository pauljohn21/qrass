//! Unit — a number's **rational (compound) unit** as two order-preserving multisets, which is what
//! makes `1px * 1em / 1rad / 1s` a value rather than a string: multiplication and division become
//! operations on the two sides, same-name cancellation is structural (never a later clean-up pass), and
//! the printed form follows from the data instead of from re-scanning text.
//!
//! ## Order is the corpus's, not alphabetical
//!
//! `core_functions/math/unit.hrx` prints **input order**: `math.unit(1px * 1em * 1rad)` → `"px*em*rad"`
//! and `math.unit(1px * 1em / 1rad / 1s)` → `"px*em/(rad*s)"`. Sorting the multisets would give
//! `"em*px*rad"` and fail both, so the representation keeps insertion order and only cancellation is
//! structural. (The plan recorded "sort by name at construction"; the fixtures refute that — see the
//! correction in `openspec/…/tasks.md` 1.4d.)
//!
//! ## The printed forms, each one quoted from a golden
//!
//! | unit | output | fixture |
//! |---|---|---|
//! | none | `""` | `unit::none` |
//! | `px` | `px` | `unit::one_numerator` |
//! | `px`, `em`, `rad` | `px*em*rad` | `unit::multiple_numerators` |
//! | denominator `px`, no numerator | `px^-1` | `unit::one_denominator` |
//! | denominators `px`,`em`,`rad`, no numerator | `(px*em*rad)^-1` | `unit::multiple_denominators` |
//! | `px` over `em` | `px/em` | `unit::numerator_and_denominator/single` |
//! | `px`,`em` over `rad`,`s` | `px*em/(rad*s)` | `unit::numerator_and_denominator/multiple` |
//!
//! A **repeated** unit (`px * px`) has no golden anywhere in the corpus — the only occurrences are
//! commented-out lines of `libsass-closed-issues/issue_783.hrx`, which the harness excludes — so this
//! model keeps duplicates as written (`px*px`) rather than inventing a power form for them. That is a
//! choice, and `unit.hrx`'s own README says the function is for debugging and "is not required to have
//! totally consistent output across different implementations".

use std::f64::consts::PI;

/// Convertible units as `(unit, family, factor to the family base)`. Sass converts only inside one
/// family, and only these ones: `em`/`rem`/`vh`/`vw`/`vmin`/`vmax`/`ch`/`ex` are relative to a context
/// Sass cannot see, so each stands in its own family and combines only with itself.
pub const UNITS: &[(&str, (&str, f64))] = &[
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

/// `(family, factor to the family base)`; an unknown or relative unit is its own family, so it can only
/// ever combine with an identical unit. The family borrows its name from whichever side supplied it — the
/// table's `'static` name or the caller's own — so no lifetime is pinned here.
pub fn measure(unit: &str) -> (&str, f64) {
    UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map_or_else(|| (unit, 1.0), |(_, (family, factor))| (*family, *factor))
}

/// One side of a rational unit: every unit name that appears there, in the order it appeared.
type Names = Vec<String>;

/// A unit as `numerator / denominator`, both cancelled against each other at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    numerator: Names,
    denominator: Names,
}

impl Unit {
    /// The unitless number.
    pub fn unitless() -> Self {
        Self {
            numerator: Vec::new(),
            denominator: Vec::new(),
        }
    }

    /// A lexeme's unit text: `""` is unitless, `px` one numerator, `%` a percentage. A unit read from
    /// source is never compound (a compound one only exists as the result of `*` or `/`), so the text is
    /// taken whole rather than re-split on `*` or `/`.
    pub fn named(name: &str) -> Self {
        Self {
            numerator: (!name.is_empty())
                .then(|| name.to_string())
                .into_iter()
                .collect(),
            denominator: Vec::new(),
        }
    }

    pub fn is_unitless(&self) -> bool {
        self.numerator.is_empty() && self.denominator.is_empty()
    }

    /// Whether a number carrying this unit may be printed **outside** a calculation: nothing at all, `%`,
    /// or one bare name. A compound unit (`px*em`, an inverted `1/em`) is calculation-only syntax, which
    /// is why `calc(1px / 2px)` collapses to `0.5` (`calc/operator/divide::simplified` wants `0.5px`)
    /// while `calc(1 / 1px / 1rad)` keeps its wrapper
    /// (`values/numbers/units/multiple::multiple_denominators`).
    pub fn is_plain(&self) -> bool {
        self.is_unitless() || self.single().is_some()
    }

    /// The one numerator unit, when that is all this is — the shape every conversion table and every
    /// per-unit message reads.
    pub fn single(&self) -> Option<&str> {
        match (self.numerator.as_slice(), self.denominator.as_slice()) {
            ([only], []) => Some(only),
            _ => None,
        }
    }

    /// A unit as its two sides, already cancelled. The cancellation is a multiset intersection: each name
    /// that appears on both sides disappears once from each, and the survivors keep their order.
    pub fn from_parts(numerator: Names, denominator: Names) -> Self {
        let (numerator, denominator) = cancel(numerator, denominator);
        Self {
            numerator,
            denominator,
        }
    }

    /// `a * b`: the sides combine, so `px/em * em` is `px` — the same structural cancellation the
    /// construction does, applied to two existing units.
    pub fn times(&self, rhs: &Unit) -> Unit {
        Unit::from_parts(
            self.numerator
                .iter()
                .chain(&rhs.numerator)
                .cloned()
                .collect(),
            self.denominator
                .iter()
                .chain(&rhs.denominator)
                .cloned()
                .collect(),
        )
    }

    /// `a / b`: the divisor's numerator goes to the denominator and its denominator comes back up, which
    /// is what makes `calc(1 / (1 / 1px))` reach `1px` rather than `1/1/px`.
    pub fn over(&self, rhs: &Unit) -> Unit {
        Unit::from_parts(
            self.numerator
                .iter()
                .chain(&rhs.denominator)
                .cloned()
                .collect(),
            self.denominator
                .iter()
                .chain(&rhs.numerator)
                .cloned()
                .collect(),
        )
    }

    /// The CSS form of the unit, per the table on this module. Unitless prints as nothing (a number
    /// followed by nothing), and a pure reciprocal takes the `^-1` form the corpus states.
    pub fn to_css(&self) -> String {
        let numerator = self.numerator.join("*");
        match (self.numerator.is_empty(), self.denominator.as_slice()) {
            (true, []) => String::new(),
            (true, [only]) => format!("{only}^-1"),
            (true, many) => format!("({})^-1", many.join("*")),
            (_, []) => numerator,
            (false, [only]) => format!("{numerator}/{only}"),
            (_, many) => format!("{numerator}/({})", many.join("*")),
        }
    }
}

/// Cancel the two sides against each other: every name the denominator also holds removes one copy of
/// itself from there and one from the numerator, so `px*em / em*rad` becomes `px / rad`. What is left of
/// each side keeps the order it was built in, which is what the corpus prints.
fn cancel(mut numerator: Names, mut denominator: Names) -> (Names, Names) {
    numerator.retain(
        |name| match denominator.iter().position(|held| held == name) {
            Some(at) => {
                denominator.remove(at);
                false
            }
            None => true,
        },
    );
    (numerator, denominator)
}
