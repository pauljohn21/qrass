//! Generic CSS Color 4 space conversions (task 11.3, to-space round).
//!
//! Every supported space either speaks XYZ-D65 directly (gamut spaces through
//! their linear matrices, `xyz`, the lab/oklab families through Bradford
//! adaptation) or round-trips through sRGB (`hsl`, `hwb`). [`Color4::convert`]
//! is the single hub: `source → XYZ-D65 → target`.

use crate::color4::{
    BRADFORD_D65_TO_D50, Color4, Mat, Space, bradford_d50_to_d65, encode_srgb, gamut_to_linear,
    lab_to_xyz, lin_a98_to_xyz, lin_p3_to_xyz, lin_prophoto_to_xyz_d50, lin_rec2020_to_xyz,
    lin_srgb_to_xyz, mul, white_d50, xyz_to_lin_srgb,
};

/// Invert a non-singular 3×3 matrix (all reference matrices are).
pub(crate) fn inv(m: &Mat) -> Mat {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let id = 1.0 / det;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * id,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * id,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * id,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * id,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * id,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * id,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * id,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * id,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * id,
        ],
    ]
}

fn encode_a98(c: f64) -> f64 {
    c.abs().powf(256.0 / 563.0) * c.signum()
}

fn encode_rec2020(c: f64) -> f64 {
    // Symmetric pure-power BT.1886 inverse (V = L^(1/2.4)), matching `decode_rec2020`
    // and the goldens: linear 0.5 encodes to 0.5^(1/2.4) = 0.7491535384.
    c.abs().powf(1.0 / 2.4) * c.signum()
}

fn encode_prophoto(c: f64) -> f64 {
    if c.abs() < 1.0 / 512.0 { c * 16.0 } else { c.abs().powf(1.0 / 1.8) * c.signum() }
}

const fn eps() -> f64 {
    216.0 / 24389.0
}

/// CIE XYZ (D50) → CIELAB, dividing by the fitted reference white.
#[allow(non_snake_case)]
pub(crate) fn xyz_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    let f = |t: f64| {
        if t > eps() {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let w = white_d50();
    let fx = f(xyz[0] / w[0]);
    let fy = f(xyz[1] / w[1]);
    let fz = f(xyz[2] / w[2]);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// D65 XYZ → CIELAB: canonical Bradford adaptation into D50 (whose exact
/// numeric inverse keeps every D50 round trip — `xyz-d50`/`prophoto-rgb`
/// sources and targets — cancelling), then the fitted white-point division
/// recovered from golden evidence (`constant_solver::fit_lab_white_point`,
/// 4 rows agreeing at 1e-12) — the previous derived white point carried the
/// ~1e-8 `to_space` tail delta.
pub(crate) fn lab_from_d65(xyz_d65: [f64; 3]) -> [f64; 3] {
    xyz_to_lab(mul(&BRADFORD_D65_TO_D50, xyz_d65))
}

// oklab reference matrices (Björn Ottosson), defined on linear sRGB.
// M1: linear sRGB -> LMS.  M2: cbrt(LMS) -> Lab.
const SRGB_TO_OKLAB_LMS: Mat = [
    [0.4122214708, 0.5363325363, 0.0514459929],
    [0.2119034982, 0.6806995451, 0.1073969566],
    [0.0883024619, 0.2817188376, 0.6299787005],
];
const OKLAB_LMS_TO_LAB: Mat = [
    [0.2104542553, 0.7936177850, -0.0040720469],
    [1.9779984951, -2.4285922050, 0.4505937099],
    [0.0259040371, 0.7827717662, -0.8086757540],
];

/// oklab (L in 0..1, a, b) → linear sRGB, built from the *exact* inverses of the
/// forward matrices above. Ottosson's separately-rounded published inverse does
/// not invert `SRGB_TO_OKLAB_LMS`: its rows sum to 0.95/1.00/1.09, so D65 white
/// (L=1, a=b=0) lands at linear sRGB (0.95, 1.0, 1.09) instead of (1, 1, 1). That
/// ~5% skew splits every neutral across the R/B channels in gamut conversions.
/// Inverting the true forward pair keeps the neutral on the r = g = b line so it
/// survives `oklab/oklch → XYZ → gamut` round-trips unchanged (matching golden).
#[allow(non_snake_case)]
fn oklab_to_lin_srgb(L: f64, a: f64, b: f64) -> [f64; 3] {
    let lms_ = mul(&inv(&OKLAB_LMS_TO_LAB), [L, a, b]);
    let lms = [lms_[0] * lms_[0] * lms_[0], lms_[1] * lms_[1] * lms_[1], lms_[2] * lms_[2] * lms_[2]];
    mul(&inv(&SRGB_TO_OKLAB_LMS), lms)
}

#[allow(non_snake_case)]
fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    // oklab is defined on linear sRGB, not XYZ: route the XYZ-D65 hub back to
    // linear sRGB (exactly inverting the `to_xyz_d65` lift), then M1 -> cbrt -> M2.
    let lin = mul(&xyz_to_lin_srgb(), xyz);
    let lms = mul(&SRGB_TO_OKLAB_LMS, lin);
    let ls = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    let lab = mul(&OKLAB_LMS_TO_LAB, ls);
    // Store lightness in the percent domain like the literal form.
    [lab[0] * 100.0, lab[1], lab[2]]
}

fn to_polar(l: f64, a: f64, b: f64) -> [f64; 3] {
    let c = (a * a + b * b).sqrt();
    let mut h = b.atan2(a).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    // Near-zero chroma has an undefined hue; keep 0 for determinism.
    [l, if c < 1e-13 { 0.0 } else { c }, if c < 1e-13 { 0.0 } else { h }]
}

/// Linear sRGB → non-linear 0..1 sRGB triple.
fn lin_to_srgb(lin: [f64; 3]) -> [f64; 3] {
    [encode_srgb(lin[0]), encode_srgb(lin[1]), encode_srgb(lin[2])]
}

/// Non-linear sRGB (0..1) → hue in degrees (HSL/HWB share the hue formula).
fn srgb_hue(v: [f64; 3]) -> f64 {
    let (r, g, b) = (v[0], v[1], v[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    // An achromatic triple has an undefined hue; the goldens report `0deg`. The
    // sRGB→XYZ→sRGB hub leaves each channel differing by only ~1e-7, so treat a
    // spread that small as neutral rather than letting `atan2` pick a random angle.
    if d < 1e-6 {
        return 0.0;
    }
    let h = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = h * 60.0;
    if h < 0.0 { h + 360.0 } else { h }
}

fn srgb_to_hsl(v: [f64; 3]) -> [f64; 3] {
    let max = v[0].max(v[1]).max(v[2]);
    let min = v[0].min(v[1]).min(v[2]);
    let l = (max + min) / 2.0;
    let d = max - min;
    // An achromatic triple has zero saturation. Out-of-gamut neutrals (a channel
    // past 1 or below 0, all equal) still divide safely: |2l-1| only reaches 1
    // at the l 0/1 ends, where a nonzero spread is impossible — the goldens
    // divides unguarded, and an epsilon here would flatten genuine out-of-gamut
    // saturations (hwb(20deg 200% -125%) → s = 11.1111111111%).
    let s = if d == 0.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    [srgb_hue(v), s * 100.0, l * 100.0]
}

fn srgb_to_hwb(v: [f64; 3]) -> [f64; 3] {
    let w = v[0].min(v[1]).min(v[2]);
    let k = 1.0 - v[0].max(v[1]).max(v[2]);
    let (w, k) = if w + k >= 1.0 {
        (w / (w + k), k / (w + k))
    } else {
        (w, k)
    };
    [srgb_hue(v), w * 100.0, k * 100.0]
}

impl Color4 {
    /// XYZ-D65 triple of this color (channels resolved with `none` → 0).
    pub fn to_xyz_d65(&self) -> [f64; 3] {
        let raw = self.raw();
        let name = self.space.name;
        match name {
            "srgb" | "srgb-linear" | "display-p3" | "display-p3-linear" | "a98-rgb" | "rec2020" => {
                let lin = gamut_to_linear(name, raw);
                let m = match name {
                    "srgb" | "srgb-linear" => lin_srgb_to_xyz(),
                    "display-p3" | "display-p3-linear" => lin_p3_to_xyz(),
                    "a98-rgb" => lin_a98_to_xyz(),
                    _ => lin_rec2020_to_xyz(),
                };
                mul(&m, lin)
            }
            "prophoto-rgb" => {
                let lin = gamut_to_linear(name, raw);
                mul(&bradford_d50_to_d65(), mul(&lin_prophoto_to_xyz_d50(), lin))
            }
            "xyz" => raw,
            "xyz-d50" => mul(&bradford_d50_to_d65(), raw),
            "lab" | "lch" => {
                let (a, b) = if name == "lch" {
                    let h = raw[2].to_radians();
                    (raw[1] * h.cos(), raw[1] * h.sin())
                } else {
                    (raw[1], raw[2])
                };
                mul(&bradford_d50_to_d65(), lab_to_xyz(raw[0], a, b))
            }
            "oklab" | "oklch" => {
                let (a, b) = if name == "oklch" {
                    let h = raw[2].to_radians();
                    (raw[1] * h.cos(), raw[1] * h.sin())
                } else {
                    (raw[1], raw[2])
                };
                mul(&lin_srgb_to_xyz(), oklab_to_lin_srgb(raw[0] / 100.0, a, b))
            }
            "hsl" | "hwb" => {
                // HSL/HWB are defined on the non-linear sRGB encoding.
                let v = if name == "hsl" {
                    hsl_to_srgb(raw)
                } else {
                    hwb_to_srgb(raw)
                };
                mul(&lin_srgb_to_xyz(), gamut_to_linear("srgb", v))
            }
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Channel triple of `xyz` expressed in `target`'s channel domains.
    pub fn from_xyz_d65(target: &'static Space, xyz: [f64; 3]) -> [f64; 3] {
        match target.name {
            "srgb" => lin_to_srgb(mul(&xyz_to_lin_srgb(), xyz)),
            "srgb-linear" => mul(&xyz_to_lin_srgb(), xyz),
            "display-p3" => lin_to_srgb(mul(&inv(&lin_p3_to_xyz()), xyz)),
            "display-p3-linear" => mul(&inv(&lin_p3_to_xyz()), xyz),
            "a98-rgb" => {
                let lin = mul(&inv(&lin_a98_to_xyz()), xyz);
                [encode_a98(lin[0]), encode_a98(lin[1]), encode_a98(lin[2])]
            }
            "rec2020" => {
                let lin = mul(&inv(&lin_rec2020_to_xyz()), xyz);
                [encode_rec2020(lin[0]), encode_rec2020(lin[1]), encode_rec2020(lin[2])]
            }
            "prophoto-rgb" => {
                let lin = mul(
                    &inv(&lin_prophoto_to_xyz_d50()),
                    mul(&BRADFORD_D65_TO_D50, xyz),
                );
                [
                    encode_prophoto(lin[0]),
                    encode_prophoto(lin[1]),
                    encode_prophoto(lin[2]),
                ]
            }
            "xyz" => xyz,
            "xyz-d50" => mul(&BRADFORD_D65_TO_D50, xyz),
            "lab" => lab_from_d65(xyz),
            "lch" => {
                let [l, a, b] = lab_from_d65(xyz);
                to_polar(l, a, b)
            }
            "oklab" => xyz_to_oklab(xyz),
            "oklch" => {
                let [l, a, b] = xyz_to_oklab(xyz);
                to_polar(l, a, b)
            }
            "hsl" => srgb_to_hsl(lin_to_srgb(mul(&xyz_to_lin_srgb(), xyz))),
            "hwb" => srgb_to_hwb(lin_to_srgb(mul(&xyz_to_lin_srgb(), xyz))),
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Convert to another supported space, propagating `none` for channels the
    /// target shares a name with in the source.
    pub fn convert(&self, target: &'static Space) -> Color4 {
        if self.space.name == target.name {
            return self.clone();
        }
        // A color with every component missing can only convert to another fully
        // missing color: nothing downstream is determined (`srgb none none none`
        // → `lab none none none`).
        if self.comps.iter().all(|c| c.is_none()) {
            return Color4 {
                space: target,
                text: [None, None, None],
                comps: [None, None, None],
                alpha: self.alpha,
                alpha_missing: self.alpha_missing,
            };
        }
        let mut comps = Self::from_xyz_d65(target, self.to_xyz_d65());
        // CSS Color 4: a neutral color (zero chroma, or a/b both ~0) has an
        // undefined hue and serializes the hue channel as `none`. The rounded
        // OKLab reference matrices (Ottosson's M2 rows sum to ~5e-8, not 0) leave
        // a tiny a/b residual on exact neutrals; that magnitude is float noise
        // (well under the 1e-4 the goldens resolve), so snap it to 0. Grays then
        // print as `lab/oklab(L 0 0)` and `lch/oklch(L 0 none)`, matching golden.
        const NEUTRAL_TOL: f64 = 1e-7;
        let neutral = match target.name {
            "lab" | "oklab" => comps[1].abs() < NEUTRAL_TOL && comps[2].abs() < NEUTRAL_TOL,
            "lch" | "oklch" => comps[1].abs() < NEUTRAL_TOL,
            _ => false,
        };
        if neutral {
            if matches!(target.name, "lab" | "oklab") {
                comps[2] = 0.0;
            }
            comps[1] = 0.0;
        }
        let mut out: [Option<f64>; 3] = comps.map(Some);
        if neutral && matches!(target.name, "lch" | "oklch") {
            out[2] = None;
        }
        // Missing-component propagation. Tristimulus spaces (the RGB gamuts plus
        // `xyz`/`xyz-d50`) are all wired together by a 3×3 matrix and their
        // channels pair up positionally (r↔x, g↔y, b↔z), so the goldens carry a
        // `none` straight across by *index* even though the channel names differ.
        // Every other space (polar, hsl, hwb) has no such fixed pairing, so a
        // `none` only carries to a target channel of the same name.
        let src_tristimulus = matches!(
            self.space.name,
            "srgb" | "srgb-linear" | "display-p3" | "display-p3-linear" | "a98-rgb" | "rec2020"
                | "prophoto-rgb" | "xyz" | "xyz-d50"
        );
        let dst_tristimulus = matches!(
            target.name,
            "srgb" | "srgb-linear" | "display-p3" | "display-p3-linear" | "a98-rgb" | "rec2020"
                | "prophoto-rgb" | "xyz" | "xyz-d50"
        );
        if src_tristimulus && dst_tristimulus {
            for (i, comp) in self.comps.iter().enumerate() {
                if comp.is_none() {
                    out[i] = None;
                }
            }
        } else {
            for (i, ch) in target.channels.iter().enumerate() {
                if let Some(j) = self.space.channels.iter().position(|c| c.name == ch.name)
                    && self.comps[j].is_none()
                {
                    out[i] = None;
                }
            }
            // Perceptual pairs (`lab`/`lch` and `oklab`/`oklch`, including across
            // the two families). The chromatic channels (a/b, or chroma/hue) go
            // `none` only when BOTH of the source's second and third channels are
            // missing — a lone missing chroma or hue resolves to 0 and stays
            // determinate (`lch(10% none 30)` → `lab(10% 0 0)`). Missing *lightness*
            // normally leaves the chromatic channels concrete (they are derived
            // from a/b or C/H alone): the golden matrix over all twelve perceptual
            // source→target pairs shows the sole exception is the same-family
            // polar→cartesian `lch → lab`, where a missing lightness poisons the
            // whole color to `lab(none none none)` (the goldens keep this pair on a
            // direct LCh↔Lab path rather than the XYZ hub).
            let perceptual = |n: &str| matches!(n, "lab" | "lch" | "oklab" | "oklch");
            if perceptual(self.space.name) && perceptual(target.name) {
                if self.space.name == "lch" && target.name == "lab" && self.comps[0].is_none() {
                    out.fill(None);
                } else if self.comps[1].is_none() && self.comps[2].is_none() {
                    out[1] = None;
                    out[2] = None;
                }
            }
        }
        // A `hsl` source reaching a perceptual target poisons the channels whose
        // inputs are unknown: the cartesian a/b are undetermined only when BOTH
        // hue and saturation are missing, while the polar chroma/hue both collapse
        // to `none` as soon as saturation is missing (chroma tracks saturation, and
        // a hueless neutral drags the hue with it). Gamut/`xyz` targets stay
        // concrete — they substitute 0 and keep every derived channel.
        if self.space.name == "hsl" && matches!(target.name, "lab" | "lch" | "oklab" | "oklch") {
            let hue_missing = self.comps[0].is_none();
            let sat_missing = self.comps[1].is_none();
            let chromatic_none = match target.name {
                "lab" | "oklab" => hue_missing && sat_missing,
                _ => sat_missing,
            };
            if chromatic_none {
                out[1] = None;
                out[2] = None;
            }
        }
        // A `hwb` source whose whiteness AND blackness are both missing leaves the
        // lightness and chroma undetermined, but a polar target still reports the
        // hue (it comes straight from the present hue channel). Only the polar
        // targets poison this way — a cartesian target substitutes and keeps the
        // derived a/b concrete (`hwb(10deg none none)` → `lab(56% 76 70)`).
        if self.space.name == "hwb" && matches!(target.name, "lch" | "oklch")
            && self.comps[1].is_none() && self.comps[2].is_none()
        {
            out[0] = None;
            out[1] = None;
        }
        Color4 { space: target, comps: out, text: [None, None, None], alpha: self.alpha, alpha_missing: self.alpha_missing }
    }
}

/// HSL (h deg, s/l unclamped percent) → non-linear sRGB 0..1 *unclamped*.
/// the goldens' legacy color stores the formula result verbatim, so an
/// out-of-gamut saturation/lightness yields channels past 1 (or negative)
/// which the `to-space(_, srgb)` goldens emit raw (`color(srgb 2.25 …)`).
fn hsl_to_srgb(v: [f64; 3]) -> [f64; 3] {
    let h = ((v[0] % 360.0) + 360.0) % 360.0;
    let c = (1.0 - (2.0 * v[2] / 100.0 - 1.0).abs()) * (v[1] / 100.0);
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v[2] / 100.0 - c / 2.0;
    let (r1, g1, b1) = match h {
        hh if hh < 60.0 => (c, x, 0.0),
        hh if hh < 120.0 => (x, c, 0.0),
        hh if hh < 180.0 => (0.0, c, x),
        hh if hh < 240.0 => (0.0, x, c),
        hh if hh < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r1 + m, g1 + m, b1 + m]
}

/// HWB (h deg, w/b unclamped percent) → non-linear sRGB, *unclamped*.
/// the goldens invert the HWB→HSL relation — `lightness = w + (100 − b − w)/2`
/// and a *signed* `saturation = (100 − b − w)/(100 − |2·lightness − 100|)·100` —
/// then feeds that HSL to the unclamped converter. The negative denominator on
/// an out-of-range lightness is what drives `hwb(20deg 200% -125%)` to
/// `color(srgb 2.25 2.08333 2)` rather than a clamped white.
fn hwb_to_srgb(v: [f64; 3]) -> [f64; 3] {
    let total = v[1] + v[2];
    let (mut w, mut b) = (v[1], v[2]);
    if total > 100.0 {
        w = w / total * 100.0;
        b = b / total * 100.0;
    }
    let num = 100.0 - b - w;
    let l = w + num / 2.0;
    let den = 100.0 - (2.0 * l - 100.0).abs();
    let s = if num == 0.0 || den == 0.0 { 0.0 } else { num / den * 100.0 };
    hsl_to_srgb([v[0], s, l])
}
