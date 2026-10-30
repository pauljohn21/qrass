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

mod matrix;
mod ops;
mod read;
mod space;
mod text;

pub use space::{InSpace, Space};

/// Alpha is clamped to `0–1`, with a `NaN` reading as transparent — the same reading `rgb`'s
/// `calc(NaN)` channel gets (`rgb/four_args/clamped`).
fn clamp_alpha(alpha: f64) -> f64 {
    // A zero loses its sign here, because the alpha is **clamped** rather than carried:
    // `rgb(0, 0, 0, -0)` → `rgba(0, 0, 0, 0)` and `hwb(…, -0)` → `hsla(…, 0)`
    // (`core_functions/color/rgb/four_args/clamped.hrx::alpha/negative_zero`,
    // `…/hwb/four_args.hrx::alpha/negative_zero`). A transcendental that receives a `-0.0` keeps it
    // (`math.sin(-0.0)` → `-0`) — that law is in the printer, not here.
    if alpha.is_nan() || alpha == 0.0 {
        0.0
    } else {
        alpha.clamp(0.0, 1.0)
    }
}

/// The magnitude of a numeric argument, whatever its unit — the axis laws above decide what to do with the
/// unit, and `hsl`/`hwb` differ here: saturation, lightness, whiteness and blackness take the number as
/// written, while the hue converts an angle ([`angle`]).
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, _) => Some(*number),
        _ => None,
    }
}

/// A hue as degrees, folded into `0–360`. An **angle unit** converts first (`60rad` → `197.7467707849`,
/// `hsl/three_args/units::hue/turn`-family rows); a unit that is not an angle converts nothing, so
/// `hwb(1in, 30%, 40%)` is `hsl(1, …)` (`hwb/three_args/units::hue/non_angle`) rather than an error this
/// slice has no message for.
///
/// The fold also **loses a zero's sign**: `color.hwb(-0, 30%, 40%, 0.5)` →
/// `hsla(0, 33.3333333333%, 45%, 0.5)` and `hsl(-1 * 0, 100%, 50%)` → `hsl(0, 100%, 50%)`
/// (`core_functions/color/hwb/four_args.hrx::hue/negative_zero`,
/// `…/hsl/four_args/in_gamut.hrx::negative_zero/hue`) — `f64::rem_euclid` keeps `-0.0`, so this path does not.
pub fn angle_of(value: &Value) -> Option<f64> {
    let factor = match value {
        Value::Number(_, unit) => unit
            .single()
            .filter(|name| crate::unit::measure(name).0 == "angle")
            .map_or(1.0, |name| crate::unit::measure(name).1),
        _ => 1.0,
    };
    let folded = (number(value)? * factor).rem_euclid(360.0);
    Some(if folded == 0.0 { 0.0 } else { folded })
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

    /// Whether this channel **prints** as a whole byte, which is what decides hex/numbers against percentages.
    ///
    /// The tolerance is not arbitrary: it is half of the last place `format_number` can print (ten digits
    /// after the point, so `5e-11`). Without it, float dust decides the form — `color.change(#993333,
    /// $blackness: 80%)` implies a red channel of `51.000000000000014`, and the corpus prints `#333333`
    /// rather than `rgb(20%, 20%, 20%)` (`change/hwb.hrx::blackness/high`). Same reason the gamut test in
    /// [`super::text`] rounds before comparing: **rounding belongs to printing**, both directions.
    fn whole(self) -> bool {
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
///
/// `Eq` is dropped for the [`Representation::InSpace`] variant: a colour in its own space carries `f64`
/// channels, and pretending they form a total equivalence would be a second claim about floats that nothing
/// in the corpus makes. `PartialEq` is all any caller here has ever asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// `hwb(0, 30%, 40%)` — the channels are hue, whiteness and blackness, while the **printing** is the
    /// hsl form the corpus gives: `hwb(0deg, 30%, 40%)` → `hsl(0, 33.3333333333%, 45%)`
    /// (`core_functions/color/hwb/three_args/units.hrx::hue/deg`). The whiteness and blackness are still
    /// what the colour *is*, which is why `color.channel(…, "whiteness", $space: hwb)` can read them back
    /// (`channel/hwb.hrx`) and why a plain hsl colour cannot.
    Hwb,
    /// The colour **is** its space channels: built by `color(…)` or moved by `color.to-space`. This is a
    /// variant of the representation rather than a second field beside it, because one fact must have one
    /// judge. With `space: Option<InSpace>` sitting next to `representation`, every read that looked only at
    /// the representation saw `Legacy` and answered a `display-p3` colour off the empty byte triple — the
    /// measurement named 15 `non_legacy` rows printed as `0%`, `1` or `rgb(85, 85, 85)`, and the corpus
    /// answers them with an error instead (`hue.hrx::error/non_legacy`: "color.hue() is only supported for
    /// legacy colors"). Now a consumer that matches on the representation must say what a space colour means
    /// to it, or the build fails.
    InSpace(InSpace),
}

/// A colour value.
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    channels: [Channel; 3],
    alpha: f64,
    representation: Representation,
    /// The hsl axes **as written by the call that produced this colour**, when it came from
    /// [`Color::with_axes`]. An out-of-gamut colour prints its axes, and they must be the ones the caller
    /// named: `color.change(red, $lightness: 120%)` → `hsl(0, 100%, 120%)` (`change/hsl.hrx::lightness/above_range`),
    /// while deriving them back from the implied bytes gives `hsl(180, -100%, 120%)` because a negative
    /// chroma rotates the hue. `None` everywhere else, so nothing about a constructed or adjusted colour
    /// changes.
    authored: Option<[f64; 3]>,
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
            // Constructed from written channels, so there is no separate authored triple.
            authored: None,
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
            authored: None,
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
            authored: None,
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

    /// An `hwb`/`hwb`-with-alpha colour, from the global `hwb`/`color.hwb` names or the `color()` syntax.
    ///
    /// The channels are stored as the call gave them — `[hue, whiteness, blackness]` — because that is what
    /// `color.channel(…, $space: hwb)` reads back (`channel/hwb.hrx`). Two laws the fixtures state, both
    /// different from `rgb`'s:
    /// * the hue follows the `hsl` law: an angle unit folds to degrees (`hwb(1rad, 30%, 40%)` →
    ///   `hsl(57.2957795131, …)`) and a non-angle is taken as a plain number (`hwb(1in, …)` → `hsl(1, …)`,
    ///   `hwb/three_args/units::hue/non_angle`);
    /// * whiteness and blackness are **not clamped to `0–100`**, but a pair that overflows the colour is
    ///   **normalised at creation**: `hwb(10deg 70% 50%)` reports its whiteness as `58.3333333333%` and its
    ///   blackness as `41.6666666667%` (`channel/hwb.hrx::local/normalized/*` — `70/120` and `50/120`), so
    ///   what the colour *is* has already changed by the time anything reads it back. `101%` and `-1%` reach
    ///   the axes the same way, which is what pins [`crate::axes::hwb_to_hsl`] (`hwb/four_args::whiteness/*`).
    pub fn hwb(hue: &Value, whiteness: &Value, blackness: &Value, alpha: f64) -> Option<Self> {
        let (white, black) = crate::axes::normalise_hwb(number(whiteness)?, number(blackness)?);
        Some(Self {
            channels: [
                Channel {
                    value: angle_of(hue)?,
                    percent: false,
                },
                Channel {
                    value: white,
                    percent: true,
                },
                Channel {
                    value: black,
                    percent: true,
                },
            ],
            alpha: clamp_alpha(alpha),
            authored: None,
            representation: Representation::Hwb,
        })
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
        Some(Self {
            channels: [
                Channel {
                    value: angle_of(hue)?,
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
            authored: None,
            representation: Representation::Hsl,
        })
    }

    /// The same colour with one `rgb` channel written, keeping how it was made: `color.change(black,
    /// $red: 255)` → `red` and `color.change(red, $red: 100)` → `#640000` (`change/rgb.hrx`).
    ///
    /// **Nothing is clamped here.** `color.change(black, $red: 500)` keeps 500 and the corpus prints the
    /// result as `hsl(0, 5000%, 98.0392156863%)` (`…::red/out_of_range`) — an out-of-gamut colour is a value
    /// Sass represents, not an error, and clamping would turn it into `rgb(255, 0, 0)`, a different colour.
    /// [`Channel::clamped`] stays where it is: only a *written* `rgb`/`hsl` channel clamps at creation.
    pub fn with_channel(&self, index: usize, value: f64) -> Option<Self> {
        let written = Channel {
            value,
            percent: false,
        };
        (self.writes_bytes() && index < self.channels.len()).then(|| Self {
            channels: [
                if index == 0 {
                    written
                } else {
                    self.channels[0]
                },
                if index == 1 {
                    written
                } else {
                    self.channels[1]
                },
                if index == 2 {
                    written
                } else {
                    self.channels[2]
                },
            ],
            ..self.clone()
        })
    }

    /// Whether this colour's channels are `0–255` bytes, which is what makes a channel **writable** by
    /// `color.change`; an `hsl` or `hwb` colour stores different axes, and writing a byte into them would be
    /// a different colour (`change/hsl.hrx` and `change/hwb.hrx` are those calls, and they decline).
    pub fn writes_bytes(&self) -> bool {
        matches!(
            self.representation,
            Representation::Legacy
                | Representation::Percent
                | Representation::Hex
                | Representation::Named
        )
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
