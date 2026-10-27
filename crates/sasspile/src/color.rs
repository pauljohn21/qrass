//! Color — a colour **created by a function**, and the representation law that decides how it prints.
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
    fn clamped(self) -> Self {
        let ceiling = if self.percent { 100.0 } else { 255.0 };
        let value = if self.value.is_nan() || self.value < 0.0 {
            0.0
        } else {
            self.value.min(ceiling)
        };
        Channel {
            value: value + 0.0,
            ..self
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
        let [red, green, blue] = [channel(red)?, channel(green)?, channel(blue)?];
        Some(Self {
            channels: [red.clamped(), green.clamped(), blue.clamped()],
            alpha: if alpha.is_nan() {
                0.0
            } else {
                alpha.clamp(0.0, 1.0)
            },
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

    /// A colour from `0–255` byte values, for hex parsing.
    fn rgb(red: f64, green: f64, blue: f64, alpha: f64) -> Option<Self> {
        let plain = |value: f64| Value::Number(value, crate::unit::Unit::unitless());
        Self::new(&plain(red), &plain(green), &plain(blue), alpha)
    }

    /// The same colour under a different alpha, keeping how it was made: `rgb(#123, 0.5)` is the hex
    /// channels with the new alpha, not a different colour (`two_args::opaque_to/partial`).
    pub fn with_alpha(&self, alpha: f64) -> Self {
        Self {
            alpha: if alpha.is_nan() {
                0.0
            } else {
                alpha.clamp(0.0, 1.0)
            },
            ..self.clone()
        }
    }

    /// The CSS text. Percentages are printed with Sass's ten-significant-digit number form, which is what
    /// `67.8431372549%` (from `173`) comes out of.
    pub fn to_css(&self) -> String {
        let byte = |value: u32| format!("{value:02x}");
        match self.representation {
            Representation::Hex => {
                // An opaque colour comes back out as hex, but a semi-transparent one takes the legacy
                // functional form: `rgb(#123, 1)` → `#112233` while `rgb(#123, 0.5)` → `rgba(17, 34, 51,
                // 0.5)` (`rgb/two_args::opaque_to/{opaque,partial}`) — no eight-digit hex is printed here.
                let bytes = self.channels.map(|channel| channel.byte().round() as u32);
                if self.alpha < 1.0 {
                    return self.legacy(self.channels, false);
                }
                format!("#{}{}{}", byte(bytes[0]), byte(bytes[1]), byte(bytes[2]))
            }
            Representation::Legacy => self.legacy(self.channels, false),
            Representation::Percent => {
                let channels = self.channels.map(|channel| Channel {
                    value: channel.percent_form(),
                    percent: true,
                });
                self.legacy(channels, true)
            }
        }
    }

    /// The legacy functional form: `rgb(r, g, b)` while opaque, `rgba(…)` once there is alpha (which is
    /// what every `four_args/clamped` row prints), and channels in whichever scale this representation
    /// uses.
    fn legacy(&self, printed: [Channel; 3], percent: bool) -> String {
        let channels = printed
            .iter()
            .map(|channel| {
                let number = crate::value::format_number(if percent {
                    channel.value
                } else {
                    channel.value.round()
                });
                format!("{number}{}", if percent { "%" } else { "" })
            })
            .collect::<Vec<String>>()
            .join(", ");
        let name = if self.alpha >= 1.0 { "rgb" } else { "rgba" };
        if self.alpha >= 1.0 {
            return format!("{name}({channels})");
        }
        format!(
            "{name}({channels}, {})",
            crate::value::format_number(self.alpha)
        )
    }
}
