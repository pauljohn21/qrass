//! The colour **axis arithmetic**: moving a colour through the hsl axes and averaging two colours.
//! Split out of [`super`] because it is the only place a colour leaves the space it was created in, and
//! keeping it apart is what lets [`crate::axes`] be tested as pure `f64` while this file owns only the
//! clamping, the wrapping and the representation the result keeps.
//!
//! The printing law that reads these results (whole bytes print as numbers or hex, anything between prints
//! as percentages) is a separate concern and lives in [`super::text`].

use super::{Channel, Color, Representation};

impl Color {
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
    /// ([`crate::axes`]) of its channels. `pub(super)` because [`super::Color::hsl_channel`] reads the same
    /// axes an operation writes — one path, so an accessor and a `darken` cannot disagree about a colour's
    /// lightness.
    pub(super) fn axes_of(&self) -> [f64; 3] {
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
}
