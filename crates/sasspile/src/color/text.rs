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
            // **An out-of-gamut colour prints in the hsl form**, whatever space it was created in:
            // `color.change(black, $red: 500)` → `hsl(0, 5000%, 98.0392156863%)` and
            // `color.change(black, $green: -50)` → `hsl(120, 100%, -9.8039215686%)`
            // (`core_functions/color/change/rgb.hrx::{red,green}/out_of_range`). The bytes stay unrounded on
            // the way in — this is the same law that keeps `grayscale(red)` a percentage row, seen from the
            // other side: rounding or clamping here would answer a different question than the one asked.
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named
                if self
                    .channels
                    .iter()
                    .any(|channel| !(0.0..=255.0).contains(&channel.byte())) =>
            {
                self.hsl_text(self.axes_of())
            }
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
            Representation::Hsl => self.hsl_text(self.axes_of()),
            // A `hwb` colour prints in the hsl form the corpus gives it: `hwb(0deg, 30%, 40%)` →
            // `hsl(0, 33.3333333333%, 45%)` (`hwb/three_args/units::hue/deg`), while the value keeps its
            // whiteness and blackness for `color.channel(.., $space: hwb)` to read back.
            //
            // **A whole-byte `hwb` colour whose bytes name a keyword prints that keyword**: `color.hwb(0, 0%,
            // 0%, 100%)` → `red`, not `hsl(0, 100%, 50%)` (`hwb/four_args::alpha/percent/{max,above_max}`).
            // That is the reverse-name law `Representation::Named` follows, and it is *not* what an `rgb`
            // call does (`rgb(0, 0, 0)` prints itself in six fixtures) — why the two origins differ is not
            // stated anywhere in the corpus, so this is recorded per fixture rather than generalised, and
            // `mix`/`adjust` results keep their own origin instead of inheriting it.
            Representation::Hwb => {
                let axes = self.axes_of();
                let rgb = crate::axes::axes_to_rgb(axes[0], axes[1], axes[2]);
                let whole = rgb.iter().all(|value| value.fract() == 0.0);
                let bytes = rgb.map(|value| value.round() as u32);
                match (self.alpha >= 1.0 && whole)
                    .then_some(bytes)
                    .and_then(Self::keyword)
                {
                    Some(name) => name.to_string(),
                    None => self.hsl_text(axes),
                }
            }
        }
    }

    /// The `hsl`/`hsla` spelling of three axes — shared with [`Representation::Hwb`], which prints as hsl.
    fn hsl_text(&self, [hue, saturation, lightness]: [f64; 3]) -> String {
        let channels = format!(
            "{}, {}%, {}%",
            crate::value::format_number(hue),
            crate::value::format_number(saturation),
            crate::value::format_number(lightness)
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

    /// The legacy functional form: `rgb(r, g, b)` while opaque, `rgba(…)` once there is alpha (which is
    /// what every `four_args/clamped` row prints), and channels in whichever scale this representation
    /// uses. Private to this file: the whole rounding-and-printing law — [`Color::fractional`],
    /// [`Color::percent_form`] and this — moved here from the parent module, so nothing outside it needs to
    /// reach in any more.
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
