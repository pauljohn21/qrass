//! 色彩空间的**数学** —— 从任意空间的通道到 CIE XYZ（D65）再回来的唯一一条路线。
//!
//! 从 [`super::space`] 里分开，是因为两半因不同理由改动：那个文件是值与它的打印，这个是 primaries 表、传递
//! 曲线与白点适应。这里没有任何东西认识 Sass；每个数都来自语料的 oracle（`core_functions/color/to_space/**`）。

use super::space::{Space, Transfer, White};

/// D50 参考白，**是推出的而不是录来的**：把 srgb primaries 作用在 `(1, 1, 1)` 上就是归档的 D65 白
/// （`to_space/srgb/xyz.hrx::white` 打 `color(xyz 0.9504559271 1 1.0890577508)`），而下面的适应把那个携到 D50。
/// 所以 `xyz_to_lab` 除的正是适应落到的那个三元组，于是一个中性色是**构造上**中性的。
///
/// 它替掉的转录值 `[0.964295675687546, 1.0, 0.825104602578703]` 在推出的 `X` 上差了 `8.3e-10`，仅那一点就把
/// `color(srgb 1 1 1)` 打成 `lab(100.0000000018% 0.0000001274 0.0000000081)`，而语料说 `lab(100% 0 0)`
/// （`to_space/srgb/lab.hrx::white`）：`a = 500 · (fx − fy)` 把一个 `1e-10` 的白点错位放大成 `1.4e-7` 的色度。
/// 一个白点用两个字面量也是文件自己页头禁的第二份答案 —— D65 那侧从不需要自己的三元组，这一侧也不需要。
fn d50() -> [f64; 3] {
    adapt(
        SRGB_PRIMARIES.map(|row| row.iter().sum::<f64>()),
        White::D65,
        White::D50,
    )
}

/// Bradford 型的白点矩阵，D50 → D65，**从归档解出来的**而不是录来的。
///
/// 过去坐在这里的是 CSS Color 4 散文印出的那几个数（`0.9555766 -0.0230393 0.0631636 …`）。语料不算它们：
/// 它的 D50→D65 行和是 `0.9956342097 1.0026671299 1.3221722918`（`to_space/xyz_d50/xyz.hrx::white`），而录来的
/// 表加起来是 `0.9957009 0.9993688 1.3289869` —— 最差的一项差 `6.9e-3`（`1.3303659262` 对 `1.3372652`）。
/// 中性色几乎看不出来；任何走到 D50 的就看得出来了，而那是每一个 `lab`、`lch` 与 `xyz-d50` 的换算。
///
/// `cargo run -q -p sasspile --example calibrate_space` 从磁盘读 `to_space/xyz_d50/xyz.hrx` 并解出这张表：
/// `out_of_range/far`（`color(xyz-d50 -999999 0 0)`）以归档的全分辨率定下第一列，而 `white` + `out_of_range/near`
/// 那一对（行列式 `1.6`）定下另两列。**没有**被解法用到的那些行上最差残差是 `5.0e-11`，那就是归档第十位小数的
/// 半步，所以这张表在“能读到什么”的限度内贴着语料。`tests/color_adaptation.rs` 把那一对两个方向的行都固定住。
///
/// 这条路线越不过去的天花板，是量出来的而不是声称的：按“印到十位小数”的行拟合出的一张表，每项只能知道到
/// 大约 `5e-11`，所以*推出的*那侧会因最后一位的一个单位而让两簇共 **6 行** 保持红（现量：
/// `to_space/xyz/xyz_d50` 2 行、`to_space/srgb/xyz_d50` 4 行）。这里只点名其中三行为例：
/// `to_space/xyz/xyz_d50.hrx::gray`（归档 `0.5014942182`，我们打 `0.5014942183`）、
/// `to_space/xyz/xyz_d50.hrx::out_of_range/far`（最差通道归档 `−29627.7791422469`，我们打
/// `−29627.7791449956`：绝对 `2.75e-6`、相对 `9.3e-11`），以及
/// `to_space/srgb/xyz_d50.hrx::white`（归档 `0.9642956764`，我们打 `0.9642956765`）。其余三行同形（末位一个单位）。
/// 任何从这些 fixture 声称更好位数的说法，都是在读一个归档从未印过的数。
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

/// Lab 曲线定义所用的 CIE epsilon/kappa 一对。
const LAB_EPSILON: f64 = 216.0 / 24_389.0;
const LAB_KAPPA: f64 = 24_389.0 / 27.0;

/// `a98-rgb` 的传递曲线，按归档施加它的方式：一个**从打印通道到线性光的纯 gamma `563/256`**，所以打印值是
/// 线性值升到 `256/563`。
///
/// 这一行坐着的地方有两件事错了，而第二件正是它们为何一直隐形：指数是舍入后的 `2.19921` 而不是 `563/256`，
/// 而且它在这一对的**错的那侧** —— 本文件用 `1/2.19921` 解码、用 `2.19921` 编码，恰好是语料所算的反面（`Srgb` 与
/// `Rec2020` 已经把较大的指数放在解码；`A98` 是那条例外的一臂）。一对倒数的指数看着对称，因为曲线钉住了 0 与 1，
/// 所以只有一行穿过它的换算能分辨它们。说出这一点的命令，以及它给每个候选印的残差，在
/// `openspec/…/tasks.md` 9.17 里只记一次；重跑
/// `cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz <exponent>` 可复现那个判定（
/// `hypothesis_holds=false` 会非零退出，所以只看数字的人没法把一个已被驳倒的指数粘回来）。
const A98_GAMMA: f64 = 563.0 / 256.0;

/// `rec2020` 的传递曲线，按归档施加它的方式：在幂那一支上是**一个普通的带符号 gamma `2.4`**，完全没有偏移。
///
/// 这一臂过去读作 `((v + (1.09929682680944 - 1)) / 1.09929682680944) ^ (1 / 0.45)` —— 一个带平移的幂，指数是
/// `1/0.45`，作为*形状*从关于这个空间的散文里录来，而不是从语料读出。分开那些候选的扫描是
/// `cargo run -q -p sasspile --example calibrate_space -- rec2020 xyz <exponent> <alpha>`，它在一个声明的曲线
/// 假设下拟合 primaries，并报告归档自己那些行上的最差残差（`hypothesis_holds=false` 非零退出）：
///
/// | 指数 | 偏移 | 最差残差 |
/// |---|---|---|
/// | `2.4` | `1.0`（无偏移） | `4.914e-11` —— 第十位小数的半步，也就是形状对了 |
/// | `1/0.45` | `1.09929682680944`（**本文件先前那条律**） | `0.214` |
/// | `2.4` | `1.09929682680944` | `0.0952` |
/// | `2.4` | `1.055` | `0.0538` |
/// | `2.4` | `1.019` | `0.0189` |
/// | `2.19921875` | `1.09929682680944` | `0.292` |
///
/// 镜像方向也同意：`-- xyz rec2020 2.4 1.0` 以 `7.70e-11` 的残差拟合，所以编码侧是
/// `printed = linear ^ (1/2.4)`，同样是无偏移那条律。
///
/// **这一臂上也没有线性分段**，那是一个读数而不是一个偏好：在新指数下保留先前的 `4.5×` 那一臂量到
/// `regress=0 net=85`（run `rec2020-no-offset`），去掉它们量到 `regress=0 net=126`（run `rec2020-pure-power`）
/// —— 所以 `net=41` 就是那个单变量对比，两个 run 的指数都是 `2.4`。这个读数到底说哪一侧很要紧：
/// `-- xyz rec2020 2.4 1.0` 报 `smallest_linear_channel = 0.00163`，低于编码那臂的 `0.081`，而它的
/// `smallest_curved_printed_channel = 0.0689` 高于解码那臂的 `0.081/4.5`。**编码那臂被演练过，而语料在那侧偏好幂；
/// 解码那臂仍不被任何行约束。**
///
/// `prophoto-rgb` 也用同一个方式探过，而两个读数在一处不一致，值得说清：`net` 与 `regress` 什么也没说
/// （从 `rec2020-pure-power` 到 `prophoto-pure-power`：`regress=0 net=0`）—— 所以本注释的一份早期草稿由此得出
/// “没有行达到 `1/16` 或 `1/512`”。`ledger changed` 驳了它：十一行变了值，而那两臂才是对归档的一侧 ——
/// `to_space/oklab/prophoto_rgb.hrx::missing/non_lightness` 说 `color(prophoto-rgb 0.016 0.016 0.016)`，
/// 保留两臂答 `0.0159999996 0.0160000002 0.016`（差一位），去掉则答 `0.0215443466 …`（差 `3.5e-3`）。那些行
/// 两边都红，因为它们是 `missing/*` 行 —— 那正是 `net` 看不见的类。所以：**prophoto 的编码臂被正向确认，它的
/// 解码臂未被演练**（那一对的 `smallest_curved_printed_channel = 0.1`），而两臂留下的理由是前者，不是后者。
const REC2020_GAMMA: f64 = 2.4;

/// 一个带符号的幂：语料把负值与范围外的通道也穿过同一条曲线（`to_space/srgb/lab.hrx::out_of_range/near` 换算
/// `color(srgb -1 0.4 2)` 并打出实数），所以每条传递都在整条直线上定义，而不是先夹进 `[0, 1]`。
///
/// 哪些空间在幂旁边保留**一个线性分段**，是逐空间量出来的，不是靠类比承继 —— 三个答案各不相同，这就是为何
/// 一条共享规则会是错的。那三条分段 A/B 的读数只住 `openspec/…/tasks.md` **9.21**；上面 `REC2020_GAMMA` 与
/// `PROPHOTO` 那段引的残差、`net`、`changed` 只住 **9.20**（那两段里写的数都是对它的转述，不是第二处推导）。
/// 这里只说结论，因为同一批数字再写一遍就是本仓库不断得到第二份答案的方式：
/// * `srgb` —— `12.92` 那两臂是**正向确认的**（去掉它们丢的行计在 `regress=156`、`net=0`，另有几条两边都红、
///   只在 `changed`/`shift` 里显形）；它带平移的幂也被确认。那分段的**阈值**是一项转录 —— 这台量具读到的行
///   没有一项进入那个区域，而它也没法对一个阈值做 A/B（见 `tasks.md` 9.21）。
/// * `rec2020` —— `4.5×` 那两臂在**编码侧被拒**；解码侧仍不受约束。
/// * `prophoto-rgb` —— `1/16`（解码）与 `1/512`（编码）那一对臂**保住**，依据是 `changed` 而不是 `net`。
/// 由 `tests/color_srgb_segment.rs` 守住，而其余两个的分段问题由它们各自的 run 守住。
fn sp(value: f64, exponent: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value.signum() * value.abs().powf(exponent)
    }
}

impl Transfer {
    /// 打印通道 → 线性光。
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

    /// 线性光 → 打印通道。
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

/// 一个 3×3 矩阵的逆，是算出来的而不是录来的：第二份手抄的表就是本仓库不断为一个事实留下两个答案的方式。
/// 只有 primaries 矩阵进来，它们都是条件良好的，所以这里不做任何奇异性处理。
fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    // 伴随矩阵除以行列式：每一项都是*转置后*那个位置的余子式。
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

/// 白点在 D65 与 D50 之间的换算。
///
/// 就是一个矩阵相乘，**不是**对锥比的 von Kries 缩放：`BRADFORD` 已经带着白点的偏移（它把 D50 白映到 D65 白），
/// 所以在它之上再乘一次锥比就是适应了两次。那个错误在一个饱和颜色上表现为整整一个 `a` 单位的偏离，而语料说
/// `lab(44.1210142293% 10.9543336098 -59.0857381981)`（`to_space/srgb/lab.hrx::middle`）；中性色却看着几乎对，
/// 这就是它会溜过只测灰色的那道检查的原因。
///
/// 那个教训的第二半，而本文件一开始又把它弄坏了：**每条路线恰好一次适应**。`to_xyz_d65` 过去在这里折叠 `XyzD50`，
/// 然后在尾部因空间自己的白是 `D50` 又折叠一次 —— 于是一次恒等换算 `color(xyz-d50 0.2 0.4 0.8)` → `xyz-d50` 算出
/// `color(xyz-d50 1.0640412 …)`，同一个三元组被矩阵作用了两次。一个数字本身就是 XYZ 的空间应原样返回它们，
/// 让尾部做那唯一一次移动。
pub fn adapt(xyz: [f64; 3], from: White, to: White) -> [f64; 3] {
    match (from, to) {
        (White::D50, White::D65) => apply(&BRADFORD, xyz),
        (White::D65, White::D50) => apply(&invert(&BRADFORD), xyz),
        _ => xyz,
    }
}

/// Lab 那条曲线，由 `lab`/`lch`（D50）共用，并为 `oklab` 族的立方根复用。
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

/// 以 D50 为基准的 XYZ → `lab`。
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
    // 亮度那一支就是 CIE 的定义：`L = 8` 之上曲线是一个立方，之下则由同一对 epsilon/kappa 说出的线性分段。
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

/// 线性 sRGB → XYZ（D65）的 primaries 表。它住在这里而不是 [`super::space`]，因为这个文件才是数值表的住所；
/// [`super::space::Space::matrix`] 交出的就是同一个 const，所以仍然只有一份。
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

/// `oklab` 的锥响应，**从语料解出来的**而不是录来的。
///
/// 定义域从来不是问题，问题只在数字。这张表作用在 XYZ（D65）上，而那正是归档算它的方式：对
/// `core_functions/color/to_space/xyz/oklab.hrx` 那 12 条普通输出行**里拟合用到的那 6 条**（`gray`、`middle`、
/// `out_of_range/near`、`missing/{x,y,z}`）拟合 `cone = C · xyz`，每一行都在锥上复现到 `1.2e-8`、在通道上到 `7.6e-8`，
/// 而它用三个互差在 `1.4e-7` 以内的锥答语料自己的 D65 白（`to_space/srgb/xyz.hrx::white` 打
/// `color(xyz 0.9504559271 1 1.0890577508)`）—— 也就是一个中性色，那正是 `to_space/srgb/oklab.hrx::white` 要的。
///
/// 今天走过又放弃的那条路线，记下来是因为它是我说的、而且它量的是**向下**：我把这条路线改走线性 sRGB，理由是
/// 录来的表行和都不等于 1，所以做不出 `(1,1,1)` 中性（那三行和的数只住在 `tasks.md` 9.11 与基线节里，这里不重抄）。
/// 相等的行和是对一个作用于线性 sRGB 的矩阵的要求；作用于 XYZ 时那个不变量是 `C · white = equal cones`。那个弯绕
/// 把白行推到 `oklab(101.9416634099% …)`（两个数都记在 `tasks.md`），而归档是 `oklab(100% 0 0)` —— 比走 XYZ 的
/// `101.8011897979%` 更远离金样。
///
/// **还不是精确的，而天花板在 [`OKLAB_LAB`] 而不是这张表：**拟合是携着那个矩阵的十位数字跑下去的，所以一个中性色
/// 仍打出一个接近 `1e-7` 的余数（按打印出来的**百分数刻度**：`oklab(100.000000148% …)`，也就是小数刻度的 `1.5e-9`），
/// 而不是归档那个字面的 `0`。把亮度矩阵独立解回来（`oklch` 那些行的色相是对它的约束，
/// 而它必须来自归档而不是某份转录）是还开着的那一半。
///
/// 每一对里只写下其中一个方向，另一个由 [`invert`] 产出。过去坐在这里的那份手抄逆矩阵与这张表的逆不一致，所以
/// `oklab → oklch → oklab` 回不到出发点；让那对彼此互逆，就是把那 13 行在 `regress=0` 下变绿的动作，而
/// `tests/color_space_round_trip.rs` 钉住那个不变量。
const OKLAB_CONE: [[f64; 3]; 3] = [
    [0.819_022_521_501, 0.361_906_211_260, -0.128_873_797_578],
    [0.032_983_669_487, 0.929_286_875_760, 0.036_144_665_391],
    [0.048_177_206_189, 0.264_239_628_112, 0.633_547_875_735],
];

/// `oklab` 从 LMS 立方根到 lightness/chroma/hue 的矩阵；它的逆是算的，不是录的。它的行和是
/// 0.9999999934、-0.0000000006 与 0.000000037 —— 第一个就是把锥开立方后的亮度，另两个就是一个等锥的颜色为何
/// 打出接近 1e-8 的余数而不是归档那个字面的 `0`（`to_space/srgb/oklab.hrx::gray`）。就是这十位数字卡住了上面的锥拟合。
const OKLAB_LAB: [[f64; 3]; 3] = [
    [0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_9],
    [1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_3],
    [0.025_904_037_1, 0.782_771_766_3, -0.808_675_766_4],
];

/// 以 D65 为基准的 XYZ → `oklab`，它的 LMS 立方根就是全部定义。
fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    let root = apply(&OKLAB_CONE, xyz).map(|value| sp(value, 1.0 / 3.0));
    apply(&OKLAB_LAB, root)
}

fn oklab_to_xyz(oklab: [f64; 3]) -> [f64; 3] {
    let root = apply(&invert(&OKLAB_LAB), oklab);
    apply(&invert(&OKLAB_CONE), root.map(|value| sp(value, 3.0)))
}

/// 直角坐标的 lightness/chroma/hue，色相折进一圈（`lch(… 280.5032243387deg)`）。
///
/// 从 [`super::space`] 可达，因为 `oklab`/`oklch` 与 `lab`/`lch` 是同一个三元组的两种读法：这样一对内部的换算
/// 不得穿过 XYZ，而它的往返正是把一个恰为零的通道变成一个 `±1e-17` 的、再变出一个 `360deg` 色相的东西。
pub(super) fn to_polar(lab: [f64; 3]) -> [f64; 3] {
    let hue = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    [lab[0], chroma, if chroma == 0.0 { 0.0 } else { hue }]
}

pub(super) fn from_polar(lch: [f64; 3]) -> [f64; 3] {
    let angle = lch[2].to_radians();
    [lch[0], lch[1] * angle.cos(), lch[1] * angle.sin()]
}

/// 这个颜色的通道作为以 D65 为基准的 CIE XYZ。一个 `none` 通道读成 `0`，这就是语料对
/// `color(srgb none 0.2 0.3)` 转到 `lab` 所显示的（`to_space/srgb/lab.hrx::missing/red`）；powerlessness 本身只在
/// 一次恒等换算里活着，而打印靠 [`crate::color::InSpace`] 保住它。
pub fn to_xyz_d65(space: Space, channels: [f64; 3]) -> [f64; 3] {
    let white = space.white();
    let xyz = match space {
        // **反向**的那条曲线：这些通道已经是 Lab 了，所以它们要去 XYZ。[`of_xyz_d65`] 里的孪生分支才是
        // 正向的那条。这一对曾在这里被装反过一次（那个分支叫 `xyz_to_lab`，而 `lab_to_xyz` 坐在那里没人用 ——
        // dead-code 警告就是那个发现），而只要读一个 `lab` 颜色，链接的换算就会碰上来：
        // `color.to-space(color.to-space(#abc, lab), srgb)`。
        Space::Lab => lab_to_xyz(channels),
        Space::Lch => lab_to_xyz(from_polar(channels)),
        Space::Oklab => oklab_to_xyz(channels),
        Space::Oklch => oklab_to_xyz(from_polar(channels)),
        Space::Xyz => return channels,
        // 它自己的数，在 D50 上：下面尾部那个 `adapt` 就是这个空间唯一的一次白点移动。
        Space::XyzD50 => channels,
        other => {
            let transfer = other.transfer();
            let decoded = channels.map(|value| transfer.decode(value));
            apply(other.primaries(), decoded)
        }
    };
    adapt(xyz, white, White::D65)
}

/// 把 XYZ（D65）作为这个空间的通道。
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
