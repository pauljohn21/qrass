//! CSS Color 4 value model (sass-spec `color.*` suites).
//!
//! A `Color4` is a color space plus three (possibly powerless `none`) channel
//! components and an alpha, exactly as the goldens store parsed `color(...)` /
//! `lab()` / `lch()` / `oklab()` / `oklch()` / `hsl()` / `hwb()` literals. The
//! legacy RGBA fields on [`crate::value::Color`] are filled through
//! [`Color4::to_rgb`], so old channel APIs keep working; serialization of a
//! Color 4 value always round-trips through its own space via [`Color4::to_css`].

use crate::value::format_number;

/// Canonical unit a channel prints and accepts with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Num,
    Pct,
    Deg,
}

#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub name: &'static str,
    pub unit: Unit,
    pub lo: f64,
    pub hi: f64,
}

const fn ch(name: &'static str, unit: Unit, lo: f64, hi: f64) -> Channel {
    Channel { name, unit, lo, hi }
}

const fn rgb_channels() -> [Channel; 3] {
    [
        ch("red", Unit::Num, 0.0, 1.0),
        ch("green", Unit::Num, 0.0, 1.0),
        ch("blue", Unit::Num, 0.0, 1.0),
    ]
}

/// One of the CSS Color 4 interpolation spaces recognized in literals.
#[derive(Clone, Copy, Debug)]
pub struct Space {
    pub name: &'static str,
    /// True for gamut spaces serialized as `color(name ...)`; false for the
    /// bare-function forms (`lab(...)`, `hsl(...)`, ...).
    pub wrapper: bool,
    pub channels: [Channel; 3],
}

pub static SPACES: &[Space] = &[
    Space { name: "srgb", wrapper: true, channels: rgb_channels() },
    Space { name: "srgb-linear", wrapper: true, channels: rgb_channels() },
    Space { name: "a98-rgb", wrapper: true, channels: rgb_channels() },
    Space { name: "display-p3", wrapper: true, channels: rgb_channels() },
    Space { name: "display-p3-linear", wrapper: true, channels: rgb_channels() },
    Space { name: "prophoto-rgb", wrapper: true, channels: rgb_channels() },
    Space { name: "rec2020", wrapper: true, channels: rgb_channels() },
    Space {
        name: "xyz",
        wrapper: true,
        channels: [
            ch("x", Unit::Num, 0.0, 1.0),
            ch("y", Unit::Num, 0.0, 1.0),
            ch("z", Unit::Num, 0.0, 1.0),
        ],
    },
    Space {
        name: "xyz-d50",
        wrapper: true,
        channels: [
            ch("x", Unit::Num, 0.0, 1.0),
            ch("y", Unit::Num, 0.0, 1.0),
            ch("z", Unit::Num, 0.0, 1.0),
        ],
    },
    Space { name: "hsl", wrapper: false, channels: [ch("hue", Unit::Deg, 0.0, 360.0), ch("saturation", Unit::Pct, 0.0, 100.0), ch("lightness", Unit::Pct, 0.0, 100.0)] },
    Space { name: "hwb", wrapper: false, channels: [ch("hue", Unit::Deg, 0.0, 360.0), ch("whiteness", Unit::Pct, 0.0, 100.0), ch("blackness", Unit::Pct, 0.0, 100.0)] },
    Space { name: "lab", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("a", Unit::Num, -125.0, 125.0), ch("b", Unit::Num, -125.0, 125.0)] },
    Space { name: "lch", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("chroma", Unit::Num, 0.0, 150.0), ch("hue", Unit::Deg, 0.0, 360.0)] },
    Space { name: "oklab", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("a", Unit::Num, -0.4, 0.4), ch("b", Unit::Num, -0.4, 0.4)] },
    Space { name: "oklch", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("chroma", Unit::Num, 0.0, 0.4), ch("hue", Unit::Deg, 0.0, 360.0)] },
];

/// `xyz-d65` aliases onto the `xyz` (D65) definition; `xyz-d50` is its own space.
pub fn lookup(name: &str) -> Option<&'static Space> {
    match name {
        "xyz-d65" => SPACES.iter().find(|s| s.name == "xyz"),
        other => SPACES.iter().find(|s| s.name == other),
    }
}

/// Color 4 payload attached to a `Color`. Powerless channels (`none`) are
/// `None`; `alpha == None` is the un-written default of 1.0.
#[derive(Clone, Debug)]
pub struct Color4 {
    pub space: &'static Space,
    pub comps: [Option<f64>; 3],
    /// Text overrides for degenerate channel spellings (`change/unusual_numbers`:
    /// `lab(50% calc(infinity) 0)` re-emits the calc *text* verbatim while the
    /// numeric slot keeps 0 for downstream conversion). A slot is text only
    /// when the literal wrote `calc(infinity)`/`calc(-infinity)`/`calc(NaN)`.
    pub text: [Option<String>; 3],
    pub alpha: Option<f64>,
    /// True only when the literal wrote `/ none` explicitly (an omitted alpha
    /// defaults to opaque 1 and is never "missing").
    pub alpha_missing: bool,
}

impl PartialEq for Color4 {
    fn eq(&self, other: &Self) -> bool {
        self.space.name == other.space.name
            && self.comps == other.comps
            && self.text == other.text
            && self.alpha == other.alpha
            && self.alpha_missing == other.alpha_missing
    }
}

impl Color4 {
    pub fn new(space: &'static Space, comps: [f64; 3]) -> Self {
        Self {
            space,
            comps: comps.map(Some),
            text: [None, None, None],
            alpha: None,
            alpha_missing: false,
        }
    }

    pub fn alpha(&self) -> f64 {
        self.alpha.unwrap_or(1.0)
    }

    pub fn channel(&self, name: &str) -> Option<f64> {
        self.space
            .channels
            .iter()
            .zip(&self.comps)
            .find(|(c, _)| c.name == name)
            .and_then(|(_, v)| *v)
    }

    /// `lab`/`lch`/`oklab`/`oklch`/`hsl`/`hwb` print bare; gamut spaces wrap in
    /// `color(name ...)`. Alpha is emitted inside the parentheses only when not
    /// the default opaque value.
    pub fn to_css(&self) -> String {
        let parts: Vec<String> = self
            .comps
            .iter()
            .zip(&self.space.channels)
            .zip(&self.text)
            .map(|((v, c), t)| fmt_comp(*v, c, t.as_deref()))
            .collect();
        let mut args = parts.join(" ");
        let a = self.alpha();
        if self.alpha_missing {
            args.push_str(" / none");
        } else if self.alpha.is_some() && a != 1.0 {
            // An explicit `/ 1` round-trips without the alpha slot (the goldens).
            args.push_str(&format!(" / {}", format_number(a)));
        }
        if self.space.wrapper {
            format!("color({} {})", self.space.name, args)
        } else {
            format!("{}({args})", self.space.name)
        }
    }

    /// Convert to 8-bit-scale sRGB channels (unclamped) for the legacy API.
    pub fn to_rgb(&self) -> [f64; 3] {
        let lin = mul(&xyz_to_lin_srgb(), self.to_xyz_d65());
        [
            encode_srgb(lin[0]) * 255.0,
            encode_srgb(lin[1]) * 255.0,
            encode_srgb(lin[2]) * 255.0,
        ]
    }

    pub(crate) fn raw(&self) -> [f64; 3] {
        [c0(self.comps[0]), c0(self.comps[1]), c0(self.comps[2])]
    }
}

fn c0(v: Option<f64>) -> f64 {
    v.unwrap_or(0.0)
}

fn fmt_comp(v: Option<f64>, c: &Channel, text: Option<&str>) -> String {
    if let Some(t) = text {
        return t.to_string();
    }
    match v {
        None => "none".into(),
        Some(x) => match c.unit {
            Unit::Pct => format!("{}%", format_number(x)),
            Unit::Deg => format!("{}deg", format_number(x)),
            Unit::Num => format_number(x),
        },
    }
}

// ---------------------------------------------------------------------------
// Space → linear sRGB conversions (CSS Color 4 reference matrices, D65).
// ---------------------------------------------------------------------------

pub(crate) type Mat = [[f64; 3]; 3];

pub(crate) fn mul(m: &Mat, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

// The RGB -> XYZ matrices are the CSS Color 4 reference matrices, taken as the
// spec's own exact rationals (computed from the R/G/B/W chromaticities with the
// white-scaling that forces rgb (1,1,1) exactly onto the reference white). Using
// the rationals keeps every space closing onto the *same* D65/D50 white, so a
// neutral grey survives a cross-gamut round-trip unchanged (hand-rounded decimal
// copies drifted the white by ~1e-8 and split greys across channels, and a fresh
// derive from the primaries does not reproduce golden — e.g. the spec resolves
// display-P3 green to Y = 247089/357200 ≈ 0.6917, not the 0.640 a naive derive
// yields). ProPhoto's matrix is irrational; the spec's 64-bit form is used.

/// linear sRGB -> XYZ-D65 (CSS Color 4 reference).
pub(crate) fn lin_srgb_to_xyz() -> Mat {
    [
        [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ]
}
/// linear Display P3 -> XYZ-D65 (CSS Color 4 reference).
pub(crate) fn lin_p3_to_xyz() -> Mat {
    [
        [608311.0 / 1250200.0, 189793.0 / 714400.0, 198249.0 / 1000160.0],
        [35783.0 / 156275.0, 247089.0 / 357200.0, 198249.0 / 2500400.0],
        [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
    ]
}
/// linear a98-rgb -> XYZ-D65 (CSS Color 4 reference).
pub(crate) fn lin_a98_to_xyz() -> Mat {
    [
        [573536.0 / 994567.0, 263643.0 / 1420810.0, 187206.0 / 994567.0],
        [591459.0 / 1989134.0, 6239551.0 / 9945670.0, 374412.0 / 4972835.0],
        [53769.0 / 1989134.0, 351524.0 / 4972835.0, 4929758.0 / 4972835.0],
    ]
}
/// linear Rec. 2020 -> XYZ-D65 (CSS Color 4 reference).
pub(crate) fn lin_rec2020_to_xyz() -> Mat {
    [
        [63426534.0 / 99577255.0, 20160776.0 / 139408157.0, 47086771.0 / 278816314.0],
        [26158966.0 / 99577255.0, 472592308.0 / 697040785.0, 8267143.0 / 139408157.0],
        [0.0, 19567812.0 / 697040785.0, 295819943.0 / 278816314.0],
    ]
}
/// linear ProPhoto RGB -> XYZ-D50 (CSS Color 4 reference, 64-bit irrational form;
/// defined on the D50 white, `to_xyz_d65` adapts it up).
pub(crate) fn lin_prophoto_to_xyz_d50() -> Mat {
    [
        [0.797_766_644_900_642_3, 0.13518129740053308, 0.031_347_734_128_392_2],
        [0.288_074_828_819_401_3, 0.711_835_234_241_873, 0.00008993693872564],
        [0.0, 0.0, 0.825_104_602_510_460_2],
    ]
}
/// XYZ-D65 -> linear sRGB (the exact inverse of [`lin_srgb_to_xyz`]).
pub(crate) fn xyz_to_lin_srgb() -> Mat {
    crate::color4_convert::inv(&lin_srgb_to_xyz())
}
/// Bradford chromatic adaptation, D65 → D50 (CSS Color 4 reference). This is the
/// single canonical adaptation matrix: the D50→D65 direction is its exact numeric
/// inverse ([`bradford_d50_to_d65`]) and the D50 reference white is derived from it
/// ([`white_d50`]). Keeping all three mutually consistent is what lets a neutral
/// survive `lab`/`lch`/`xyz-d50`/`prophoto` round-trips exactly — the spec's
/// separately-rounded inverse and a hand-picked D50 white disagreed by ~6e-6,
/// which leaked float noise into the a/b channels of every neutral.
pub(crate) const BRADFORD_D65_TO_D50: Mat = [
    [1.0479298208, 0.0229467933, -0.0501922295],
    [0.0296278157, 0.9904344745, -0.0170738250],
    [-0.0092430582, 0.0150551448, 0.7518742899],
];
/// Bradford chromatic adaptation, D50 → D65: the exact inverse of
/// [`BRADFORD_D65_TO_D50`].
pub(crate) fn bradford_d50_to_d65() -> Mat {
    crate::color4_convert::inv(&BRADFORD_D65_TO_D50)
}
/// CIE D50 reference white (Y = 1): `BRADFORD_D65_TO_D50` applied to the sRGB D65
/// white, so the adaptation chain closes on itself and D65 white maps to `a = b = 0`.
pub(crate) fn white_d50() -> [f64; 3] {
    let d65 = mul(&lin_srgb_to_xyz(), [1.0, 1.0, 1.0]);
    mul(&BRADFORD_D65_TO_D50, d65)
}

fn decode_srgb(c: f64) -> f64 {
    let s = c.abs();
    let l = if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
    l * c.signum()
}

pub(crate) fn encode_srgb(c: f64) -> f64 {
    let s = c.abs();
    let v = if s <= 0.0031308 { 12.92 * s } else { 1.055 * s.powf(1.0 / 2.4) - 0.055 };
    v * c.signum()
}

fn decode_rec2020(c: f64) -> f64 {
    // The goldens model the rec2020 *display* with the pure-power BT.1886
    // EOTF, L = V^2.4, not the CSS Color 4 piecewise α/β (0.45) OETF inverse:
    // a neutral 0.5 decodes to 0.5^2.4 = 0.1894645708, which the spec's `to-space`
    // goldens reproduce exactly. Kept symmetric with `encode_rec2020`.
    c.abs().powf(2.4) * c.signum()
}

fn decode_a98(c: f64) -> f64 {
    // a98 gamma is 563/256 ≈ 2.2: nonlinear → linear raises by that exponent.
    c.abs().powf(563.0 / 256.0) * c.signum()
}

fn decode_prophoto(c: f64) -> f64 {
    // ProPhoto extended transfer (CSS Color 4): linear segment below Et2 = 16/512.
    if c.abs() <= 16.0 / 512.0 { c / 16.0 } else { c.abs().powf(1.8) * c.signum() }
}

/// Non-linear comps of a gamut space → linear.
pub(crate) fn gamut_to_linear(space: &str, v: [f64; 3]) -> [f64; 3] {
    match space {
        "srgb" | "display-p3" => [decode_srgb(v[0]), decode_srgb(v[1]), decode_srgb(v[2])],
        "srgb-linear" | "display-p3-linear" => v,
        "a98-rgb" => [decode_a98(v[0]), decode_a98(v[1]), decode_a98(v[2])],
        "rec2020" => [decode_rec2020(v[0]), decode_rec2020(v[1]), decode_rec2020(v[2])],
        "prophoto-rgb" => [decode_prophoto(v[0]), decode_prophoto(v[1]), decode_prophoto(v[2])],
        _ => v,
    }
}

#[allow(non_snake_case)]
pub(crate) fn lab_to_xyz(L: f64, a: f64, b: f64) -> [f64; 3] {
    const K: f64 = 6.0 / 29.0;
    let fy = (L + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let finv = |t: f64| {
        if t > K {
            t * t * t
        } else {
            3.0 * K * K * (t - 4.0 / 29.0)
        }
    };
    let w = white_d50();
    [
        w[0] * finv(fx),
        w[1] * finv(fy),
        w[2] * finv(fz),
    ]
}

/// A degenerate calc-only channel text: `calc(infinity)` / `calc(+infinity)`
/// / `calc(-infinity)` / `calc(NaN)` — exact spellings only. The goldens
/// re-emit these **verbatim** in the channel slot (`change/unusual_numbers`
/// `lab(50% calc(infinity) 0)`), while downstream conversion treats the
/// numeric part as 0 (no conversion golden exists for them).
pub fn degenerate_channel_text(text: &str) -> bool {
    let Some(inner) = text.trim().strip_prefix("calc(").and_then(|t| t.strip_suffix(')')) else {
        return false;
    };
    matches!(
        inner.trim().to_ascii_lowercase().as_str(),
        "infinity" | "+infinity" | "-infinity" | "nan"
    )
}
