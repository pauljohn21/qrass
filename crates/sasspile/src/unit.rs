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

/// The table's entry for a unit — `(family, factor to the family base)` — or `None` when the unit is not in
/// it. `em`/`rem`/`vh`/`ch`… are absent: each stands for its own context-relative family, so
/// `math.compatible(1px, 2em)` is `false` (`core_functions/math/comparable.hrx::unit/to_different`) while
/// `1em` vs `2em` compares as written. Comparison helpers ask this first so an unlisted unit declines instead
/// of claiming a clash (`math.max.hrx` evidences `px` vs `s`; nothing evidences `px` vs `c`).
///
/// This is the only table of convertible units in the project, and `math.div`, `math.compatible`, the colour
/// hues and `math.hypot` all read it through here — `math.rs` carried its own list of length literals until
/// `hypot.hrx::units` needed `q` and the exact quarter-millimetre relation.
pub fn conversion(unit: &str) -> Option<(&'static str, f64)> {
    UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, entry)| *entry)
}

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

    /// Both sides as written, for a function that must judge the whole fraction rather than one name
    /// (`builtins::dimension` compares two units by their **dimension signature**).
    pub fn names(&self) -> (&[String], &[String]) {
        (&self.numerator, &self.denominator)
    }

    /// Cancel a **convertible pair across the fraction** and carry its factor into the magnitude: `px` on
    /// top with `in` below is not two surviving names but `96` divided out, because the corpus cancels
    /// `calc(96px * 1rad / 1ms / 1Hz) / 1in` down to `calc(1rad / 1ms / 1Hz)`
    /// (`values/numbers/units/multiple::division/cancels/compatible`). Names of the same family pair off
    /// first-to-first in the order written; what cannot be matched stays.
    pub fn cancel_convertible(&self, value: f64) -> (f64, Unit) {
        let mut denominator = self.denominator.clone();
        let mut magnitude = value;
        let mut numerator = self.numerator.clone();
        // Each numerator name pairs off with the first denominator of the same family that is not already
        // the same unit (`Unit::from_parts` cancels identical names before this runs); the pair leaves the
        // fraction and `top / bottom` in base factors enters the magnitude.
        numerator.retain(|top| {
            let (family, top_factor) = measure(top);
            let pair = denominator
                .iter()
                .position(|below| measure(below).0 == family && measure(below).1 != top_factor);
            match pair {
                Some(index) => {
                    magnitude *= top_factor / measure(&denominator[index]).1;
                    denominator.remove(index);
                    false
                }
                None => true,
            }
        });
        (magnitude, Unit::from_parts(numerator, denominator))
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

    /// A number **inside a calculation**: the magnitude is written once, then every remaining unit with
    /// its own `1`, because a compound unit has no plain-CSS spelling and the fixtures show the product
    /// form (`values/numbers/units/multiple.hrx`: `calc(1px * 1rad)`, `calc(1 / 1px / 1rad)`,
    /// `calc(1px * 1rad / 1ms / 1Hz)`).
    ///
    /// A magnitude that is **not finite** has no plain-CSS spelling either, so it takes [`calc_product`] too:
    /// gluing `infinity` onto a unit prints `infinitypx`, which is not a CSS number, where the corpus writes
    /// `calc(1% + infinity * 1px)` (`values/calculation/calc/constant.hrx::precedence/after_plus`) and
    /// `calc(NaN * 1deg)` (`core_functions/math/asin.hrx::greater_than_one`).
    ///
    /// Said plainly so the next reader does not over-read this: the **colour** printers do not come through
    /// here. `Color::legacy`/`Channel`/`space::print` write `format_number(…) + "%"` directly, so the same
    /// bare `infinity%` is still printed there — measured as `core_functions/color/to_space/hwb/hsl::white`,
    /// which got `hsl(0, infinity%, 100%)` against the archive's `hsl(0, 0%, 100%)`. One law, one home, is
    /// still open work on that side.
    pub fn calc_form(&self, value: f64) -> String {
        let magnitude = crate::value::format_number(value);
        if !value.is_finite() {
            return self.calc_product(&magnitude);
        }
        let with = |prefix: &str, unit: &str| format!("{prefix}{unit}");
        let head = match self.numerator.split_first() {
            Some((first, rest)) => std::iter::once(with(&magnitude, first))
                .chain(rest.iter().map(|unit| with("1", unit)))
                .collect::<Vec<String>>()
                .join(" * "),
            // A pure reciprocal (or a unitless number): the magnitude stands on its own in front.
            None => magnitude,
        };
        head + &self
            .denominator
            .iter()
            .map(|unit| format!(" / 1{unit}"))
            .collect::<String>()
    }

    /// The same shape for a magnitude that has **no plain-CSS spelling at all** — `NaN` or an infinity — where
    /// even a single unit has to carry its own `1`: `calc(NaN * 1deg)`
    /// (`core_functions/math/asin.hrx::greater_than_one`), `calc(-infinity / 1px)` and
    /// `calc(infinity * 1px * 1em)` (`values/numbers/units/multiple.hrx`). [`Unit::calc_form`] cannot say
    /// this: it writes the first numerator unit as `<magnitude>px`, which reads as `NaNdeg`.
    pub fn calc_product(&self, magnitude: &str) -> String {
        let (numerator, denominator) = self.names();
        magnitude.to_string()
            + &numerator
                .iter()
                .map(|unit| format!(" * 1{unit}"))
                .collect::<String>()
            + &denominator
                .iter()
                .map(|unit| format!(" / 1{unit}"))
                .collect::<String>()
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
