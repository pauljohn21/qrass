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
    /// than a wrong number: reading `color.red(hsl(0, 100%, 50%))` needs the rgb↔hsl conversion, which is
    /// its own change (the fixtures for it are `color/red.hrx` rows beyond the four pinned today).
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

    /// The same colour after a change to its **hsl axes**, wherever it is stored, with the two clamps and
    /// the hue wrap the fixtures state:
    /// * `saturate(plum, 100%)` and `saturate(plum, 53%)` both give `#ff7eff` — saturation clamps at 100
    ///   and, landing on whole bytes, prints hex (`core_functions/color/saturate.hrx::{max,max_remaining}`);
    /// * `desaturate(plum, 100%)` → `rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)` — lightness
    ///   `(221 + 160)/2 = 190.5` of 255 is not a whole byte, so percentages (`desaturate.hrx::max`);
    /// * `darken(red, 100%)` clamps lightness at 0 rather than going negative.
    ///
    /// `color.grayscale` is the same entry with saturation pinned to zero, which is why it no longer needs
    /// its own path.
    pub fn transform_axes(&self, change: impl Fn([f64; 3]) -> [f64; 3]) -> Self {
        let [hue, saturation, lightness] = change(self.axes_of());
        let axes = [
            hue.rem_euclid(360.0),
            saturation.clamp(0.0, 100.0),
            lightness.clamp(0.0, 100.0),
        ];
        match self.representation {
            Representation::Hsl => Self {
                channels: [
                    Channel {
                        value: axes[0],
                        percent: false,
                    },
                    Channel {
                        value: axes[1],
                        percent: true,
                    },
                    Channel {
                        value: axes[2],
                        percent: true,
                    },
                ],
                ..self.clone()
            },
            _ => Self::from_axes(self, axes),
        }
    }

    /// The colour's axes: the stored ones when it is held in hsl, otherwise the round trip
    /// ([`crate::axes`]) of its channels.
    fn axes_of(&self) -> [f64; 3] {
        match self.representation {
            Representation::Hsl => self.channels.map(|channel| channel.value),
            _ => self.axes(),
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

    /// The legacy `mix`: an **alpha-weighted** average in sRGB, whose shape only this formula explains.
    /// Verified row by row against `core_functions/color/mix/{unweighted,explicit_weight,alpha}.hrx`:
    /// * `color.mix(#91e16f, #0144bf)` → `rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)` — with equal
    ///   alphas the weight is a plain 0.5, giving `(145 + 1)/2 = 73` over 255;
    /// * `color.mix(rgba(#91e16f, 0.8), rgba(#0144bf, 0.3))` → `rgba(42.7450980392%, …, 0.55)` — `w = 0`,
    ///   `a = 0.5`, so the first colour carries `0.75`: `145 × 0.75 + 1 × 0.25 = 109`, and alpha is the
    ///   average; the mirrored row (`0.4`/`0.9`) gives `0.25` → `37` → `14.5098039216%`;
    /// * `color.mix(#91e16f, transparent)` → `rgba(145, 225, 111, 0.5)` — `a = 1` makes the first colour
    ///   carry everything, and the result stays whole-byte so it prints numbers;
    /// * the result keeps the **first** colour's representation, which is what makes that last row print
    ///   numbers rather than a keyword when the other argument is `transparent`.
    ///
    /// A colour held in the hsl space has no `0–255` channels to mix, so the call declines rather than
    /// inventing a converted answer.
    pub fn mixed(first: &Color, second: &Color, weight: f64) -> Option<Self> {
        let span = first.alpha - second.alpha;
        let tilt = 2.0 * (weight / 100.0).clamp(0.0, 1.0) - 1.0;
        let ratio = if tilt * span == -1.0 {
            tilt
        } else {
            (tilt + span) / (1.0 + tilt * span)
        } / 2.0
            + 0.5;
        let channel = |colour: &Color, index: usize| match colour.representation {
            Representation::Hsl => None,
            _ => Some(colour.channels[index].byte()),
        };
        let [red, green, blue] = [0, 1, 2].map(|index| {
            let value = channel(first, index)? * ratio + channel(second, index)? * (1.0 - ratio);
            Some(Channel {
                value,
                percent: false,
            })
        });
        Some(Self {
            channels: [red?, green?, blue?],
            alpha: (first.alpha + second.alpha) / 2.0,
            representation: first.representation,
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
}
