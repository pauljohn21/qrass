//! How a [`Color`] becomes CSS text — the representation law, kept apart from the data and the axis
//! arithmetic in [`super`] because it is the half that the corpus contradicts most often.
//!
//! The rules, each with its fixture:
//! * a colour keeps the form it was created in — unitless channels `rgb(18, 52, 86)`, a percentage
//!   anywhere `rgb(7.1%, 20.4%, 33.9%)`, hex `#112233`, hsl `hsl(60, 100%, 50%)`;
//! * **rounding belongs to printing, not to the conversion**: a colour whose channels are not whole bytes
//!   prints percentages (`color.grayscale(#633736)` → `rgb(30%, 30%, 30%)`), a whole-byte one prints hex or
//!   numbers (`color.grayscale(#494949)` → `#494949`, `darken(rgba(red, 0.2), 100%)` → `rgba(0, 0, 0, 0.2)`);
//! * hex-created colours print hex while opaque and take the functional form once translucent
//!   (`rgb(#123, 0.5)` → `rgba(17, 34, 51, 0.5)`), and no eight-digit hex is printed;
//! * percentages are written with Sass's number form (`67.8431372549%` from `173`).

use super::{Channel, Color, Representation};

/// Every CSS colour keyword, **as spellings only**.
///
/// No RGB value is written here: each spelling is resolved through `color::parse_color`, so a typo in this
/// list shows up as a name that does not parse (pinned by `tests/color.rs`) rather than as a wrong colour,
/// and a missing spelling degrades to printing hex — the safe direction. The list is what the reverse
/// lookup needs, because the corpus distinguishes colours by **how they were created**:
/// * `lighten(red, 0%)` → `red`, `grayscale(white)` → `white`, `desaturate(plum, 0%)` → `plum`;
/// * `complement(red)` → `aqua` (a keyword-origin colour moved onto another keyword's bytes takes that
///   name), while `complement(turquoise)` → `#e04050` (moved onto bytes that name nothing) prints hex;
/// * and a colour built from `rgb(0, 0, 0)` prints `rgb(0, 0, 0)` — **not** `black` — in six fixtures under
///   `core_functions/color/rgb/**`, which is what rules out a value-only lookup for every colour.
const NAMES: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

impl Color {
    /// The keyword a byte triple stands for, resolved through the same dependency that supplies the
    /// numbers. Not cached: a lookup table would be shared mutable state, and a colour is printed a handful
    /// of times per compilation, so the scan is the cheaper discipline.
    fn keyword(bytes: [u32; 3]) -> Option<&'static str> {
        NAMES.iter().copied().find(|name| {
            color::parse_color(name)
                .map(|colour| {
                    let rgb = colour.to_alpha_color::<color::Srgb>().to_rgba8();
                    [u32::from(rgb.r), u32::from(rgb.g), u32::from(rgb.b)] == bytes
                })
                .unwrap_or(false)
        })
    }

    /// The CSS text.
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
            // A colour created from a keyword: the keyword when it still names one, otherwise the same
            // fall-backs a hex colour has (`complement(turquoise)` → `#e04050`, `desaturate(plum, 100%)` →
            // percentages, `rgb(transparent, 0.5)` → `rgba(0, 0, 0, 0.5)`).
            Representation::Named => {
                if self.fractional() {
                    return self.percent_form();
                }
                let bytes = self.channels.map(|channel| channel.byte().round() as u32);
                if self.alpha < 1.0 {
                    return self.legacy(self.channels, false);
                }
                match Self::keyword(bytes) {
                    Some(name) => name.to_string(),
                    None => format!("#{}{}{}", byte(bytes[0]), byte(bytes[1]), byte(bytes[2])),
                }
            }
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
    /// uses. `pub(super)` because [`super::Color::percent_form`] reaches for it from the parent module.
    pub(super) fn legacy(&self, printed: [Channel; 3], percent: bool) -> String {
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
