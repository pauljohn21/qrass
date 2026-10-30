//! The colour **spaces** — the model `core_functions/color/to_space/**` and the `color()` constructor need,
//! kept apart from [`crate::color`]'s srgb-era laws.
//!
//! Why a separate owned value instead of more fields on [`crate::color::Representation`]: a colour in
//! `display-p3` or `oklch` *is* its three channel values plus the space they belong to, so the space travels
//! as data on the colour (`Color::space`) and every existing print/read law keeps working untouched for the
//! colours that have no space. `None` means "the channels are the srgb-family bytes / hsl-hwb axes the struct
//! already stored" — which is what keeps 1,690 passing rows from being re-decided by this file.
//!
//! The math is one table per space (primaries matrix, transfer function, reference white) and **one** route:
//! every conversion goes through XYZ referenced to D65. The white-point adaptation is computed from the
//! Bradford cone matrix rather than copied as a rounded constant, because the corpus prints ten significant
//! digits and a hand-copied matrix (`0.964295676`) is already wrong in the tenth (`color(srgb 1 1 1)` converted
//! to `xyz-d50` is `color(xyz-d50 0.9642956764 1 0.8251046025)`,
//! `to_space/srgb/xyz_d50.hrx::white`).

use super::matrix::{from_polar, of_xyz_d65, to_polar, to_xyz_d65};
use crate::value::format_number;

/// The spaces this compiler can carry and convert. `rgb`/`hsl`/`hwb` are absent on purpose: those targets are
/// answered by the legacy [`crate::color::Representation`] they already print in, not by a space channel
/// triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Srgb,
    SrgbLinear,
    DisplayP3,
    DisplayP3Linear,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    Xyz,
    XyzD50,
    Lab,
    Lch,
    Oklab,
    Oklch,
}

/// A colour's own space and the channel values as it carries them. `None` is a `none` channel — powerless,
/// which is data rather than a sentinel number, because `to-space` treats it as `0` for the maths while still
/// answering `none` when nothing at all was given (`to_space/srgb/lab.hrx::missing/*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InSpace {
    pub space: Space,
    pub channels: [Option<f64>; 3],
}

/// Which white point a space's numbers are referenced to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum White {
    D65,
    D50,
}

/// The transfer between the printed channel and the linear-light value the matrices expect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transfer {
    Linear,
    /// srgb, and the same curve for `display-p3` (`color(display-p3 …)` channels).
    Srgb,
    /// Pure gamma `2.19921` (`563/256`).
    A98,
    /// Gamma `1.8` with the linear segment below `1/16`.
    Prophoto,
    /// The `alpha = 1.09929682680944, beta = 0.081` curve.
    Rec2020,
}

impl Space {
    /// The space a written name names. Underscore and hyphen spellings are the same space, because the corpus
    /// fixture files use the former (`to_space/srgb/xyz_d50.hrx`) while CSS prints the latter
    /// (`color(xyz-d50 …)`), and a Sass name is case-insensitive (`color/color/no_alpha.hrx::case`).
    pub fn parse(name: &str) -> Option<Self> {
        let key: String = name.to_ascii_lowercase().replace('_', "-");
        match key.as_str() {
            "srgb" => Some(Self::Srgb),
            "srgb-linear" => Some(Self::SrgbLinear),
            "display-p3" => Some(Self::DisplayP3),
            "display-p3-linear" => Some(Self::DisplayP3Linear),
            "a98-rgb" => Some(Self::A98Rgb),
            "prophoto-rgb" => Some(Self::ProphotoRgb),
            "rec2020" => Some(Self::Rec2020),
            "xyz" | "xyz-d65" => Some(Self::Xyz),
            "xyz-d50" => Some(Self::XyzD50),
            "lab" => Some(Self::Lab),
            "lch" => Some(Self::Lch),
            "oklab" => Some(Self::Oklab),
            "oklch" => Some(Self::Oklch),
            _ => None,
        }
    }

    /// The name the printed form uses.
    pub fn css(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::SrgbLinear => "srgb-linear",
            Self::DisplayP3 => "display-p3",
            Self::DisplayP3Linear => "display-p3-linear",
            Self::A98Rgb => "a98-rgb",
            Self::ProphotoRgb => "prophoto-rgb",
            Self::Rec2020 => "rec2020",
            Self::Xyz => "xyz",
            Self::XyzD50 => "xyz-d50",
            Self::Lab => "lab",
            Self::Lch => "lch",
            Self::Oklab => "oklab",
            Self::Oklch => "oklch",
        }
    }

    /// The primaries matrix, linear-light of this space to XYZ **at the space's own white**. `None` for the
    /// spaces whose channels already are (or are derived from) XYZ.
    pub(super) fn matrix(self) -> Option<&'static [[f64; 3]; 3]> {
        match self {
            Self::Srgb | Self::SrgbLinear => Some(&super::matrix::SRGB_PRIMARIES),
            Self::DisplayP3 | Self::DisplayP3Linear => Some(&[
                [
                    0.486_570_948_648_212,
                    0.265_667_693_169_093,
                    0.198_217_285_234_362,
                ],
                [
                    0.228_974_564_069_775,
                    0.691_738_521_836_506,
                    0.079_286_914_093_745,
                ],
                [0.0, 0.045_113_376_859_663, 1.043_944_361_990_867],
            ]),
            Self::A98Rgb => Some(&[
                [
                    0.576_669_042_910_130,
                    0.186_614_468_906_882,
                    0.188_887_366_974_017,
                ],
                [
                    0.297_344_875_851_542,
                    0.627_363_566_387_213,
                    0.075_291_557_851_244,
                ],
                [
                    0.027_031_362_253_985,
                    0.070_605_520_089_455,
                    0.952_976_666_330_321,
                ],
            ]),
            Self::ProphotoRgb => Some(&[
                [
                    0.797_760_489_672_548,
                    0.137_194_907_788_244,
                    0.065_037_549_835_993,
                ],
                [
                    0.288_071_128_224_072,
                    0.711_835_993_922_564,
                    0.000_089_931_689_853,
                ],
                [0.0, 0.0, 0.825_104_602_595_398],
            ]),
            Self::Rec2020 => Some(&[
                [
                    0.636_958_048_301_291,
                    0.144_616_903_586_043,
                    0.168_880_975_164_172,
                ],
                [
                    0.262_703_970_245_312,
                    0.677_998_271_772_319,
                    0.059_297_688_361_440,
                ],
                [0.0, 0.028_072_693_049_087, 1.060_985_057_639_950],
            ]),
            _ => None,
        }
    }

    pub(super) fn white(self) -> White {
        match self {
            Self::ProphotoRgb | Self::XyzD50 | Self::Lab | Self::Lch => White::D50,
            _ => White::D65,
        }
    }

    pub(super) fn transfer(self) -> Transfer {
        match self {
            Self::Srgb | Self::DisplayP3 => Transfer::Srgb,
            Self::SrgbLinear | Self::DisplayP3Linear => Transfer::Linear,
            Self::A98Rgb => Transfer::A98,
            Self::ProphotoRgb => Transfer::Prophoto,
            Self::Rec2020 => Transfer::Rec2020,
            _ => Transfer::Linear,
        }
    }

    /// Whether this space's channels are a `color()` call's `r g b` (or `x y z`) triple printed as plain
    /// numbers, as against the `lab(…)`/`lch(…)` forms with their own suffixes.
    pub(super) fn is_functional(self) -> bool {
        !matches!(self, Self::Lab | Self::Lch | Self::Oklab | Self::Oklch)
    }

    /// The number a **full-scale percentage** names in this space's lightness: `lab`/`lch` state lightness on
    /// a `0–100` scale, so their `50%` *is* `50` (`to_space/srgb/lab.hrx::middle` reads `lab(44.1210142293% …)`
    /// and prints the same number it holds), while every other space is a fraction of one, so `100%` is `1`
    /// (`to_space/oklab/srgb.hrx::white` reads `oklab(100% 0 0)` and wants `color(srgb 1 1 1)`;
    /// `to_space/srgb/oklab.hrx::white` wants `oklab(100% 0 0)` back for white).
    ///
    /// This is **one** law with both directions read from it: the argument parser divides by it (in
    /// `builtins/color/constructors.rs`) and [`print`] multiplies by it before writing the `%`. It was
    /// written three times before — the print suffix, this divisor, and the matrix's `0–1` domain — and the
    /// three disagreed for `oklab`/`oklch`, turning `oklab(100% 0 0)` into `L = 100` and a conversion into
    /// numbers of order `1e5`.
    pub fn percent_full(self) -> f64 {
        match self {
            Self::Lab | Self::Lch => 1.0,
            _ => 100.0,
        }
    }
}

/// One channel as printed: `none` for a powerless one, otherwise the number the corpus's printer yields.
fn channel(value: Option<f64>, suffix: &str) -> String {
    value.map_or_else(
        || "none".to_string(),
        |number| format!("{}{suffix}", format_number(number)),
    )
}

/// The CSS text of a colour carried in its own space, in the forms the corpus pins:
/// `color(display-p3 0.25 0.4 0.77)`, `lab(44.1210142293% 10.9543336098 -59.0857381981)`,
/// `lch(44.1210142293% 60.0926108873 280.5032243387deg)` and an alpha after a `/`
/// (`to_space/srgb/*.hrx::middle` and `::alpha/partial`).
pub fn print(space: Space, channels: [Option<f64>; 3], alpha: f64) -> String {
    let suffixes: [&str; 3] = if space.is_functional() {
        ["", "", ""]
    } else {
        match space {
            // Lightness is a percentage; a and b are plain.
            Space::Lab | Space::Oklab => ["%", "", ""],
            // Lightness a percentage, chroma plain, hue an angle.
            _ => ["%", "", "deg"],
        }
    };
    let body = [0, 1, 2]
        .iter()
        .map(|index| {
            // A percentage channel is written back on the space's own scale — `L = 1` of `oklab` is `100%`,
            // `L = 44.1210142293` of `lab` is `44.1210142293%` — through the same [`Space::percent_full`] the
            // reading divides by, so the two directions of one law cannot drift apart again.
            let scaled = channels[*index].map(|value| {
                if suffixes[*index] == "%" {
                    value * space.percent_full()
                } else {
                    value
                }
            });
            channel(scaled, suffixes[*index])
        })
        .collect::<Vec<String>>()
        .join(" ");
    let alpha_part = if alpha < 1.0 {
        format!(" / {}", format_number(alpha))
    } else {
        String::new()
    };
    if space.is_functional() {
        format!("color({} {body}{alpha_part})", space.css())
    } else {
        format!("{}({body}{alpha_part})", space.css())
    }
}

use super::{Channel, Color, Representation};

impl Color {
    /// A colour **in its own space**: the space and its channels travel as one value inside
    /// `Representation::InSpace` (with `none` kept as `None`), and the byte/hsl-axis triple stays empty
    /// because every read of such a colour goes through [`Color::xyz`] and every print through [`print`].
    pub fn in_space(space: Space, channels: [Option<f64>; 3], alpha: f64) -> Self {
        let zero = |value: f64| Channel {
            value,
            percent: false,
        };
        Self {
            channels: [zero(0.0), zero(0.0), zero(0.0)],
            alpha: crate::color::clamp_alpha(alpha),
            representation: Representation::InSpace(InSpace { space, channels }),
            authored: None,
        }
    }

    /// CIE XYZ referenced to D65 — the one route between any two spaces.
    ///
    /// A space colour converts by the table above. A legacy colour reads as srgb: an `hsl`/`hwb` colour first
    /// through the axes it stores, an `rgb`-family one through its bytes, and both are the *encoded* channel
    /// of the srgb transfer curve, which is why `#3366cc` and `color(srgb 0.2 0.4 0.8)` convert to the same
    /// `lab` (`to_space/srgb/lab.hrx::middle`).
    pub fn xyz(&self) -> [f64; 3] {
        let bytes = match self.representation {
            Representation::InSpace(carried) => {
                return to_xyz_d65(
                    carried.space,
                    carried.channels.map(|value| value.unwrap_or(0.0)),
                );
            }
            // Each of these two reads its own axes through the one helper that states how (`stored_axes` for
            // what `hsl` wrote, `hwb_axes` for the derivation a `hwb` colour implies) — a colour that carries
            // a space cannot reach here, so no zero-filled triple is ever read as bytes.
            Representation::Hsl => {
                let [hue, saturation, lightness] = self.stored_axes();
                crate::axes::axes_to_rgb(hue, saturation, lightness)
            }
            Representation::Hwb => {
                let [hue, saturation, lightness] = self.hwb_axes();
                crate::axes::axes_to_rgb(hue, saturation, lightness)
            }
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => self.channels.map(|channel| channel.byte()),
        };
        to_xyz_d65(Space::Srgb, bytes.map(|byte| byte / 255.0))
    }

    /// This colour's channels in `space`, for `color.to-space`. A `none` in every channel stays a `none` in
    /// every channel (`to_space/srgb/lab.hrx::missing/all` answers `lab(none none none)`); a single `none`
    /// contributes zero to the maths and prints as a number, which is what `::missing/red` shows.
    pub fn converted(&self, space: Space) -> Self {
        // **Same space, same numbers.** An identity conversion may neither re-round a channel nor decide which
        // ones were `none` by way of a matrix round trip: `to_space/xyz_d50/xyz_d50.hrx::missing/x` keeps
        // `color(xyz-d50 none 0.2 0.3)` verbatim, and running the triple through XYZ twice is what turned it
        // into invented numbers.
        let carried = match self.representation {
            Representation::InSpace(carried) => Some(carried),
            _ => None,
        };
        if let Some(existing) = carried {
            if existing.space == space {
                return self.clone();
            }
        }
        // **A rectangular/polar pair is one triple, not a conversion.** `oklab` ↔ `oklch` and `lab` ↔ `lch`
        // share every channel and differ only in how the last two are written, so no matrix is involved. The
        // archive shows this directly: `to_space/oklab/oklch.hrx::missing/b` writes
        // `color.to-space(oklab(10% 0.2 none), oklch)` and answers `oklch(10% 0.2 0deg)` — lightness and chroma
        // carried over digit for digit. Sending that through XYZ cannot reproduce it: the round trip leaves the
        // exactly-zero channel at ±1e-17, and a negative residue makes `atan2` fold to `359.999…deg`, which
        // prints as `360deg`. That is how one row went red when the cone table was corrected.
        let polar_pair = |carried: InSpace| {
            let triple = carried.channels.map(|value| value.unwrap_or_default());
            match (carried.space, space) {
                (Space::Oklab, Space::Oklch) | (Space::Lab, Space::Lch) => Some(to_polar(triple)),
                (Space::Oklch, Space::Oklab) | (Space::Lch, Space::Lab) => Some(from_polar(triple)),
                _ => None,
            }
        };
        let channels = match carried.and_then(polar_pair) {
            Some(triple) => triple,
            None => of_xyz_d65(space, self.xyz()),
        };
        // A `none` in every channel is a colour with no power at all, and it stays powerless
        // (`to_space/srgb/lab.hrx::missing/all` answers `lab(none none none)`).
        let powerless =
            carried.is_some_and(|existing| existing.channels.iter().all(Option::is_none));
        Self::in_space(
            space,
            channels.map(|value| (!powerless).then_some(value)),
            self.alpha,
        )
    }

    /// The colour as a **legacy srgb** colour, which is what `color.to-space($c, rgb)` asks for. The form is
    /// the hex one because that is what the corpus prints: `color(srgb 0.2 0.4 0.8)` to `rgb` is `#3366cc`
    /// (`to_space/srgb/rgb.hrx::middle`), while a fractional channel falls back to percentages under the law
    /// already in [`crate::color::text`].
    pub fn as_legacy_rgb(&self) -> Self {
        let bytes = of_xyz_d65(Space::Srgb, self.xyz()).map(|channel| channel * 255.0);
        Self {
            channels: bytes.map(|value| Channel {
                value,
                percent: false,
            }),
            alpha: self.alpha,
            representation: Representation::Hex,
            authored: None,
        }
    }

    /// The colour as a legacy **hsl** colour: `color(srgb 0.2 0.4 0.8)` to `hsl` is `hsl(220, 60%, 50%)`
    /// (`to_space/srgb/hsl.hrx::middle`), the comma form the existing printer already produces.
    pub fn as_legacy_hsl(&self) -> Self {
        let bytes = of_xyz_d65(Space::Srgb, self.xyz()).map(|channel| channel * 255.0);
        let [hue, saturation, lightness] = crate::axes::rgb_to_axes(bytes[0], bytes[1], bytes[2]);
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
            authored: None,
        }
    }
}
