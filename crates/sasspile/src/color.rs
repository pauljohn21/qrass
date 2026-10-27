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
        let [red, green, blue] = [channel(red)?, channel(green)?, channel(blue)?];
        Some(Self {
            channels: [red.clamped(), green.clamped(), blue.clamped()],
            alpha: clamp_alpha(alpha),
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
            representation: Representation::Hex,
            ..color
        })
    }

    /// A colour written as text: hex or keyword. Used where an argument arrives as unquoted text, which
    /// is how a declaration keeps `red` and `#123` — `rgb(...)`/`hsl(...)` written as a call already
    /// evaluated into [`Value::Color`] before it gets here.
    pub fn of(value: &Value) -> Option<Color> {
        match value {
            Value::Color(color) => Some(color.clone()),
            Value::Raw(text) => {
                let text = text.trim();
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
    /// than a wrong number: reading `color.red(hsl(0, 100%, 50%))` needs the rgb↔hsl conversion, which is
    /// its own change (the fixtures for it are `color/red.hrx` rows beyond the four pinned today).
    pub fn byte(&self, index: usize) -> Option<f64> {
        match self.representation {
            Representation::Legacy | Representation::Percent | Representation::Hex => self
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

    /// The hue in degrees, for `color.hue` (`hue.hrx`: `359deg`, and `hsl(540, 50%, 50%)` → `180deg`
    /// because the hue was already folded into `0–360` at creation).
    pub fn hue(&self) -> Option<f64> {
        self.hsl_channel(0)
    }

    /// The saturation, for `color.saturation` (`saturation.hrx`: `100%`, `0%`, `50%`, `0.5%`, `42%`).
    pub fn saturation(&self) -> Option<f64> {
        self.hsl_channel(1)
    }

    /// The lightness, for `color.lightness` (`lightness.hrx`: the same five readings in the L axis).
    pub fn lightness(&self) -> Option<f64> {
        self.hsl_channel(2)
    }

    /// One axis of a colour **held in the hsl space**, or nothing.
    ///
    /// An `rgb`/hex/keyword-sourced colour is not read here: every value row of
    /// `core_functions/color/{hue,saturation,lightness}.hrx` feeds an `hsl(…)` colour (the non-`hsl`
    /// arguments in those files are `error/*` rows, which want messages this project has not built), so
    /// the rgb→hsl conversion has no golden to be written against yet. Declining keeps a converted guess
    /// out of the pass count; the conversion is `adjust`/`change`/`channel`'s prerequisite and its own step.
    fn hsl_channel(&self, index: usize) -> Option<f64> {
        match self.representation {
            Representation::Hsl => self.channels.get(index).map(|channel| channel.value),
            _ => None,
        }
    }

    /// The same colour with its hue moved by `degrees` and its saturation set to `saturation`, going
    /// through the hsl axes (`color.grayscale(red)` → `rgb(50%, 50%, 50%)`: the lightness survives the
    /// round trip, the saturation does not).
    ///
    /// The axes follow CSS Color 4's arithmetic, but the *readings* below are what the corpus pins, and
    /// they are the reason the conversion exists at all: `color.grayscale(#633736)` → `rgb(30%, 30%, 30%)`
    /// (L = (99 + 54)/2 = 76.5 of 255), while `color.grayscale(#494949)` → `#494949` — the same operation
    /// on a colour whose lightness is an exact byte prints hex. That pair is the whole `fractional →
    /// percentage` law in two lines, and it lives in [`Color::to_css`].
    pub fn recolour(&self, hue_shift: f64, saturation: Option<f64>) -> Self {
        match self.representation {
            Representation::Hsl => {
                let [hue, current, lightness] = self.channels.map(|channel| channel.value);
                Self {
                    channels: [
                        Channel {
                            value: (hue + hue_shift).rem_euclid(360.0),
                            percent: false,
                        },
                        Channel {
                            value: saturation.unwrap_or(current),
                            percent: true,
                        },
                        Channel {
                            value: lightness,
                            percent: true,
                        },
                    ],
                    ..self.clone()
                }
            }
            _ => {
                let [hue, axis_saturation, lightness] = self.axes();
                let channels = [
                    hue + hue_shift,
                    saturation.unwrap_or(axis_saturation),
                    lightness,
                ];
                Self::from_axes(self, channels)
            }
        }
    }

    /// The hsl axes of an `rgb`-space colour, via [`crate::axes`] — the conversion is pure `f64` and is
    /// tested there on its own, so this only hands it the channel values.
    fn axes(&self) -> [f64; 3] {
        let [red, green, blue] = self.channels.map(|channel| channel.byte());
        crate::axes::rgb_to_axes(red, green, blue)
    }

    /// Rebuild a colour of the same kind from hsl axes, keeping the channels un-rounded so the printing
    /// rule can decide between numbers, percentages and hex.
    fn from_axes(source: &Color, [hue, saturation, lightness]: [f64; 3]) -> Self {
        let channels = crate::axes::axes_to_rgb(hue, saturation, lightness);
        Self {
            channels: channels.map(|value| Channel {
                value,
                percent: false,
            }),
            alpha: source.alpha,
            representation: source.representation,
        }
    }

    /// Whether any channel is not a whole byte — the trigger for printing percentages instead of numbers
    /// (`grayscale(red)` → `rgb(50%, 50%, 50%)`, `grayscale(#494949)` → `#494949`).
    fn fractional(&self) -> bool {
        self.channels
            .iter()
            .any(|channel| channel.byte().fract() != 0.0)
    }

    /// The same colour written as `rgb(…, …%, …)`: each channel converted to its share of 255.
    fn percent_form(&self) -> String {
        let channels = self.channels.map(|channel| Channel {
            value: channel.percent_form(),
            percent: true,
        });
        self.legacy(channels, true)
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
    ///   unlike an `rgb` channel they are **not clamped**: `hsl(0, 500%, 50%)` stays
    ///   (`hsl/three_args/bounds.hrx`);
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
                    value: number(saturation)?,
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

    /// The CSS text. Percentages are printed with Sass's ten-significant-digit number form, which is what
    /// `67.8431372549%` (from `173`) comes out of.
    pub fn to_css(&self) -> String {
        let byte = |value: u32| format!("{value:02x}");
        match self.representation {
            Representation::Hex => {
                // A colour whose channels are not whole bytes prints percentages, not hex and not
                // numbers: `grayscale(#633736)` → `rgb(30%, 30%, 30%)` while `grayscale(#494949)` →
                // `#494949` (`core_functions/color/grayscale.hrx`).
                if self.fractional() {
                    return self.percent_form();
                }
                // An opaque colour comes back out as hex, but a semi-transparent one takes the legacy
                // functional form: `rgb(#123, 1)` → `#112233` while `rgb(#123, 0.5)` → `rgba(17, 34, 51,
                // 0.5)` (`rgb/two_args::opaque_to/{opaque,partial}`) — no eight-digit hex is printed here.
                let bytes = self.channels.map(|channel| channel.byte().round() as u32);
                if self.alpha < 1.0 {
                    return self.legacy(self.channels, false);
                }
                format!("#{}{}{}", byte(bytes[0]), byte(bytes[1]), byte(bytes[2]))
            }
            Representation::Legacy => {
                if self.fractional() {
                    return self.percent_form();
                }
                self.legacy(self.channels, false)
            }
            Representation::Percent => self.percent_form(),
            // Hue is a bare number and the other two always carry a `%`; the name still follows the alpha
            // exactly as the `hsl/four_args/alpha.hrx` rows show (`100%` alpha → `hsl(…)`, `0%` →
            // `hsla(…, 0)`).
            Representation::Hsl => {
                let [hue, saturation, lightness] = self.channels;
                let channels = format!(
                    "{}, {}%, {}%",
                    crate::value::format_number(hue.value),
                    crate::value::format_number(saturation.value),
                    crate::value::format_number(lightness.value)
                );
                let name = if self.alpha >= 1.0 { "hsl" } else { "hsla" };
                if self.alpha >= 1.0 {
                    format!("{name}({channels})")
                } else {
                    format!(
                        "{name}({channels}, {})",
                        crate::value::format_number(self.alpha)
                    )
                }
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
