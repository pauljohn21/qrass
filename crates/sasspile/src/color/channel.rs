//! One **legacy channel**: the number as it was written, the scale it was written in, and the laws that
//! belong to neither of those two facts (clamping, and how the arithmetic reads an unwritten channel).
//!
//! Split out of the parent because the colour-level constructors and the printing dispatch had grown past
//! this repository's line budget, and the seam the code already drew is this one: [`Color`](super::Color)
//! decides *what a colour is*; a channel here decides *how one written number behaves*. Nothing in this file
//! knows about colour spaces, representations, or printing.

use crate::value::Value;

/// Whether a value **is the word `none`** — the one spelling of that question in the crate. It is asked in
/// three places (`Color::new` reading a written channel, the `color(…)` constructor's channel reader, and the
/// `change` family reading an argument), and they used to disagree on case: two compared
/// `eq_ignore_ascii_case`, one compared `== "none"`, so `rgb(NONE 0 0)` was a missing channel while
/// `color.change(x, $red: NONE)` was not. **No corpus row exercises any case variant**
/// (`grep -rn "\bNONE\b" sass-spec/spec/core_functions/color/` is empty), so the case rule is a stated choice —
/// case-insensitive, matching how space names are already read — not a law the archive prints.
/// `change/hwb.hrx::whiteness/none` is the row that establishes the unquoted word itself.
pub(crate) fn is_missing_word(value: &Value) -> bool {
    matches!(value, Value::Raw(text) if text.eq_ignore_ascii_case("none"))
}

/// How the maths reads a channel that was not written: as `0`. The one place that law is stated — both the
/// byte-scale reads below and [`super::space::InSpace::filled`] go through it, because a channel that is
/// absent is one fact and two spellings of it have already drifted apart once in this repository.
///
/// What the *answer* prints is a different question, decided by the index-aligned rule at the bottom of
/// [`Color::converted`](super::Color::converted) and by [`super::space::print`]; nothing here guesses it.
pub(super) fn missing_as_zero(value: Option<f64>) -> f64 {
    value.unwrap_or(0.0)
}

/// One channel: the value as written, and whether it was written as a percentage. `None` is a `none` — a
/// channel the call left unwritten, which the corpus **keeps unwritten** through an identity conversion
/// (`to_space/rgb/rgb.hrx::missing/red` answers `color.to-space(rgb(none 20 30), rgb)` with
/// `rgb(none 20 30)`) rather than substituting `0`, so absence is data and not a sentinel number. It lives in
/// the channel itself because that is the whole fact: a separate mask beside the triple would be a second
/// place to say the same thing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Channel {
    pub(super) value: Option<f64>,
    pub(super) percent: bool,
}

impl Channel {
    /// `0–255` regardless of the scale it arrived in.
    pub(super) fn byte(self) -> f64 {
        let value = missing_as_zero(self.value);
        if self.percent {
            value * 255.0 / 100.0
        } else {
            value
        }
    }

    /// The percentage form, for a colour that has any percentage channel.
    pub(super) fn percent_form(self) -> f64 {
        let value = missing_as_zero(self.value);
        if self.percent {
            value
        } else {
            value * 100.0 / 255.0
        }
    }

    /// Whether this channel **prints** as a whole byte, which is what decides hex/numbers against percentages.
    ///
    /// The tolerance is not arbitrary: it is half of the last place `format_number` can print (ten digits
    /// after the point, so `5e-11`). Without it, float dust decides the form — `color.change(#993333,
    /// $blackness: 80%)` implies a red channel of `51.000000000000014`, and the corpus prints `#333333`
    /// rather than `rgb(20%, 20%, 20%)` (`change/hwb.hrx::blackness/high`). Same reason the gamut test in
    /// [`super::text`] rounds before comparing: **rounding belongs to printing**, both directions.
    pub(super) fn whole(self) -> bool {
        (self.byte() - self.byte().round()).abs() < 5e-11
    }

    /// Clamped into its own scale: percentages to `0–100`, plain numbers to `0–255`. A negative zero
    /// becomes zero, which is what `four_args/clamped::red/minus_zero` shows (`rgb(-0, …)` → `0`).
    ///
    /// A **clamped percentage loses its percentage spelling** and comes back as the byte it clamped to —
    /// `rgb(100.1% 0 0)` → `rgb(255, 0, 0)` and `rgb(0 -0.1% 0)` → `rgb(0, 0, 0)`, while `rgb(0% 100% 50%)`
    /// keeps every `%` it was given (`rgb/one_arg/no_alpha.hrx::{percents/clamped/*,percents/boundaries}`).
    /// Only the modern one-argument rows state this law; no legacy comma fixture has an out-of-range
    /// percentage, which is why it was not visible earlier. A channel at exactly its ceiling is not clamped,
    /// so `100%` above stays `100%`.
    pub(super) fn clamped(self) -> Self {
        // A channel that was never written has no value to clamp, and clamping it to `0` would turn the
        // corpus's `rgb(none 20 30)` into `rgb(0 20 30)` — the two are different colours as far as the archive
        // is concerned (`to_space/rgb/rgb.hrx::missing/red`).
        let Some(value) = self.value else {
            return self;
        };
        let ceiling = if self.percent { 100.0 } else { 255.0 };
        let bound = if value.is_nan() || value < 0.0 {
            0.0
        } else {
            value.min(ceiling)
        } + 0.0;
        match (self.percent, bound == value) {
            (true, false) => Channel {
                value: Some(bound * 255.0 / 100.0),
                percent: false,
            },
            _ => Channel {
                value: Some(bound),
                ..self
            },
        }
    }
}
