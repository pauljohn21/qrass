//! The colour-space **mathematics** — the one route from any space's channels to CIE XYZ (D65) and back.
//!
//! Split from [`super::space`] because the two halves change for different reasons: that file is the value
//! and its printing, this one is the primaries tables, the transfer curves and the white-point adaptation.
//! Nothing here knows about Sass; every number is the corpus's oracle (`core_functions/color/to_space/**`).

use super::space::{Space, Transfer, White};

/// The D50 reference white, as the cone math reads it.
///
/// Only D50 is written down: the D65 side never needs its own triple, because the D50→D65 move is the one
/// matrix below and the corpus's own D65 white (`to_space/srgb/xyz.hrx::white` answers
/// `color(xyz 0.9504559271 1 1.0890577508)`) comes out of applying it to the srgb primaries. A second
/// hand-copied white is a second answer for one fact — the constant that used to sit here was never read.
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

/// The linear-sRGB → XYZ (D65) primaries table, named so the `oklab` route can reach it without the
/// fallible accessor. It lives here rather than in [`super::space`] because this file is the home of the
/// numeric tables; [`super::space::Space::matrix`] hands the same const out, so there is still one copy.
pub(super) const SRGB_PRIMARIES: [[f64; 3]; 3] = [
    [
        0.412_390_799_265_934,
        0.357_584_339_383_837,
        0.180_480_788_401_814,
    ],
    [
        0.212_639_005_871_520,
        0.715_168_678_767_736,
        0.072_192_315_360_733,
    ],
    [
        0.019_330_818_715_591,
        0.119_194_779_794_625,
        0.950_532_152_249_660,
    ],
];

/// `oklab`'s cone response, defined on **linear sRGB** — not on XYZ.
///
/// Applying this table to XYZ is what made a white print `oklab(101.8011897979% 0.1787788126 -0.0136361186)`
/// instead of `oklab(100% 0 0)` (`to_space/srgb/oklab.hrx::white`): its rows sum to 1.3097 / 0.9984 / 0.9808,
/// so three equal channels cannot give three equal cones, and equal cones are exactly what a neutral has to
/// have — which is also why the corpus answers `oklab(59.8180730527% 0 0)` for `color(srgb 0.5 0.5 0.5)`
/// (`…::gray`, a = b = 0 exactly). The route below is therefore XYZ (D65) → linear sRGB → this matrix, and
/// only one direction is written down: [`invert`] produces the other, because the hand-copied inverse that
/// used to sit here disagreed with the forward table — its **second** row carried `0.1116806229` where this
/// table's own inverse is `1.092975889` — so a colour sent out and read back did not come back as it went in.
/// That round trip is what the 13 rows of `to_space/oklab/oklch` and `to_space/oklch/oklab` measure, and they
/// went green at `regress=0` (`core_functions/color/to_space/oklab/oklch::middle` and twelve siblings); both
/// halves of this batch made it, since a round trip needs one table and an inverse of it, not either alone.
/// `tests/color_space_round_trip.rs` pins it so a second copied table cannot come back quietly.
///
/// **Still wrong, said plainly because the next reader must not take this table as calibrated:** the corpus's
/// cone matrix must have **equal row sums** (the two neutral rows above pin that), and these digits do not —
/// `to_space/srgb/oklab.hrx::white` prints `oklab(101.9416634099% 0.1844594607 0.0072495212)` and `…::gray`
/// prints `oklab(60.9795386897% 0.110340095 0.0043365239)` against the archive's `oklab(59.8180730527% 0 0)`.
/// Both readings are **further** from the golden than the run before this batch (`101.8011897979% …` and
/// `60.8955100818% 0.1069420407 -0.0081568634`), so this file's 545 changed rows are not progress: 13 turned
/// green, the rest got a different wrong number, and every row that crosses to or from a non-polar space
/// (`to_space/oklab/srgb`, `to_space/srgb/oklab`, `to_space/xyz/oklab`, and the same for `oklch`) stays red —
/// several of those expect `color-mix(in oklab, …)` output, which this compiler cannot emit at all.
/// The nine cone entries can be solved from the archive's plain-output rows the way `invert` now recovers
/// the other direction, but that has not been done here, and no row count is evidence that it was; the
/// calibration target is not only equal row sums, because [`OKLAB_LAB`]'s second and third rows must sum to
/// zero as well or a neutral prints a residue instead of the archive's literal `0`.
const OKLAB_CONE: [[f64; 3]; 3] = [
    [0.818_933_010_1, 0.361_866_742_4, 0.128_859_713_6],
    [0.032_984_543_6, 0.929_316_420_8, 0.036_145_668_7],
    [0.048_200_301_8, 0.015_660_887_9, 0.916_906_934_1],
];

/// `oklab`'s LMS-cube-root → lightness/chroma/hue matrix; its inverse is computed, not copied. Its rows sum
/// to 0.9999999934, -0.0000000006 and 0.000000037 — the first is lightness as the cone root, the other two
/// are what makes an equal-cone colour print `a = b = 0`, and at these magnitudes it prints a residue near
/// 1e-8 instead of the archive's literal `0` (`to_space/srgb/oklab.hrx::gray`).
const OKLAB_LAB: [[f64; 3]; 3] = [
    [0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_9],
    [1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_3],
    [0.025_904_037_1, 0.782_771_766_3, -0.808_675_766_4],
];

/// XYZ (D65) → `oklab`, through the linear sRGB the space is defined on.
fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    let linear = apply(&invert(&SRGB_PRIMARIES), xyz);
    let root = apply(&OKLAB_CONE, linear).map(|value| sp(value, 1.0 / 3.0));
    apply(&OKLAB_LAB, root)
}

fn oklab_to_xyz(oklab: [f64; 3]) -> [f64; 3] {
    let root = apply(&invert(&OKLAB_LAB), oklab);
    let linear = apply(&invert(&OKLAB_CONE), root.map(|value| sp(value, 3.0)));
    apply(&SRGB_PRIMARIES, linear)
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
        // The **inverse** curve: these channels are already Lab, so they go to XYZ. The sibling branch in
        // [`of_xyz_d65`] is the forward one. This pair was inverted here once (the branch called
        // `xyz_to_lab`, and `lab_to_xyz` sat unused — the dead-code warning was the finding), which a chained
        // conversion hits as soon as a `lab` colour is read: `color.to-space(color.to-space(#abc, lab), srgb)`.
        Space::Lab => lab_to_xyz(channels),
        Space::Lch => lab_to_xyz(from_polar(channels)),
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
