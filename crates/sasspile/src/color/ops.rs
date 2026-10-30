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
    pub fn transform_axes(&self, change: impl Fn([f64; 3]) -> [f64; 3]) -> Option<Self> {
        // A colour in its own space is refused **by this judge**, which answers `None` for
        // `Representation::InSpace`: deriving axes from the byte triple it does not carry answered
        // `complement(color(srgb …))` with zeros, and echoing the input back claimed the operation had
        // happened. The Color-4 rule the corpus asks for is a conversion through the named space
        // (`complement.hrx::space/missing/modern/different` writes
        // `color.complement(color(srgb none 100 200), $space: oklch)` and expects converted channels, and
        // `grayscale.hrx::modern/predefined` expects `color(xyz …)` to come out changed), which this slice has
        // not built. Said plainly, because the distinction matters to the next reader: **no corpus row reaches
        // this refusal today** — those calls already decline at argument binding, because `$space:` is not a
        // declared parameter of them (`builtins/mod.rs` reports the unknown label and the call keeps its text),
        // so the error this path returns has zero measured hits. Making `$space:` a parameter of the
        // Color-4 accessors is the precondition for the refusal ever being exercised; `tasks.md` 9.9 keeps it
        // open.
        let [hue, saturation, lightness] = change(self.axes_of()?);
        let axes = [
            hue.rem_euclid(360.0),
            saturation.clamp(0.0, 100.0),
            lightness.clamp(0.0, 100.0),
        ];
        Some(match self.representation {
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
            // A `hwb` colour cannot store an hsl change as whiteness and blackness, so it comes back as the
            // `hsl` value it prints — and through the `rgb` round trip, which is what the corpus shows:
            // `color.grayscale(hwb(120deg 10% 20%))` → `hsl(0, 0%, 45%)` (`grayscale.hrx::
            // legacy/powerless/different`), hue `0` because a grey has no hue, whereas the same operation on
            // an `hsl` colour keeps its hue (`grayscale(hsl(120deg 10% 20%))` → `hsl(120, 0%, 20%)`).
            Representation::Hwb => {
                let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
                let [hue, saturation, lightness] = crate::axes::rgb_to_axes(rgb[0], rgb[1], rgb[2]);
                Self {
                    channels: [
                        Channel {
                            value: hue,
                            percent: false,
                        },
                        Channel {
                            value: saturation,
                            percent: true,
                        },
                        Channel {
                            value: lightness,
                            percent: true,
                        },
                    ],
                    alpha: self.alpha,
                    representation: Representation::Hsl,
                    // A `hwb` colour moved through the axes becomes an ordinary hsl value; there is no
                    // “as written” triple behind it beyond these axes, which `axes_of` already returns.
                    authored: None,
                }
            }
            // Named, not folded into `_`: unreachable through the `?` above, and a future variant must not
            // fall into the bytes path silently — that silence is how a space colour came to print
            // `rgb(0, 0, 0)`.
            Representation::InSpace(..) => return None,
            _ => Self::from_axes(self, axes),
        })
    }

    /// The same colour with its **hsl axes written, without clamping** — `color.change`'s law, which is
    /// *not* the adjust family's: `color.change(plum, $saturation: 120%)` keeps 120 and prints
    /// `hsl(300, 120%, 74.7058823529%)` (`change/hsl.hrx::saturation/above_range`), while
    /// `saturate(plum, 100%)` clamps at 100 and prints `#ff7eff` (`saturate.hrx::max`). Two clamping
    /// behaviours that both come from rows, so they are two entry points rather than one flag a caller has
    /// to remember to pass.
    ///
    /// `None` for a `hwb`-created colour: it stores whiteness and blackness, and no fixture says what
    /// writing hsl axes into it means, so the call declines instead of inventing a round trip.
    pub fn with_axes(&self, write: impl FnOnce([f64; 3]) -> [f64; 3]) -> Option<Self> {
        // The same refusal as [`Self::transform_axes`], stated once by [`Color::axes_of`]: writing hsl axes onto
        // a colour that carries its own space would silently throw that space away and print srgb bytes.
        let current = self.axes_of()?;
        match self.representation {
            Representation::Hwb => None,
            // The axes are the value, so they go straight back in — hue as a plain number of degrees and
            // the other two as percentages, exactly as [`Color::hsl`] stores them.
            Representation::Hsl => {
                let [hue, saturation, lightness] = write(current);
                Some(Self {
                    channels: [
                        Channel {
                            value: hue,
                            percent: false,
                        },
                        Channel {
                            value: saturation,
                            percent: true,
                        },
                        Channel {
                            value: lightness,
                            percent: true,
                        },
                    ],
                    alpha: self.alpha,
                    representation: Representation::Hsl,
                    // The channels *are* the axes for an hsl colour, so there is nothing extra to remember:
                    // an out-of-gamut hsl value prints from `axes_of`, which returns exactly these.
                    authored: None,
                })
            }
            // An `rgb`-family colour keeps its own representation and gains the bytes the new axes imply;
            // `to_css` then decides between numbers, percentages, hex, a keyword and — for axes outside the
            // gamut, which is what makes 120% visible — the hsl form.
            _ => {
                let axes = write(current);
                let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
                Some(Self {
                    channels: rgb.map(|value| Channel {
                        value,
                        percent: false,
                    }),
                    alpha: self.alpha,
                    representation: self.representation,
                    // The axes the call named, kept for the out-of-gamut print: `with_axes` is the only path
                    // that writes an hsl triple onto a colour built some other way, and deriving the triple
                    // back from the implied bytes changes it when they fall outside the gamut
                    // (`color.change(red, $lightness: 120%)` → `hsl(0, 100%, 120%)`, `change/hsl.hrx`).
                    authored: Some(axes),
                })
            }
        }
    }

    /// The same colour with its **whiteness and blackness written**, keeping the caller's own representation:
    /// `color.change(#cc6666, $whiteness: 80%)` → `#cccccc` (`change/hwb.hrx::whiteness/high`), while a pair
    /// that leaves the gamut prints the hsl triple it implies —
    /// `color.change(#993333, $whiteness: 50%, $blackness: -20%)` → `hsl(0, 233.3333333333%, 85%)`
    /// (`…::out_of_range`), which is [`super::Color::authored`] doing its job.
    ///
    /// An `hsl`-created colour is not answered here: no fixture writes whiteness or blackness into one, and
    /// inventing the round trip through bytes would be a rule nobody stated.
    pub fn with_hwb(&self, hue: f64, whiteness: f64, blackness: f64) -> Option<Self> {
        let (white, black) = crate::axes::normalise_hwb(whiteness, blackness);
        let axes = crate::axes::hwb_to_hsl(hue, white, black);
        let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
        self.writes_bytes().then(|| Self {
            channels: rgb.map(|value| Channel {
                value,
                percent: false,
            }),
            alpha: self.alpha,
            representation: self.representation,
            authored: Some(axes),
        })
    }

    /// The colour's axes: the stored ones when it is held in hsl, the derived ones for a `hwb` colour,
    /// otherwise the round trip ([`crate::axes`]) of its channels. `pub(super)` because
    /// [`super::Color::hsl_channel`] reads the same axes an operation writes — one path, so an accessor and a
    /// `darken` cannot disagree about a colour's lightness.
    ///
    /// **`None` for a colour in its own colour space**, and that is the whole point of the variant: such a
    /// colour stores no hsl axes, so the legacy accessors have no number to hand back and decline to their
    /// text — which is what the corpus asks for (`hue.hrx::error/non_legacy` answers
    /// `color.hue(lch(0% 0 0deg))` with "only supported for legacy colors", not a hue). Reading the empty byte
    /// triple instead is what produced `0%` and `rgb(85, 85, 85)` on 15 rows.
    pub(super) fn axes_of(&self) -> Option<[f64; 3]> {
        match self.representation {
            Representation::Hsl => Some(self.stored_axes()),
            Representation::Hwb => Some(self.hwb_axes()),
            Representation::InSpace(..) => None,
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => Some(self.axes()),
        }
    }

    /// The triple a colour **stores as written**: for `hsl` the hsl axes, for `hwb` hue/whiteness/blackness.
    /// One statement, shared by [`Color::axes_of`] and the printer, so neither restates the reading.
    pub(super) fn stored_axes(&self) -> [f64; 3] {
        self.channels.map(|channel| channel.value)
    }

    /// A `hwb` colour's hsl axes, derived, not stored: `hwb(0deg, 30%, 40%)` is asked for as
    /// `hsl(0, 33.3333333333%, 45%)` (`hwb/three_args/units::hue/deg`), and `color.channel` reads that same hue
    /// back (`channel/hwb.hrx::local/hue`). Shared by [`Color::axes_of`] and the printer for the same reason.
    pub(super) fn hwb_axes(&self) -> [f64; 3] {
        let [hue, whiteness, blackness] = self.stored_axes();
        crate::axes::hwb_to_hsl(hue, whiteness, blackness)
    }

    /// The hsl axes of an `rgb`-space colour, via [`crate::axes`] — the conversion is pure `f64` and is
    /// tested there on its own, so this only hands it the channel values.
    pub(super) fn axes(&self) -> [f64; 3] {
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
            // The axes are re-derived from these bytes on demand, which is exact while they stay in the
            // gamut — and `transform_axes` clamps, so they do.
            authored: None,
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
    /// A colour held in the hsl or hwb space has no `0–255` channels to mix, so the call declines rather
    /// than inventing a converted answer: no fixture states how `color.mix` treats those spaces, and mixing
    /// a `hwb` colour's whiteness and blackness as if they were bytes would be a wrong value the pass count
    /// cannot see.
    pub fn mixed(first: &Color, second: &Color, weight: f64) -> Option<Self> {
        let span = first.alpha - second.alpha;
        let tilt = 2.0 * (weight / 100.0).clamp(0.0, 1.0) - 1.0;
        let ratio = if tilt * span == -1.0 {
            tilt
        } else {
            (tilt + span) / (1.0 + tilt * span)
        } / 2.0
            + 0.5;
        let channel = |colour: &Color, index: usize| {
            // [`Color::writes_bytes`] is the one judge of which representations store `0–255` bytes, so an
            // `hsl`/`hwb` colour and a colour in its own space both answer `None` here: reading the byte triple
            // the latter leaves behind would answer `rgb(0, 0, 0)` — a wrong value no pass count can see
            // (`tests/color.rs::a_colour_in_an_unimplemented_space_is_not_answered_in_srgb`). Mixing across
            // spaces needs the conversion in [`crate::color::space`] driven by a common space, which is what
            // `mix/named.hrx::polar_space` and `::rectangular_space` ask for; it is not a guess here.
            colour.writes_bytes().then(|| colour.channels[index].byte())
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
            authored: None,
        })
    }
}
