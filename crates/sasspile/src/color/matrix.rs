//! The colour-space **mathematics** — the one route from any space's channels to CIE XYZ (D65) and back.
//!
//! Split from [`super::space`] because the two halves change for different reasons: that file is the value
//! and its printing, this one is the primaries tables, the transfer curves and the white-point adaptation.
//! Nothing here knows about Sass; every number is the corpus's oracle (`core_functions/color/to_space/**`).

use super::space::{Space, Transfer, White};

/// The two reference whites, as the cone math reads them.
/// `l = 0.964295676…`-style white points, as the cone math reads them.
const D65: [f64; 3] = [0.950_455_938_644_184, 1.0, 1.089_057_772_410_216_8];
const D50: [f64; 3] = [0.964_295_675_687_546, 1.0, 0.825_104_602_578_703];

/// Bradford cone-response matrix (D65-adapted form, as CSS Color 4 states it).
const BRADFORD: [[f64; 3]; 3] = [
    [0.955_576_6, -0.023_039_3, 0.063_163_6],
    [-0.028_289_5, 1.006_652_5, 0.021_005_8],
    [0.012_298_2, -0.020_576_5, 1.337_265_2],
];

/// The CIE epsilon/kappa pair the Lab curves are defined with.
const LAB_EPSILON: f64 = 216.0 / 24_389.0;
const LAB_KAPPA: f64 = 24_389.0 / 27.0;

/// A signed power: the corpus pushes negative and out-of-range channels through the same curve
/// (`to_space/srgb/lab.hrx::out_of_range/near` converts `color(srgb -1 0.4 2)` and prints real numbers), so
/// every transfer is defined on the whole line, not clamped into `[0, 1]` first.
fn sp(value: f64, exponent: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value.signum() * value.abs().powf(exponent)
    }
}

impl Transfer {
    /// Printed channel → linear light.
    fn decode(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                if value.abs() <= 0.040_45 {
                    value / 12.92
                } else {
                    sp(value.signum() * ((value.abs() + 0.055) / 1.055), 2.4)
                }
            }
            Self::A98 => sp(value, 1.0 / 2.199_21),
            Self::Prophoto => {
                if value.abs() >= 1.0 / 16.0 {
                    sp(value, 1.8)
                } else {
                    value / 16.0
                }
            }
            Self::Rec2020 => {
                if value.abs() < 0.081 / 4.5 {
                    value / 4.5
                } else {
                    sp(
                        value.signum()
                            * ((value.abs() + (1.099_296_826_809_44 - 1.0)) / 1.099_296_826_809_44),
                        1.0 / 0.45,
                    )
                }
            }
        }
    }

    /// Linear light → printed channel.
    fn encode(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                if value.abs() <= 0.003_130_8 {
                    value * 12.92
                } else {
                    value.signum() * (1.055 * value.abs().powf(1.0 / 2.4) - 0.055)
                }
            }
            Self::A98 => sp(value, 2.199_21),
            Self::Prophoto => {
                if value.abs() > 1.0 / 512.0 {
                    sp(value, 1.0 / 1.8)
                } else {
                    value * 16.0
                }
            }
            Self::Rec2020 => {
                if value.abs() < 0.081 {
                    value * 4.5
                } else {
                    value.signum()
                        * (1.099_296_826_809_44 * value.abs().powf(0.45)
                            - (1.099_296_826_809_44 - 1.0))
                }
            }
        }
    }
}

/// `M · v`.
fn apply(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

/// The inverse of a 3×3 matrix, computed rather than copied: a second hand-transcribed table is how this
/// repository keeps ending up with two answers for one fact. Only the primaries matrices go in, all of them
/// well-conditioned, so no singularity handling is attempted here.
fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    // Adjugate over the determinant: each entry is the cofactor of the *transposed* position.
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) / det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) / det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) / det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) / det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) / det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) / det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) / det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) / det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) / det,
        ],
    ]
}

/// White-point conversion between D65 and D50.
///
/// The plain matrix product, **not** a von Kries scaling of cone ratios: `BRADFORD` already carries the
/// white-point shift (it maps the D50 white onto the D65 white), so multiplying by cone ratios on top of it
/// adapts twice. That mistake showed as a full unit of `a` on a saturated colour — `color(srgb 0.2 0.4 0.8)`
/// computed `lab(45.3584412071% 20.7260166824 -57.206844096)` where the corpus says
/// `lab(44.1210142293% 10.9543336098 -59.0857381981)` (`to_space/srgb/lab.hrx::middle`), while neutrals looked
/// nearly right, which is how it would slip past a grey-only check.
///
/// The second half of that lesson, which this file initially broke again: **exactly one adaptation per
/// route**. `to_xyz_d65` used to fold `XyzD50` here and then fold once more at its tail because the space's
/// own white is `D50` — so an identity conversion `color(xyz-d50 0.2 0.4 0.8)` → `xyz-d50` computed
/// `color(xyz-d50 1.0640412 …)`, the matrix applied twice to the same triple. A space whose numbers already
/// are XYZ returns them untouched and lets the tail do the single move.
pub fn adapt(xyz: [f64; 3], from: White, to: White) -> [f64; 3] {
    match (from, to) {
        (White::D50, White::D65) => apply(&BRADFORD, xyz),
        (White::D65, White::D50) => apply(&invert(&BRADFORD), xyz),
        _ => xyz,
    }
}

/// The Lab curve, shared by `lab`/`lch` (D50) and reused for the `oklab` family's cube root.
fn lab_f(t: f64) -> f64 {
    if t > LAB_EPSILON {
        t.cbrt()
    } else {
        (LAB_KAPPA * t + 16.0) / 116.0
    }
}

fn lab_inverse(f: f64) -> f64 {
    let cube = f * f * f;
    if cube > LAB_EPSILON {
        cube
    } else {
        116.0 * (f - 16.0 / 116.0) / LAB_KAPPA
    }
}

/// D50-referenced XYZ → `lab`.
fn xyz_to_lab(xyz_d50: [f64; 3]) -> [f64; 3] {
    let f = |index: usize| lab_f(xyz_d50[index] / D50[index]);
    let (fx, fy, fz) = (f(0), f(1), f(2));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn lab_to_xyz(lab: [f64; 3]) -> [f64; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = fy + lab[1] / 500.0;
    let fz = fy - lab[2] / 200.0;
    // The lightness branch is the CIE definition: above `L = 8` the curve is a cube, below it the linear
    // segment the same epsilon/kappa pair states.
    let y = if lab[0] > 8.0 {
        fy * fy * fy
    } else {
        lab[0] / LAB_KAPPA
    };
    [
        lab_inverse(fx) * D50[0],
        y * D50[1],
        lab_inverse(fz) * D50[2],
    ]
}

/// D65-referenced XYZ → `oklab`, whose LMS cube roots are the whole definition.
fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    let cone = [
        0.818_933_010_1 * xyz[0] + 0.361_866_742_4 * xyz[1] + 0.128_859_713_6 * xyz[2],
        0.032_984_543_6 * xyz[0] + 0.929_316_420_8 * xyz[1] + 0.036_145_668_7 * xyz[2],
        0.048_200_301_8 * xyz[0] + 0.015_660_887_9 * xyz[1] + 0.916_906_934_1 * xyz[2],
    ];
    let root = cone.map(|value| sp(value, 1.0 / 3.0));
    [
        0.210_454_255_3 * root[0] + 0.793_617_785_0 * root[1] - 0.004_072_046_9 * root[2],
        1.977_998_495_1 * root[0] - 2.428_592_205_0 * root[1] + 0.450_593_709_3 * root[2],
        0.025_904_037_1 * root[0] + 0.782_771_766_3 * root[1] - 0.808_675_766_4 * root[2],
    ]
}

fn oklab_to_xyz(oklab: [f64; 3]) -> [f64; 3] {
    let root = [
        oklab[0] + 0.396_337_777_4 * oklab[1] + 0.215_803_757_3 * oklab[2],
        oklab[0] - 0.105_561_345_8 * oklab[1] - 0.063_854_172_8 * oklab[2],
        oklab[0] - 0.089_484_177_5 * oklab[1] - 1.291_485_548_0 * oklab[2],
    ];
    let cone = root.map(|value| sp(value, 3.0));
    [
        1.226_879_873_4 * cone[0] - 1.386_668_417_4 * cone[1] + 0.367_874_353_5 * cone[2],
        -0.040_575_762_6 * cone[0] + 0.111_680_622_9 * cone[1] - 0.068_116_547_0 * cone[2],
        -0.076_372_910_0 * cone[0] - 0.423_851_940_9 * cone[1] + 1.524_968_906_0 * cone[2],
    ]
}

/// Cartesian lightness/chroma/hue, with the hue folded into a turn (`lch(… 280.5032243387deg)`).
fn to_polar(lab: [f64; 3]) -> [f64; 3] {
    let hue = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    [lab[0], chroma, if chroma == 0.0 { 0.0 } else { hue }]
}

fn from_polar(lch: [f64; 3]) -> [f64; 3] {
    let angle = lch[2].to_radians();
    [lch[0], lch[1] * angle.cos(), lch[1] * angle.sin()]
}

/// This colour's channels as CIE XYZ referenced to D65. A `none` channel reads as `0`, which is what the
/// corpus shows for `color(srgb none 0.2 0.3)` converted to `lab` (`to_space/srgb/lab.hrx::missing/red`); the
/// powerlessness itself only survives an identity conversion, and printing keeps it via [`InSpace`].
pub fn to_xyz_d65(space: Space, channels: [f64; 3]) -> [f64; 3] {
    let white = space.white();
    let xyz = match space {
        Space::Lab => xyz_to_lab(channels),
        Space::Lch => xyz_to_lab(from_polar(channels)),
        Space::Oklab => oklab_to_xyz(channels),
        Space::Oklch => oklab_to_xyz(from_polar(channels)),
        Space::Xyz => return channels,
        // Its own numbers, at D50: the tail `adapt` below is this space's one and only white-point move.
        Space::XyzD50 => channels,
        other => {
            let transfer = other.transfer();
            let decoded = channels.map(|value| transfer.decode(value));
            apply(other.matrix().unwrap_or(&BRADFORD), decoded)
        }
    };
    adapt(xyz, white, White::D65)
}

/// XYZ (D65) as this space's channels.
pub fn of_xyz_d65(space: Space, xyz: [f64; 3]) -> [f64; 3] {
    match space {
        Space::Xyz => xyz,
        Space::XyzD50 => adapt(xyz, White::D65, White::D50),
        Space::Lab => xyz_to_lab(adapt(xyz, White::D65, White::D50)),
        Space::Lch => to_polar(xyz_to_lab(adapt(xyz, White::D65, White::D50))),
        Space::Oklab => xyz_to_oklab(xyz),
        Space::Oklch => to_polar(xyz_to_oklab(xyz)),
        other => {
            let in_white = adapt(xyz, White::D65, other.white());
            let linear = apply(&invert(other.matrix().unwrap_or(&BRADFORD)), in_white);
            let transfer = other.transfer();
            linear.map(|value| transfer.encode(value))
        }
    }
}
