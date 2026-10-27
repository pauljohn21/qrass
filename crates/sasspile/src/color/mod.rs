//! Color — a colour **created by a function**, as data: its channels, its alpha, and the
//! representation that decides how it prints.
//!
//! Three files, one law each, split because each was contradicted by the corpus on its own terms: this one
//! holds the value and its reading ([`Color::byte`], [`Color::hsl_channel`]), [`ops`] holds the axis
//! arithmetic that moves a colour through hsl, and [`text`] holds the printing law (whole bytes print as
//! numbers or hex, anything between prints as percentages, keyword-created colours print their keyword).
//!
//! ## Why a colour is not just channels
//!
//! The corpus shows the same channels printing three different ways, so the *form the colour was created
//! in* is part of the value, not a formatting choice made later:
//!
//! | created by | prints as | fixture |
//! |---|---|---|
//! | `rgb(18, 52, 86)` | `rgb(18, 52, 86)` | `core_functions/color/rgb/three_args/unitless::numbers` |
//! | `rgb(7.1%, 20.4%, 33.9%)` | `rgb(7.1%, 20.4%, 33.9%)` | `…/three_args/percents::all/percent` |
//! | `rgb(74.7%, 173, 93%)` | `rgb(74.7%, 67.8431372549%, 93%)` | `…/percents::unitless/green` |
//! | `rgb(#123, 1)` | `#112233` | `…/two_args::opaque_to/opaque` |
//! | `rgb(0, 0, 0, 0.5)` | `rgba(0, 0, 0, 0.5)` | `…/four_args/clamped::red` |
//!
//! A mixed percent/unitless call prints **all** channels as percentages — one percentage anywhere makes
//! the whole colour percentage-form — while a colour that came from hex prints as hex with its shorthand
//! expanded. Nothing here is inferred from how another implementation stores a colour: each row is the
//! fixture's own `output.css`.
//!
//! ## What a hex in a declaration value is *not*
//!
//! `a {b: #AbCd}` keeps its source spelling, capitalisation included
//! (`values/colors/alpha_hex/initial_letter` prints `four-digit: #AbCd`), so **parsing a declaration never
//! produces this type**. A `Color` exists only once a function has built one, which is also why the clamped
//! channel values below are canonical: they were numbers on their way in.
//!
//! ## Channels keep the scale they were written in
//!
//! `7.1%` must come back out as `7.1%`. Normalising every channel to 0–255 and dividing again would put
//! `7.0980392157%` on the page, so a channel stores its value **and** which scale it arrived in, and the
//! conversion happens at the moment a form needs the other scale.
//!
//! Channels are clamped on the way in — `256` → `255`, `-1` → `0`, `-0` → `0` (`four_args/clamped/*`) —
//! and out-of-range percentages likewise (`clamped/red` of `percents.hrx`: `100.1%` → `100%`).

use crate::value::Value;

mod ops;
mod text;

/// Alpha is clamped to `0–1`, with a `NaN` reading as transparent — the same reading `rgb`'s
/// `calc(NaN)` channel gets (`rgb/four_args/clamped`).
fn clamp_alpha(alpha: f64) -> f64 {
    if alpha.is_nan() {
        0.0
    } else {
        alpha.clamp(0.0, 1.0)
    }
}

/// One channel: the value as written, and whether it was written as a percentage.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Channel {
    value: f64,
    percent: bool,
}

impl Channel {
    /// `0–255` regardless of the scale it arrived in.
    fn byte(self) -> f64 {
        if self.percent {
            self.value * 255.0 / 100.0
        } else {
            self.value
        }
    }

    /// The percentage form, for a colour that has any percentage channel.
    fn percent_form(self) -> f64 {
        if self.percent {
            self.value
        } else {
            self.value * 100.0 / 255.0
        }
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
    fn clamped(self) -> Self {
        let ceiling = if self.percent { 100.0 } else { 255.0 };
        let bound = if self.value.is_nan() || self.value < 0.0 {
            0.0
        } else {
            self.value.min(ceiling)
        } + 0.0;
        match (self.percent, bound == self.value) {
            (true, false) => Channel {
                value: bound * 255.0 / 100.0,
                percent: false,
            },
            _ => Channel {
                value: bound,
                ..self
            },
        }
    }
}

/// The form a colour was created in, which decides how it prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    /// `rgb(18, 52, 86)` — the legacy comma form, channels as numbers
    Legacy,
    /// `rgb(7.1%, 20.4%, 33.9%)` — the same form, but a percentage was written
    Percent,
    /// `#112233` — created from a hex literal, so it prints back as hex
    Hex,
    /// `red` — created from a CSS keyword. Such a colour prints its keyword again while its bytes still name
    /// one (`lighten(red, 0%)` → `red`, `complement(red)` → `aqua`) and falls back to hex otherwise
    /// (`complement(turquoise)` → `#e04050`), whereas a colour built from `rgb(0, 0, 0)` keeps printing
    /// `rgb(0, 0, 0)` — see [`super::text`] for the fixtures that separate the two.
    Named,
    /// `hsl(60, 100%, 50%)` — hue as a plain number of degrees, saturation and lightness as percentages
    /// (`core_functions/color/hsl/three_args/units.hrx`)
    Hsl,
}

/// A colour value.
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    channels: [Channel; 3],
    alpha: f64,
    representation: Representation,
}

impl Color {
    /// The three channels as they were written, already clamped, plus an alpha clamped to `0–1`.
    pub fn new(red: &Value, green: &Value, blue: &Value, alpha: f64) -> Option<Self> {
        let channel = |value: &Value| match value {
            Value::Number(number, unit) if unit.is_unitless() => Some(Channel {
                value: *number,
                percent: false,
            }),
            Value::Number(number, unit) if unit.single() == Some("%") => Some(Channel {
                value: *number,
                percent: true,
            }),
            _ => None,
        };
        let [red, green, blue] =
            [channel(red)?, channel(green)?, channel(blue)?].map(Channel::clamped);
        Some(Self {
            channels: [red, green, blue],
            alpha: clamp_alpha(alpha),
            // Decided **after** clamping, because a clamped percentage is no longer a percentage
            // ([`Channel::clamped`]): `rgb(100.1% 0 0)` has no percent channel left and so prints
            // `rgb(255, 0, 0)` rather than the all-percent form.
            representation: match (red.percent, green.percent, blue.percent) {
                (false, false, false) => Representation::Legacy,
                _ => Representation::Percent,
            },
        })
    }

    /// A colour written as hex: three, four, six or eight digits, any case. Anything else is `None`, so
    /// the caller keeps whatever verbatim behaviour it had.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        let hex = |c: char| c.to_digit(16);
        let value = |from: usize, width: usize| {
            (0..width)
                .map(|index| hex(digits.chars().nth(from + index)?))
                .try_fold(0u32, |acc, nibble| Some(acc * 16 + nibble?))
        };
        match digits.len() {
            3 | 4 => {
                let byte = |index: usize| value(index, 1).map(|nibble| nibble * 17);
                let red = byte(0)?;
                let green = byte(1)?;
                let blue = byte(2)?;
                let alpha = match digits.len() {
                    4 => f64::from(byte(3)?) / 255.0,
                    _ => 1.0,
                };
                Self::rgb(f64::from(red), f64::from(green), f64::from(blue), alpha)
            }
            6 | 8 => {
                let red = value(0, 2)?;
                let green = value(2, 2)?;
                let blue = value(4, 2)?;
                let alpha = match digits.len() {
                    8 => f64::from(value(6, 2)?) / 255.0,
                    _ => 1.0,
                };
                Self::rgb(f64::from(red), f64::from(green), f64::from(blue), alpha)
            }
            _ => None,
        }
        .map(|color| Self {
            representation: Representation::Hex,
            ..color
        })
    }

    /// A colour named in CSS (`color.alpha(red)` → `1`, `core_functions/color/alpha.hrx::color/named`).
    ///
    /// The keywords resolve through the `color` crate (linebender; actively maintained, targets CSS Color
    /// 4) rather than a hand-typed table, because the local corpus has no complete name→RGB source —
    /// `values/colors/` holds `alpha_hex/` and `equality.hrx` only (which states a handful, e.g.
    /// `purple == hsl(300, 100%, 25.098039215686%)`), and an audited dependency beats my recollection.
    /// **Only the keyword→colour mapping is taken**; how a colour prints stays this project's own law
    /// (`Representation`, `to_css`).
    ///
    /// The one thing the dependency's table lacks is CSS's **`-grey` spellings** — its keyword list is
    /// `148 − 7 aliases + transparent = 142` — and the corpus does use them (`invert(grey)` and
    /// `color.adjust(grey, …)` are real rows). Those need no RGB data at all: CSS defines each `-grey`
    /// form as another spelling of the same entry, so the alias is folded here and the value still comes
    /// from the dependency.
    pub fn from_name(name: &str) -> Option<Self> {
        let keyword = name.trim().to_lowercase().replace("grey", "gray");
        let rgba = color::parse_color(&keyword)
            .ok()?
            .to_alpha_color::<color::Srgb>()
            .to_rgba8();
        Self::rgb(
            f64::from(u16::from(rgba.r)),
            f64::from(u16::from(rgba.g)),
            f64::from(u16::from(rgba.b)),
            f64::from(u16::from(rgba.a)) / 255.0,
        )
        .map(|color| Self {
            representation: Representation::Named,
            ..color
        })
    }

    /// A colour written as text: hex or keyword. Used where an argument arrives as unquoted text, which
    /// is how a declaration keeps `red` and `#123` — `rgb(...)`/`hsl(...)` written as a call already
    /// evaluated into [`Value::Color`] before it gets here.
    ///
    /// Text carrying parentheses is refused: the parser behind [`Self::from_name`] also reads
    /// `color(display-p3 1 0 0)`, `lab(…)` and `lch(…)`, and answering those with sRGB bytes would be a
    /// **wrong value** for a colour space this compiler has not implemented. Declining keeps the call as
    /// written, which is what `core_functions/color/mix/predefined.hrx` and the `channel/*` rows of other
    /// spaces ask for.
    pub fn of(value: &Value) -> Option<Color> {
        match value {
            Value::Color(color) => Some(color.clone()),
            Value::Raw(text) => {
                let text = text.trim();
                if text.contains('(') {
                    return None;
                }
                if text.starts_with('#') {
                    Self::from_hex(text)
                } else {
                    Self::from_name(text)
                }
            }
            _ => None,
        }
    }

    /// One `0–255` channel, for the accessors. A colour held in **another space** returns `None` rather
    /// than a wrong number.
    ///
    /// Nothing in the corpus contradicts that caution, and the measurement says it is not a gap I should
    /// fill by inference: `grep`ing every `.hrx` under `spec/` for `color.(red|green|blue)(…hsl(…))` finds
    /// **no row at all** — the hsl-space fixtures read the hsl axes (`channel/hsl.hrx`), which
    /// [`Self::hsl_channel`] now serves. `hsl` → `rgb` therefore has no golden to be written against, while
    /// the reverse direction does and is implemented.
    pub fn byte(&self, index: usize) -> Option<f64> {
        match self.representation {
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => self
                .channels
                .get(index)
                .map(|channel| channel.byte().round()),
            Representation::Hsl => None,
        }
    }

    /// The colour's own alpha: `color.alpha(rgba(red, 0.42))` → `0.42` (`alpha.hrx::color/middle`).
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// The hue in degrees: `359deg` from `hsl(359, 50%, 50%)`, `180deg` from `hsl(540, …)` because the hue
    /// was folded into `0–360` at creation (`color/hue.hrx`), and `210deg` from `#abcdef` — read off the
    /// `rgb` channels through [`Self::axes_of`] (`modules/color/hue.hrx`).
    pub fn hue(&self) -> Option<f64> {
        self.hsl_channel(0)
    }

    /// The saturation, for `color.saturation`: `100%`, `0%`, `50%`, `0.5%`, `42%` (`color/saturation.hrx`)
    /// and `68%` for `#abcdef` (`modules/color/saturation.hrx`).
    pub fn saturation(&self) -> Option<f64> {
        self.hsl_channel(1)
    }

    /// The lightness, for `color.lightness`: the same five readings in the L axis (`color/lightness.hrx`)
    /// and `80.3921568627%` for `#abcdef` (`modules/color/lightness.hrx`).
    pub fn lightness(&self) -> Option<f64> {
        self.hsl_channel(2)
    }

    /// One axis of the colour, wherever it is stored: the channels themselves for an `hsl` colour, the
    /// [`crate::axes`] round trip for a colour held in another space.
    ///
    /// The converted reading is what the corpus asks for, row by row: `color.lightness(#abcdef)` →
    /// `80.3921568627%` is `(239 + 171)/2` of 255 and `color.saturation(#abcdef)` → `68%` is
    /// `68/(510-410)`, both from `modules/color/*.hrx`. These three rows are why the conversion is safe to
    /// read here; an `hsl` colour's own axes are already stored, so no round trip is taken for them.
    fn hsl_channel(&self, index: usize) -> Option<f64> {
        self.axes_of().get(index).copied()
    }

    /// A colour from `0–255` byte values, for hex parsing.
    fn rgb(red: f64, green: f64, blue: f64, alpha: f64) -> Option<Self> {
        let plain = |value: f64| Value::Number(value, crate::unit::Unit::unitless());
        Self::new(&plain(red), &plain(green), &plain(blue), alpha)
    }

    /// An `hsl`/`hsla` colour. The three laws the fixtures state, none of them shared with `rgb`:
    /// * the **hue is folded to degrees and wrapped into `0–360`** — `hsl(60rad, 100%, 50%)` is
    ///   `hsl(197.7467707849, 100%, 50%)` and `0deg` prints as `0`
    ///   (`hsl/three_args/units.hrx`);
    /// * saturation and lightness **always** carry a `%` (`hsl(0, 50, 50%)` → `hsl(0, 50%, 50%)`), and
    ///   neither is clamped at its upper end: `hsl(0, 500%, 50%)` and `hsl(0, 100%, 500%)` stay
    ///   (`hsl/three_args/out_of_gamut.hrx`, `hsl/one_arg/no_alpha.hrx::{out_of_gamut/saturation/above,
    ///   clamped/lightness/above}`);
    /// * a **negative saturation** however goes to `0` — `hsl(0 -100% 50%)` is `hsl(0, 0%, 50%)`
    ///   (`hsl/one_arg/no_alpha.hrx::clamped/saturation/below`) — while a negative lightness is left alone
    ///   (`…::clamped/lightness/below` keeps `-100%`). No legacy comma value row exercises either, so the
    ///   asymmetry is stated only by the one-argument spelling;
    /// * the alpha is clamped to `0–1` and a `1` alpha returns the three-argument spelling
    ///   (`hsl/four_args/alpha.hrx`).
    pub fn hsl(hue: &Value, saturation: &Value, lightness: &Value, alpha: f64) -> Option<Self> {
        let number = |value: &Value| match value {
            Value::Number(number, _) => Some(*number),
            _ => None,
        };
        let degrees = |value: &Value| {
            let number = number(value)?;
            let factor = match value {
                Value::Number(_, unit) => unit
                    .single()
                    .filter(|name| crate::unit::measure(name).0 == "angle")
                    .map_or(1.0, |name| crate::unit::measure(name).1),
                _ => 1.0,
            };
            Some((number * factor).rem_euclid(360.0))
        };
        Some(Self {
            channels: [
                Channel {
                    value: degrees(hue)?,
                    percent: false,
                },
                Channel {
                    value: number(saturation)?.max(0.0),
                    percent: true,
                },
                Channel {
                    value: number(lightness)?,
                    percent: true,
                },
            ],
            alpha: clamp_alpha(alpha),
            representation: Representation::Hsl,
        })
    }

    /// The same colour under a different alpha, keeping how it was made: `rgb(#123, 0.5)` is the hex
    /// channels with the new alpha, not a different colour (`two_args::opaque_to/partial`).
    pub fn with_alpha(&self, alpha: f64) -> Self {
        Self {
            alpha: clamp_alpha(alpha),
            ..self.clone()
        }
    }
}
