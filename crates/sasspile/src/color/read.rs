//! Reading **one channel** out of a colour — the accessors and `color.channel`. Split out of [`super`]
//! because it is the half where the space matters: the same colour answers `171` in `rgb`,
//! `0.7529411765` in `srgb` and `349.5238095238deg` in `hsl`/`hwb`, and which of those a call may ask for is
//! its own law (`channel/*.hrx`), separate from how a colour is built or printed.

use super::{Channel, Color, Representation};
use crate::value::Value;

impl Color {
    /// One `0–255` channel, for the accessors. A colour held in **another space** returns `None` rather
    /// than a wrong number.
    ///
    /// Nothing in the corpus contradicts that caution, and the measurement says it is not a gap I should
    /// fill by inference: `grep`ing every `.hrx` under `spec/` for `color.(red|green|blue)(…hsl(…))` finds
    /// **no row at all** — the hsl-space fixtures read the hsl axes (`channel/hsl.hrx`), which
    /// [`Self::hsl_channel`] serves. `hsl` → `rgb` therefore has no golden to be written against, while
    /// the reverse direction does and is implemented.
    pub fn byte(&self, index: usize) -> Option<f64> {
        match self.representation {
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => self
                .channels
                .get(index)
                .map(|channel: &Channel| channel.byte().round()),
            Representation::Hsl | Representation::Hwb => None,
        }
    }

    /// The colour's own alpha: `color.alpha(rgba(red, 0.42))` → `0.42` (`alpha.hrx::color/middle`).
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// One channel, named the way `color.channel` names it, read in a space.
    ///
    /// `space` is the call's `$space:` as written (`hsl`, `rgb`, `srgb`); `None` means **the colour's own
    /// space**, which is all the corpus pins without a label: `color.channel(#abcdef, "red")` → `171` and
    /// `color.channel(hsl(10 40% 60%), "hue")` → `10deg` (`channel/rgb.hrx::local/*`,
    /// `channel/hsl.hrx::local/*`). A cross-space read is answered only when the label says so
    /// (`channel/hsl.hrx::foreign/hue` is `color.channel(pink, "hue", $space: hsl)` → `349.5238095238deg`);
    /// converting on the colour's behalf without one would invent a rule no fixture states.
    ///
    /// Units come from the corpus too: an `rgb`/`srgb` channel is a bare number (`171`, and
    /// `color.channel(pink, "green", $space: srgb)` → `0.7529411765` — the byte over 255), the hue carries
    /// `deg`, saturation and lightness carry `%`.
    pub fn channel(&self, name: &str, space: Option<&str>) -> Option<Value> {
        let number = |value: f64| Value::Number(value, crate::unit::Unit::unitless());
        let percent = |value: f64| Value::Number(value, crate::unit::Unit::named("%"));
        let rgb_space = matches!(
            self.representation,
            Representation::Legacy
                | Representation::Percent
                | Representation::Hex
                | Representation::Named
        );
        match name {
            "red" | "green" | "blue" => {
                let index = match name {
                    "red" => 0,
                    "green" => 1,
                    _ => 2,
                };
                match space {
                    Some("srgb") => Some(number(self.byte(index)? / 255.0)),
                    Some("rgb") => Some(number(self.byte(index)?)),
                    None if rgb_space => Some(number(self.byte(index)?)),
                    _ => None,
                }
            }
            // The hue is the same number in `hsl` and in `hwb` (`channel/hwb.hrx::foreign/hue` reads
            // `349.5238095238deg` off `pink`), and [`Self::axes_of`] derives it for every space stored.
            "hue" => match space {
                Some("hsl") | Some("hwb") => Some(Value::Number(
                    self.hsl_channel(0)?,
                    crate::unit::Unit::named("deg"),
                )),
                None if matches!(
                    self.representation,
                    Representation::Hsl | Representation::Hwb
                ) =>
                {
                    Some(Value::Number(
                        self.hsl_channel(0)?,
                        crate::unit::Unit::named("deg"),
                    ))
                }
                _ => None,
            },
            "saturation" | "lightness" => {
                let axis = usize::from(name == "lightness") + 1;
                match space == Some("hsl")
                    || (space.is_none() && self.representation == Representation::Hsl)
                {
                    true => self.hsl_channel(axis).map(percent),
                    false => None,
                }
            }
            // Whiteness and blackness exist only in `hwb`: a `hwb`-created colour stores them, and an
            // `rgb`-family colour answers the label through [`crate::axes::rgb_to_hwb`]
            // (`color.channel(pink, "whiteness", $space: hwb)` → `75.2941176471%`, `…"blackness"…` → `0%`,
            // `channel/hwb.hrx::foreign/*`).
            "whiteness" | "blackness" => {
                let index = usize::from(name == "blackness");
                let own = self.representation == Representation::Hwb;
                let read = if own {
                    self.channels[index + 1].value
                } else if rgb_space {
                    let [red, green, blue] = self.channels.map(|channel| channel.byte());
                    crate::axes::rgb_to_hwb(red, green, blue)[index]
                } else {
                    return None;
                };
                (own && space.is_none() || space == Some("hwb")).then(|| percent(read))
            }
            // Every other space (`lab`, `lch`, `oklab`, `oklch`, the device spaces) needs a conversion this
            // compiler has not implemented.
            _ => None,
        }
    }

    /// The colour's `[hue, whiteness, blackness]`, in the form [`Color::with_hwb`] takes back:
    /// stored for a `hwb` colour, derived from the bytes otherwise (`color.channel(pink, "whiteness",
    /// $space: hwb)` → `75.2941176471%` reads the same derivation, `channel/hwb.hrx::foreign/*`).
    /// `None` for an `hsl`-created colour, whose whiteness and blackness no fixture ever asks for.
    pub fn hwb_of(&self) -> Option<[f64; 3]> {
        let hue = self.hue()?;
        match self.representation {
            Representation::Hwb => Some([hue, self.channels[1].value, self.channels[2].value]),
            _ if self.writes_bytes() => {
                let [red, green, blue] = self.channels.map(|channel| channel.byte());
                let [white, black] = crate::axes::rgb_to_hwb(red, green, blue);
                Some([hue, white, black])
            }
            _ => None,
        }
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
    pub(super) fn hsl_channel(&self, index: usize) -> Option<f64> {
        self.axes_of().get(index).copied()
    }
}
