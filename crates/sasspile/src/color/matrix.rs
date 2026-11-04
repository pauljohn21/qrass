//! The colour-space **mathematics** — the one route from any space's channels to CIE XYZ (D65) and back.
//!
//! Split from [`super::space`] because the two halves change for different reasons: that file is the value
//! and its printing, this one is the primaries tables, the transfer curves and the white-point adaptation.
//! Nothing here knows about Sass; every number is the corpus's oracle (`core_functions/color/to_space/**`).

use super::space::{Space, Transfer, White};

/// The D50 reference white, **derived rather than copied**: the srgb primaries applied to `(1, 1, 1)` are
/// the archive's D65 white (`to_space/srgb/xyz.hrx::white` prints `color(xyz 0.9504559271 1 1.0890577508)`)
/// and the adaptation below carries that onto D50. `xyz_to_lab` therefore divides by the very triple the
/// adaptation lands on, so a neutral is neutral **by construction**.
///
/// The transcribed `[0.964295675687546, 1.0, 0.825104602578703]` this replaced sat `8.3e-10` off the derived
/// `X`, and that alone printed `lab(100.0000000018% 0.0000001274 0.0000000081)` for `color(srgb 1 1 1)` where
/// the corpus states `lab(100% 0 0)` (`to_space/srgb/lab.hrx::white`): `a = 500 · (fx − fy)` turns a `1e-10`
/// white mismatch into a `1.4e-7` chroma. Two literals for one white was also the second answer the file's
/// own header forbids — the D65 side never needed its own triple, so neither does this one.
fn d50() -> [f64; 3] {
    adapt(
        SRGB_PRIMARIES.map(|row| row.iter().sum::<f64>()),
        White::D65,
        White::D50,
    )
}

/// Bradford-type white-point matrix, D50 → D65, **solved out of the archive** rather than transcribed.
///
/// What used to sit here were the digits the CSS Color 4 prose prints (`0.9555766 -0.0230393 0.0631636 …`).
/// The corpus does not compute with them: its D50→D65 row sums are `0.9956342097 1.0026671299 1.3221722918`
/// (`to_space/xyz_d50/xyz.hrx::white`) while the copied table summed to `0.9957009 0.9993688 1.3289869` —
/// worst entry off by `6.9e-3` (`1.3303659262` against `1.3372652`). Neutrals barely noticed; anything
/// reaching D50 did, which is every `lab`, `lch` and `xyz-d50` conversion.
///
/// `cargo run -q -p sasspile --example calibrate_space` reads `to_space/xyz_d50/xyz.hrx` from disk and
/// solves this table: `out_of_range/far` (`color(xyz-d50 -999999 0 0)`) states the first column at the
/// archive's full resolution, and the `white` + `out_of_range/near` pair — determinant `1.6` — states the
/// other two. Worst residual over the rows **not** used by the solve is `5.0e-11`, which is the half-step
/// of the archive's tenth decimal place, so the table matches the corpus as closely as the corpus can be
/// read. `tests/color_adaptation.rs` pins rows from both directions of the pair.
///
/// The ceiling this route cannot pass, measured rather than asserted: a table fitted to rows printed at ten
/// decimal places is knowable to about `5e-11` per entry, so the **derived** side of the pair keeps three
/// rows red by one unit in that last place — `to_space/xyz/xyz_d50.hrx::gray` (archive `0.5014942182`, we
/// print `0.5014942183`), `to_space/xyz/xyz_d50.hrx::out_of_range/far` (worst channel archive
/// `−29627.7791422469`, we print `−29627.7791449956`: `2.75e-6` absolute, `9.3e-11` relative), and
/// `to_space/srgb/xyz_d50.hrx::white` (archive `0.9642956764`, we print `0.9642956765`). Anything claiming
/// better digits than that from these fixtures is reading a number the archive never printed.
const BRADFORD: [[f64; 3]; 3] = [
    [
        0.955_473_421_488_075_2,
        -0.023_098_454_977_640_986,
        0.063_259_243_189_565_8,
    ],
    [
        -0.028_369_709_333_863_534,
        1.009_995_398_000_994_1,
        0.021_041_441_232_869_385,
    ],
    [
        0.012_314_014_864_481_963,
        -0.020_507_649_308_403_597,
        1.330_365_926_243_921_6,
    ],
];

/// The CIE epsilon/kappa pair the Lab curves are defined with.
const LAB_EPSILON: f64 = 216.0 / 24_389.0;
const LAB_KAPPA: f64 = 24_389.0 / 27.0;

/// `a98-rgb`'s transfer, as the archive applies it: a **pure gamma of `563/256` from the printed channel to
/// linear light**, so the printed value is the linear one raised to `256/563`.
///
/// Two things were wrong where this line sits, and the second is why they stayed invisible: the exponent was
/// the rounded `2.19921` rather than `563/256`, and it was on the **wrong side** of the pair — this file
/// decoded with `1/2.19921` and encoded with `2.19921`, the inverse of what the corpus computes
/// (`Srgb` and `Rec2020` already put the larger exponent on decode; `A98` was the odd arm). A pair of
/// reciprocal exponents looks symmetric because a curve fixes 0 and 1, so only a row converted through it
/// tells them apart. The command that says so, and the residuals it printed for each candidate, are recorded
/// once in `openspec/…/tasks.md` 9.17; re-run
/// `cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz <exponent>` to reproduce the verdict
/// (`hypothesis_holds=false` exits non-zero, so a refuted exponent cannot be pasted back in by someone
/// reading only the numbers).
const A98_GAMMA: f64 = 563.0 / 256.0;

/// `rec2020`'s transfer, as the archive applies it: a **plain signed gamma of `2.4`** on the power branch,
/// with no offset at all.
///
/// This arm used to read `((v + (1.09929682680944 - 1)) / 1.09929682680944) ^ (1 / 0.45)` — a shifted power
/// with an exponent of `1/0.45`, copied as a shape from prose about the space rather than read from the
/// corpus. The scan that separates the candidates is
/// `cargo run -q -p sasspile --example calibrate_space -- rec2020 xyz <exponent> <alpha>`, which fits the
/// primaries under a stated curve hypothesis and reports the worst residual over the archive's own rows
/// (`hypothesis_holds=false` exits non-zero):
///
/// | exponent | alpha | worst residual |
/// |---|---|---|
/// | `2.4` | `1.0` (no offset) | `4.914e-11` — the tenth-decimal half-step, i.e. the shape is right |
/// | `1/0.45` | `1.09929682680944` (**this file's former law**) | `0.214` |
/// | `2.4` | `1.09929682680944` | `0.0952` |
/// | `2.4` | `1.055` | `0.0538` |
/// | `2.4` | `1.019` | `0.0189` |
/// | `2.19921875` | `1.09929682680944` | `0.292` |
///
/// The mirrored direction agrees: `-- xyz rec2020 2.4 1.0` fits with a `7.70e-11` residual, so the encode side
/// is `printed = linear ^ (1/2.4)` with the same no-offset law.
///
/// **There is also no linear segment on this arm**, which is a reading rather than a preference: keeping the
/// former `4.5×` arms at the new exponent measured `regress=0 net=85` (run `rec2020-no-offset`), removing them
/// measured `regress=0 net=126` (run `rec2020-pure-power`) — so `net=41` is that one-variable comparison, both
/// runs at exponent `2.4`. Which side that reading actually speaks to matters:
/// `-- xyz rec2020 2.4 1.0` reports `smallest_linear_channel = 0.00163`, below the encode arm's `0.081`, while
/// its `smallest_curved_printed_channel = 0.0689` sits above the decode arm's `0.081/4.5`. **The encode arm was
/// exercised and the corpus prefers the power there; the decode arm stays unconstrained by any row.**
///
/// `prophoto-rgb` was probed the same way, and the two readings disagree in a way worth keeping straight:
/// `net` and `regress` said nothing (`rec2020-pure-power` to `prophoto-pure-power`: `regress=0 net=0`) — so an
/// earlier draft of this comment concluded no row reaches `1/16` or `1/512`. `ledger changed` refutes that:
/// eleven rows changed value, and the arms are the side that matches the archive —
/// `to_space/oklab/prophoto_rgb.hrx::missing/non_lightness` states `color(prophoto-rgb 0.016 0.016 0.016)`, the
/// arms answer `0.0159999996 0.0160000002 0.016` (one digit off), removing them answers `0.0215443466 …`
/// (`3.5e-3` off). Those rows are red either way because they are `missing/*` rows, which is exactly the class
/// `net` cannot see. So: **prophoto's encode arm is positively confirmed, its decode arm is unexercised**
/// (`smallest_curved_printed_channel = 0.1` for that pair), and the arms stay for the first reason, not the
/// second.
const REC2020_GAMMA: f64 = 2.4;

/// A signed power: the corpus pushes negative and out-of-range channels through the same curve
/// (`to_space/srgb/lab.hrx::out_of_range/near` converts `color(srgb -1 0.4 2)` and prints real numbers), so
/// every transfer is defined on the whole line, not clamped into `[0, 1]` first.
///
/// Which spaces keep a **linear segment** next to the power is measured per space, not inherited by analogy -
/// the three answers differ, which is why one shared rule would have been wrong. The readings behind them
/// (residuals, `regress`, `net`, `changed`) live once, in `openspec/…/tasks.md` 9.21; this states only the
/// conclusions, because the same digits copied into a second file is how this repository keeps acquiring a
/// second answer:
/// * `srgb` - the `12.92` arms are **confirmed** (removing them loses rows on both sides of the reading: some
///   `net`, and several more that stay red either way and only show in `changed`/`shift`); its shifted power is
///   confirmed too. The **threshold** of the segment is a transcription - no row this instrument reads enters
///   that region, and it cannot A/B a threshold anyway (see `tasks.md` 9.21).
/// * `rec2020` - the `4.5×` arms are **rejected** on the encode side; the decode side stays unconstrained.
/// * `prophoto-rgb` - the `1/16` arms **survive**, on `changed` evidence rather than on `net`.
/// Guarded by `tests/color_srgb_segment.rs`, and the segment question for the other two by their own runs.
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
            Self::A98 => sp(value, A98_GAMMA),
            Self::Prophoto => {
                if value.abs() >= 1.0 / 16.0 {
                    sp(value, 1.8)
                } else {
                    value / 16.0
                }
            }
            Self::Rec2020 => sp(value, REC2020_GAMMA),
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
            Self::A98 => sp(value, 1.0 / A98_GAMMA),
            Self::Prophoto => {
                if value.abs() > 1.0 / 512.0 {
                    sp(value, 1.0 / 1.8)
                } else {
                    value * 16.0
                }
            }
            Self::Rec2020 => sp(value, 1.0 / REC2020_GAMMA),
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
    let white = d50();
    let f = |index: usize| lab_f(xyz_d50[index] / white[index]);
    let (fx, fy, fz) = (f(0), f(1), f(2));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn lab_to_xyz(lab: [f64; 3]) -> [f64; 3] {
    let white = d50();
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
        lab_inverse(fx) * white[0],
        y * white[1],
        lab_inverse(fz) * white[2],
    ]
}

/// The linear-sRGB → XYZ (D65) primaries table. It lives here rather than in [`super::space`] because this
/// file is the home of the numeric tables; [`super::space::Space::matrix`] hands the same const out, so there
/// is still one copy.
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

/// `oklab`'s cone response, **solved out of the corpus** instead of transcribed.
///
/// The domain was never the problem, only the digits. The table is applied to XYZ (D65), which is where the
/// archive computes it: fitting `cone = C · xyz` over all six plain-output rows of
/// `core_functions/color/to_space/xyz/oklab.hrx` (`gray`, `middle`, `out_of_range/near`, `missing/{x,y,z}`)
/// reproduces every one to `1.2e-8` in the cone and `7.6e-8` in the channels, and answers the corpus's own D65
/// white (`to_space/srgb/xyz.hrx::white` prints `color(xyz 0.9504559271 1 1.0890577508)`) with three cones
/// agreeing to `1.4e-7` — that is, a neutral, which is what `to_space/srgb/oklab.hrx::white` demands.
///
/// What I withdrew, recorded because it was mine and it measured **downward**: earlier today I routed this
/// through linear sRGB on the argument that the transcribed rows sum to 1.3097 / 0.9984 / 0.9808 and so cannot
/// make `(1,1,1)` neutral. Equal row sums are a requirement on a matrix acting on linear sRGB; acting on XYZ
/// the invariant is `C · white = equal cones`. The detour put white at
/// `oklab(101.9416634099% 0.1844594607 0.0072495212)` against the archive's `oklab(100% 0 0)` — further from
/// the golden than the XYZ route's `101.8011897979%`.
///
/// **Not exact yet, and the limit is [`OKLAB_LAB`] rather than this table:** the fit is carried through that
/// matrix's ten digits, so a neutral still prints a residue near `1e-7` instead of the archive's literal `0`.
/// Recovering the lightness matrix independently (the `oklch` rows' hue is a constraint on it, and it must
/// come from the archive, not from a transcription) is the open half.
///
/// Only one direction of each pair is written down and [`invert`] produces the other. The hand-copied inverse
/// that used to sit here disagreed with this table's inverse, so `oklab → oklch → oklab` did not return what it
/// started from; making the pair mutual is what turned those 13 rows green at `regress=0`, and
/// `tests/color_space_round_trip.rs` pins the invariant.
const OKLAB_CONE: [[f64; 3]; 3] = [
    [0.819_022_521_501, 0.361_906_211_260, -0.128_873_797_578],
    [0.032_983_669_487, 0.929_286_875_760, 0.036_144_665_391],
    [0.048_177_206_189, 0.264_239_628_112, 0.633_547_875_735],
];

/// `oklab`'s LMS-cube-root → lightness/chroma/hue matrix; its inverse is computed, not copied. Its rows sum
/// to 0.9999999934, -0.0000000006 and 0.000000037 — the first is lightness as the cone root, the other two are
/// why an equal-cone colour prints a residue near 1e-8 instead of the archive's literal `0`
/// (`to_space/srgb/oklab.hrx::gray`). These ten digits are what caps the cone fit above.
const OKLAB_LAB: [[f64; 3]; 3] = [
    [0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_9],
    [1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_3],
    [0.025_904_037_1, 0.782_771_766_3, -0.808_675_766_4],
];

/// D65-referenced XYZ → `oklab`, whose LMS cube roots are the whole definition.
fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    let root = apply(&OKLAB_CONE, xyz).map(|value| sp(value, 1.0 / 3.0));
    apply(&OKLAB_LAB, root)
}

fn oklab_to_xyz(oklab: [f64; 3]) -> [f64; 3] {
    let root = apply(&invert(&OKLAB_LAB), oklab);
    apply(&invert(&OKLAB_CONE), root.map(|value| sp(value, 3.0)))
}

/// Cartesian lightness/chroma/hue, with the hue folded into a turn (`lch(… 280.5032243387deg)`).
///
/// Reachable from [`super::space`] because `oklab`/`oklch` and `lab`/`lch` are one triple read two ways: a
/// conversion inside such a pair must not go through XYZ, whose round trip is what turned an exactly-zero
/// channel into a `±1e-17` one and then a `360deg` hue.
pub(super) fn to_polar(lab: [f64; 3]) -> [f64; 3] {
    let hue = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    [lab[0], chroma, if chroma == 0.0 { 0.0 } else { hue }]
}

pub(super) fn from_polar(lch: [f64; 3]) -> [f64; 3] {
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
            apply(other.primaries(), decoded)
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
            let linear = apply(&invert(other.primaries()), in_white);
            let transfer = other.transfer();
            linear.map(|value| transfer.encode(value))
        }
    }
}
