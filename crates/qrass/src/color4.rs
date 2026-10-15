//! CSS Color 4 value model (sass-spec `color.*` suites).
//!
//! A `Color4` is a color space plus three (possibly powerless `none`) channel
//! components and an alpha, exactly as dart-sass stores parsed `color(...)` /
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
    Space { name: "hsl", wrapper: false, channels: [ch("hue", Unit::Deg, 0.0, 360.0), ch("saturation", Unit::Pct, 0.0, 100.0), ch("lightness", Unit::Pct, 0.0, 100.0)] },
    Space { name: "hwb", wrapper: false, channels: [ch("hue", Unit::Deg, 0.0, 360.0), ch("whiteness", Unit::Pct, 0.0, 100.0), ch("blackness", Unit::Pct, 0.0, 100.0)] },
    Space { name: "lab", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("a", Unit::Num, -125.0, 125.0), ch("b", Unit::Num, -125.0, 125.0)] },
    Space { name: "lch", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("chroma", Unit::Num, 0.0, 150.0), ch("hue", Unit::Deg, 0.0, 360.0)] },
    Space { name: "oklab", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("a", Unit::Num, -0.4, 0.4), ch("b", Unit::Num, -0.4, 0.4)] },
    Space { name: "oklch", wrapper: false, channels: [ch("lightness", Unit::Pct, 0.0, 100.0), ch("chroma", Unit::Num, 0.0, 0.4), ch("hue", Unit::Deg, 0.0, 360.0)] },
];

/// `xyz-d50` / `xyz-d65` alias onto the `xyz` definition.
pub fn lookup(name: &str) -> Option<&'static Space> {
    match name {
        "xyz-d50" | "xyz-d65" => SPACES.iter().find(|s| s.name == "xyz"),
        other => SPACES.iter().find(|s| s.name == other),
    }
}

/// Color 4 payload attached to a `Color`. Powerless channels (`none`) are
/// `None`; `alpha == None` is the un-written default of 1.0.
#[derive(Clone, Debug)]
pub struct Color4 {
    pub space: &'static Space,
    pub comps: [Option<f64>; 3],
    pub alpha: Option<f64>,
}

impl PartialEq for Color4 {
    fn eq(&self, other: &Self) -> bool {
        self.space.name == other.space.name && self.comps == other.comps && self.alpha == other.alpha
    }
}

impl Color4 {
    pub fn new(space: &'static Space, comps: [f64; 3]) -> Self {
        Self { space, comps: comps.map(Some), alpha: None }
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
            .map(|(v, c)| fmt_comp(*v, c))
            .collect();
        let mut args = parts.join(" ");
        let a = self.alpha();
        if self.alpha.is_some() {
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
        let lin = linear_srgb_from(self, self.raw());
        [
            encode_srgb(lin[0]) * 255.0,
            encode_srgb(lin[1]) * 255.0,
            encode_srgb(lin[2]) * 255.0,
        ]
    }

    fn raw(&self) -> [f64; 3] {
        [c0(self.comps[0]), c0(self.comps[1]), c0(self.comps[2])]
    }
}

fn c0(v: Option<f64>) -> f64 {
    v.unwrap_or(0.0)
}

fn fmt_comp(v: Option<f64>, c: &Channel) -> String {
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

type Mat = [[f64; 3]; 3];

fn mul(m: &Mat, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

const XYZ_TO_LIN_SRGB: Mat = [
    [3.24096995, -1.53738317, -0.49861072],
    [-0.96924320, 1.87596754, 0.04155506],
    [0.05563010, -0.20397695, 1.05697152],
];
const LIN_P3_TO_XYZ: Mat = [
    [0.48657095, 0.26566770, 0.19821729],
    [0.22897456, 0.69173853, 0.07928691],
    [0.00000000, 0.04511371, 1.04394423],
];
const LIN_A98_TO_XYZ: Mat = [
    [0.57666904, 0.18555824, 0.18643497],
    [0.29734498, 0.62736357, 0.07529146],
    [0.02703137, 0.07068875, 0.99133754],
];
const LIN_REC2020_TO_XYZ: Mat = [
    [0.63695805, 0.14461691, 0.16888098],
    [0.26270036, 0.67799808, 0.05930163],
    [0.00000000, 0.02807257, 1.06098506],
];
const LIN_PROPHOTO_TO_XYZ_D50: Mat = [
    [0.79767490, 0.13519171, 0.03135341],
    [0.28804021, 0.71187414, 0.00008566],
    [0.00000000, 0.00000000, 0.82521000],
];
/// Bradford chromatic adaptation, D50 → D65.
const BRADFORD_D50_TO_D65: Mat = [
    [0.9555766, -0.0230393, 0.0631636],
    [-0.0282895, 1.0099416, 0.0210077],
    [0.0122982, -0.0204830, 1.3299098],
];
const WHITE_D50: [f64; 3] = [0.9642956764, 1.0, 0.8251046025];

fn decode_srgb(c: f64) -> f64 {
    let s = c.abs();
    let l = if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
    l * c.signum()
}

fn encode_srgb(c: f64) -> f64 {
    let s = c.abs();
    let v = if s <= 0.0031308 { 12.92 * s } else { 1.055 * s.powf(1.0 / 2.4) - 0.055 };
    v * c.signum()
}

fn decode_rec2020(c: f64) -> f64 {
    let s = c.abs();
    let l = if s <= 0.081 { s / 4.5 } else { ((s + 0.099) / 1.099).powf(1.0 / 0.45) };
    l * c.signum()
}

fn decode_a98(c: f64) -> f64 {
    c.abs().powf(256.0 / 563.0) * c.signum()
}

fn decode_prophoto(c: f64) -> f64 {
    if c.abs() < 1.0 / 16.0 { c / 16.0 } else { c.abs().powf(1.8) * c.signum() }
}

/// Non-linear comps of a gamut space → linear.
fn gamut_to_linear(space: &str, v: [f64; 3]) -> [f64; 3] {
    match space {
        "srgb" | "display-p3" => [decode_srgb(v[0]), decode_srgb(v[1]), decode_srgb(v[2])],
        "srgb-linear" => v,
        "a98-rgb" => [decode_a98(v[0]), decode_a98(v[1]), decode_a98(v[2])],
        "rec2020" => [decode_rec2020(v[0]), decode_rec2020(v[1]), decode_rec2020(v[2])],
        "prophoto-rgb" => [decode_prophoto(v[0]), decode_prophoto(v[1]), decode_prophoto(v[2])],
        _ => v,
    }
}

#[allow(non_snake_case)]
fn hsl_to_lin_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    // dart-sass keeps the HSL round-trip in the non-linear sRGB encoding.
    let col = crate::value::Color::from_hsla(h, s, l, 1.0);
    [
        decode_srgb(col.r / 255.0),
        decode_srgb(col.g / 255.0),
        decode_srgb(col.b / 255.0),
    ]
}

fn hwb_to_lin_rgb(h: f64, w: f64, b: f64) -> [f64; 3] {
    let total = w + b;
    let (w, b) = if total >= 100.0 {
        (w / total * 100.0, b / total * 100.0)
    } else {
        (w, b)
    };
    let s = 100.0 - w - b;
    let l = w + s / 2.0;
    hsl_to_lin_rgb(h, if s == 0.0 { 0.0 } else { s / (100.0 - (2.0 * l - 100.0).abs()) * 100.0 }.min(100.0), l)
}

#[allow(non_snake_case)]
fn lab_to_xyz(L: f64, a: f64, b: f64) -> [f64; 3] {
    const K: f64 = 6.0 / 29.0;
    let fy = (L + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let finv = |t: f64| {
        if t > K {
            t * t * t
        } else {
            3.0 * K * K * (t - 4.0 * K)
        }
    };
    [
        WHITE_D50[0] * finv(fx),
        WHITE_D50[1] * finv(fy),
        WHITE_D50[2] * finv(fz),
    ]
}

#[allow(non_snake_case)]
fn oklab_to_xyz(L: f64, a: f64, b: f64) -> [f64; 3] {
    let l_ = L + 0.3963377794 * a + 0.2158037573 * b;
    let m_ = L - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = L - 0.0894841775 * a - 1.2914855480 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.0767416621 * l - 1.2684380046 * m - 0.0041960863 * s,
        -1.2686807231 * l + 2.6093000011 * m - 0.3413193967 * s,
        -0.0041960863 * l - 0.7034160958 * m + 1.7076693116 * s,
    ]
}

/// Convert a Color 4 value's (possibly powerless) components to linear sRGB.
fn linear_srgb_from(c4: &Color4, raw: [f64; 3]) -> [f64; 3] {
    let name = c4.space.name;
    match name {
        "srgb" | "srgb-linear" | "display-p3" | "a98-rgb" | "rec2020" | "prophoto-rgb" => {
            let lin = gamut_to_linear(name, raw);
            let xyz = match name {
                "srgb" | "srgb-linear" => mul(&LIN_SRGB_TO_XYZ, lin),
                "display-p3" => mul(&LIN_P3_TO_XYZ, lin),
                "a98-rgb" => mul(&LIN_A98_TO_XYZ, lin),
                "rec2020" => mul(&LIN_REC2020_TO_XYZ, lin),
                _ => mul(&BRADFORD_D50_TO_D65, mul(&LIN_PROPHOTO_TO_XYZ_D50, lin)),
            };
            mul(&XYZ_TO_LIN_SRGB, xyz)
        }
        "xyz" => mul(&XYZ_TO_LIN_SRGB, raw),
        "hsl" => hsl_to_lin_rgb(raw[0], raw[1], raw[2]),
        "hwb" => hwb_to_lin_rgb(raw[0], raw[1], raw[2]),
        "lab" => mul(&XYZ_TO_LIN_SRGB, mul(&BRADFORD_D50_TO_D65, lab_to_xyz(raw[0], raw[1], raw[2]))),
        "oklab" => mul(&XYZ_TO_LIN_SRGB, oklab_to_xyz(raw[0], raw[1], raw[2])),
        "lch" => {
            let h = raw[2].to_radians();
            lab_to_xyz_into(c4, raw[0], raw[1] * h.cos(), raw[1] * h.sin())
        }
        "oklch" => {
            let h = raw[2].to_radians();
            mul(&XYZ_TO_LIN_SRGB, oklab_to_xyz(raw[0], raw[1] * h.cos(), raw[1] * h.sin()))
        }
        _ => [0.0, 0.0, 0.0],
    }
}

#[allow(non_snake_case)]
fn lab_to_xyz_into(_c4: &Color4, L: f64, a: f64, b: f64) -> [f64; 3] {
    mul(&XYZ_TO_LIN_SRGB, mul(&BRADFORD_D50_TO_D65, lab_to_xyz(L, a, b)))
}

const LIN_SRGB_TO_XYZ: Mat = [
    [0.41239080, 0.35758434, 0.18048079],
    [0.21263901, 0.71516871, 0.07219231],
    [0.01933083, 0.11919481, 0.95053216],
];
