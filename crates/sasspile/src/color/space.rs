//! 色彩**空间** —— `core_functions/color/to_space/**` 与 `color()` 构造子需要的模型，与
//! [`crate::color`] 那些 srgb 时代的律分开。
//!
//! 为什么是一个独立的 owned 值而不是往 [`crate::color::Representation`] 上再加字段：一个在 `display-p3` 或
//! `oklch` 里的颜色*就是*它的三个通道值加上它们所属的那个空间，所以空间作为颜色上的数据跟着它走
//! （`Color::space`），而每一条既存的打印/读入律对那些没有空间的颜色继续原样工作。`None` 的意思是
//! “这些通道就是结构体早存下的 srgb 族字节 / hsl-hwb 轴” —— 那正是保住 1,690 行已通过的行不被本文件重定一次的根据。
//!
//! 数学是每个空间一张表（primaries 矩阵、传递函数、参考白）加上**唯一一条**路线：每次换算都走以 D65 为基准的
//! XYZ。白点适应是从 Bradford 锥矩阵算出来的，而不是作为一个舍入常数录来的，因为语料打十位有效数字，而一份
//! 手录的矩阵（`0.964295676`）在第十位就已经错了（`color(srgb 1 1 1)` 转到 `xyz-d50` 是
//! `color(xyz-d50 0.9642956764 1 0.8251046025)`，`to_space/srgb/xyz_d50.hrx::white`）。

use super::matrix::{from_polar, of_xyz_d65, to_polar, to_xyz_d65};

/// 本编译器能携带与换算的那些空间。`rgb`/`hsl`/`hwb` 是有意缺席的：那些目标由它们自己已经在打的 legacy
/// [`crate::color::Representation`] 作答，而不是由一个空间通道三元组。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Srgb,
    SrgbLinear,
    DisplayP3,
    DisplayP3Linear,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    Xyz,
    XyzD50,
    Lab,
    Lch,
    Oklab,
    Oklch,
}

/// 一个颜色自己的空间，以及它携带通道值的方式。`None` 就是一个 `none` 通道 —— powerless，那是数据而不是一个哨兵
/// 数字，因为数学总是需要一个数，而答案有时得把 `none` 说回去。两者如何调和是一个只有一答的问题，答案在
/// [`Color::converted`]；这个类型只携数据。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InSpace {
    pub space: Space,
    pub channels: [Option<f64>; 3],
}

impl InSpace {
    /// 数学读法下的那些通道：一个 powerless 通道贡献 `0`，那正是归档算账用的东西
    /// （`to_space/srgb/xyz.hrx::missing/red` 把另两个通道保持在 `0` 给 red 的那些值上）。只保留一种读法，
    /// 因为它的两种拼法已经飘开过一次。
    pub(super) fn filled(self) -> [f64; 3] {
        self.channels.map(super::missing_as_zero)
    }
}

/// 一个空间的数字以哪个白点为基准。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum White {
    D65,
    D50,
}

/// 打印通道与矩阵期望的那个线性光值之间的那条传递曲线。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transfer {
    Linear,
    /// srgb，以及 `display-p3` 用的同一条曲线（`color(display-p3 …)` 的通道）。
    Srgb,
    /// 从打印通道到线性光的纯 gamma `563/256`。写成分数而不是小数，因为舍入后的 `2.19921` 不是归档的指数 ——
    /// 它远到足以移动第十位（`matrix.rs` 的 `A98_GAMMA` 是唯一以代码写下该指数的地方，其余只是散文提及；分开那些
    /// 候选的残差在 `tasks.md` 9.17）。
    A98,
    /// 带 `1/16` 以下线性分段的 gamma `1.8`。
    Prophoto,
    /// 一个普通的带符号 gamma `2.4`，**在编码侧没有偏移也没有线性分段** —— 语料在那侧以 `net=41` 行偏好纯幂而不是
    /// 保住先前的 `4.5×` 两臂（那个扫描、两个 A/B run、以及每个读数到底说哪一侧都在 `matrix.rs` 的
    /// `REC2020_GAMMA` 上；`tasks.md` 9.20）。
    Rec2020,
}

impl Space {
    /// 一个写下的名字所命名的空间。下划线与连字符两种拼法是同一个空间，因为语料的 fixture 文件用前者
    /// （`to_space/srgb/xyz_d50.hrx`）而 CSS 打后者（`color(xyz-d50 …)`），而 Sass 的名字大小写不敏感
    /// （`color/color/no_alpha.hrx::case`）。
    pub fn parse(name: &str) -> Option<Self> {
        let key: String = name.to_ascii_lowercase().replace('_', "-");
        match key.as_str() {
            "srgb" => Some(Self::Srgb),
            "srgb-linear" => Some(Self::SrgbLinear),
            "display-p3" => Some(Self::DisplayP3),
            "display-p3-linear" => Some(Self::DisplayP3Linear),
            "a98-rgb" => Some(Self::A98Rgb),
            "prophoto-rgb" => Some(Self::ProphotoRgb),
            "rec2020" => Some(Self::Rec2020),
            "xyz" | "xyz-d65" => Some(Self::Xyz),
            "xyz-d50" => Some(Self::XyzD50),
            "lab" => Some(Self::Lab),
            "lch" => Some(Self::Lch),
            "oklab" => Some(Self::Oklab),
            "oklch" => Some(Self::Oklch),
            _ => None,
        }
    }

    /// 打印出来的形体所用的名字。
    pub fn css(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::SrgbLinear => "srgb-linear",
            Self::DisplayP3 => "display-p3",
            Self::DisplayP3Linear => "display-p3-linear",
            Self::A98Rgb => "a98-rgb",
            Self::ProphotoRgb => "prophoto-rgb",
            Self::Rec2020 => "rec2020",
            Self::Xyz => "xyz",
            Self::XyzD50 => "xyz-d50",
            Self::Lab => "lab",
            Self::Lch => "lch",
            Self::Oklab => "oklab",
            Self::Oklch => "oklch",
        }
    }

    /// 这个空间的 primaries 矩阵，从本空间的线性光到 XYZ，**在表自己落到的那个白上**。通道已经是（或从）XYZ 派生
    /// 的那些空间是 `None`。那些表与每个拟合数字的出处住在 [`super::primaries`]，那是一个独立的文件，因为这些表
    /// 加上那些说明会把本文件顶过行数上限。
    pub(super) fn matrix(self) -> Option<&'static [[f64; 3]; 3]> {
        super::primaries::table(self)
    }

    /// 换算会把这个空间路由过哪个白点。每个变体都是有意具名的：一个 `_ => White::D65` 的兜底就是本文件在别处
    /// 拒绝的同一个形状（`unwrap_or(&BRADFORD)`，见 [`Self::primaries`]）—— 它会在没人问的情况下为一个以后新增的
    /// 变体作答。
    ///
    /// 对有 primaries 表的那五张表盖到的七个变体（`srgb`/`srgb-linear` 共用一张，`display-p3`/`-linear` 共用一张），
    /// 这**不是一个独立事实**：表自己的行和就是它的白，而归档不经曲线就把它们说出来（`color(<space> 1 1 1)` 对任何
    /// 传递都成立，因为一个幂钉住 0 与 1）。所以 `to_space/*/xyz.hrx::white` 就是这个函数的测试。那些行的逐位数字
    /// 与每张表的出处只住 [`super::primaries`]，这里不复述 —— 它已经复述过一次而留下两个答案：rec2020 转录表
    /// `7e-8` 的行和缺陷早已被拟合行顶掉，所以 `to_space/rec2020/xyz.hrx::white` 现在是绿的，今天红的
    /// 是另一件事（现量 `cargo run -q -p ledger -- failures <run> "to_space/rec2020/xyz" --count` = **5**：
    /// `xyz_d50::{white,middle,out_of_range/near,out_of_range/far}` 与 `xyz::out_of_range/far`，那走的是 D50
    /// 路线上的换算，不是“这里声明的白与表不一致”）。
    pub(super) fn white(self) -> White {
        match self {
            Self::XyzD50 | Self::Lab | Self::Lch => White::D50,
            // 尽管颜料自己的参考白是 D50，`prophoto-rgb` 在这里仍是 D65，因为拟合出的表自己就落在 D65 白上
            // （`to_space/prophoto_rgb/xyz.hrx::white`）；再把它声明成 D50 就是适应第二遍。白点随表走。
            Self::Srgb
            | Self::SrgbLinear
            | Self::DisplayP3
            | Self::DisplayP3Linear
            | Self::A98Rgb
            | Self::ProphotoRgb
            | Self::Rec2020
            | Self::Xyz
            | Self::Oklab
            | Self::Oklch => White::D65,
        }
    }

    /// 一个通道**就是** RGB 型的那个空间的 primaries 表。[`Self::matrix`] 是个 `Option`，因为其余那些
    /// （`xyz`、`xyz-d50`、`lab`、`lch`、`oklab`、`oklch`）在 [`super::matrix`] 里用自己的数学作答，从不走到这里；
    /// 它替掉的兜底（`unwrap_or(&BRADFORD)`）是把白点矩阵当 primaries 用来自答一个缺席的表 —— 那是一个没有任何
    /// fixture 说过的数字，而如果一个变体带着它自己的臂进来，它会一直没人看见。
    pub(super) fn primaries(self) -> &'static [[f64; 3]; 3] {
        self.matrix()
            .expect("a space that reaches the primaries arm has a primaries table")
    }

    pub(super) fn transfer(self) -> Transfer {
        match self {
            Self::Srgb | Self::DisplayP3 => Transfer::Srgb,
            Self::SrgbLinear | Self::DisplayP3Linear => Transfer::Linear,
            Self::A98Rgb => Transfer::A98,
            Self::ProphotoRgb => Transfer::Prophoto,
            Self::Rec2020 => Transfer::Rec2020,
            _ => Transfer::Linear,
        }
    }

    /// **唯一**那一条臂，它把“通道是一个亮度加上一对派生值”的空间（`lab`/`lch`/`oklab`/`oklch`）与“通道是一个
    /// 按下标对齐的 RGB 或 XYZ 三元组”的那些分开。两个推论挂在它上面，而归档把它们各自说出来 —— 这里只列不抄，
    /// 因为两份臂名单就是对同一个划分有两个答案：
    /// * 派生空间用自己的后缀打（[`Self::is_functional`]）；
    /// * 一个 `none` 只有在按下标对齐的那族里，才能**在写下它的那个位置上**活过 `color.to-space` ——
    ///   `to_space/srgb/xyz.hrx::missing/red` 答 `color(xyz none 0.028962782 0.0735618977)`，而
    ///   `to_space/srgb/lab.hrx::missing/red` 把缺的那个通道代入后答
    ///   `lab(19.2964565453% -7.7361660103 -20.488348653)`（[`Color::converted`]）。
    fn is_derived(self) -> bool {
        matches!(self, Self::Lab | Self::Lch | Self::Oklab | Self::Oklch)
    }

    /// 这个空间的通道是不是一个 `color()` 调用里打成裸数字的 `r g b`（或 `x y z`）三元组，而不是带自己后缀的
    /// `lab(…)`/`lch(…)` 形体。
    pub(super) fn is_functional(self) -> bool {
        !self.is_derived()
    }

    /// **满量程百分比**在这个空间的亮度上命名的那个数 —— 即 [`Space::percent_divisor`] 的 `通道 0` 行，那张表
    /// 带着理由与 fixture。它保留自己的名字，因为打印机与 `color(…)` 的读入只看那一个通道；同一个空间的其他通道
    /// 坐在不同的底上（`change/lab.hrx::a/percent` 不在 `lightness` 的尺度上）。
    pub fn percent_full(self) -> f64 {
        super::channels::full(self)
    }

    /// `name` **在这个空间里**的下标，当这个空间没有叫那个名的通道时是 `None`。那些名字与背后的 fixture 在
    /// [`super::channels`]：`change/srgb.hrx` 写 `$red`、`change/xyz.hrx` 写 `$x`、`change/lab.hrx` 写 `$a`、
    /// `change/lch.hrx` 写 `$chroma`。
    pub fn channel(self, name: &str) -> Option<usize> {
        super::channels::index(self, name)
    }

    /// 这个空间的三个通道名，按下标顺序 —— 颜色带着这个空间时一次调用会点名什么（`change/xyz.hrx` 点名 `$x`、
    /// `change/lch.hrx` 点名 `$chroma`；表在 [`super::channels`]）。
    pub fn channel_names(self) -> [&'static str; 3] {
        super::channels::names(self)
    }

    /// 这个空间是否有一个通道是它的色相，而一个实参会以一个角到达它那里。
    pub fn is_hue(self, channel: usize) -> bool {
        super::channels::is_hue(self, channel)
    }

    /// 一个通道上的百分比实参所读的那个除数，或者在归档对那个通道没有说过百分比时是 `None`（那两个色相）。
    /// **逐通道而不是逐空间** —— 那正是 `change/lab.hrx::{lightness,a}/percent` 的发现，同一个空间里它们底不同。
    pub fn percent_divisor(self, channel: usize) -> Option<f64> {
        super::channels::percent_divisor(self, channel)
    }
}

use super::{Channel, Color, Representation};

impl Color {
    /// 一个**在自己空间里**的颜色：空间与它的通道作为一个值在 `Representation::InSpace` 里跟着颜色走（`none`
    /// 继续作为 `None` 存），而字节/hsl-轴那个三元组保持空白，因为这样一个颜色的每一次读入都走 [`Color::xyz`]、
    /// 每一次打印都走 [`super::text::print`]。
    pub fn in_space(space: Space, channels: [Option<f64>; 3], alpha: f64) -> Self {
        let zero = |value: f64| Channel {
            value: Some(value),
            percent: false,
        };
        Self {
            channels: [zero(0.0), zero(0.0), zero(0.0)],
            alpha: crate::color::clamp_alpha(alpha),
            representation: Representation::InSpace(InSpace { space, channels }),
            authored: None,
        }
    }

    /// 颜色的通道，**作为一个能携带 `none` 的三元组**；当它存的是轴时是 `None`。一个 legacy `rgb` 三元组
    /// 就*是*一个 srgb 三元组 —— 这正是 [`Color::converted`] 能为同一个颜色的两种拼法只读一个来源的全部原因 ——
    /// 而一个 `hsl`/`hwb` 颜色还存不下 `none`（它们的构造子仍拒绝一个，而 `tasks.md` 9.23 点名了等它的行），
    /// 所以它们在这里答 `None`，而不是声称一个没东西持有的事实。**那个划分唯一一份穷举列出**：换算在两处问它，
    /// 而一个带条件的 `if` 不可能穷举，所以集合在这里只命名一次，而不是被重新打一遍。
    fn triple(&self) -> Option<[Option<f64>; 3]> {
        match self.representation {
            Representation::InSpace(carried) => Some(carried.channels),
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => Some(self.channels.map(|channel| channel.value)),
            Representation::Hsl | Representation::Hwb => None,
        }
    }

    /// 以 D65 为基准的 CIE XYZ —— 任意两个空间之间唯一的那条路线。
    ///
    /// 一个空间颜色按上面那张表换算。一个 legacy 颜色按 srgb 读：一个 `hsl`/`hwb` 颜色先过它存的轴，一个
    /// `rgb` 族的颜色过它的字节，而两者都是 srgb 传递曲线的*编码后*通道，这就是为何 `#3366cc` 与
    /// `color(srgb 0.2 0.4 0.8)` 换算到同一个 `lab`（`to_space/srgb/lab.hrx::middle`）。
    pub fn xyz(&self) -> [f64; 3] {
        let bytes = match self.representation {
            Representation::InSpace(carried) => {
                return to_xyz_d65(carried.space, carried.filled());
            }
            // 这两个读法各自过那个说出怎么读的唯一 helper（`stored_axes` 读 `hsl` 写下的东西，`hwb_axes` 读一个
            // `hwb` 颜色蕴含的推导）—— 一个携带空间的颜色到不了这里，所以永远不会把一个填零的三元组当字节读。
            Representation::Hsl => {
                let [hue, saturation, lightness] = self.stored_axes();
                crate::axes::axes_to_rgb(hue, saturation, lightness)
            }
            Representation::Hwb => {
                let [hue, saturation, lightness] = self.hwb_axes();
                crate::axes::axes_to_rgb(hue, saturation, lightness)
            }
            // 四个存字节的表示，列出来是因为一个守卫形式（`writes_bytes`）自己不可能穷举 —— 这正是为何*换算*要问
            // 两次的那个划分住在 [`Color::triple`]，而不是在这里再打一遍。
            Representation::Legacy
            | Representation::Percent
            | Representation::Hex
            | Representation::Named => self.channels.map(|channel| channel.byte()),
        };
        to_xyz_d65(Space::Srgb, bytes.map(|byte| byte / 255.0))
    }

    /// 这个颜色在 `space` 里的通道，给 `color.to-space` 用。一个 `none` 是数据而不是哨兵：数学永远把它读成 `0`
    /// （[`InSpace::filled`]），而**答案**打些什么由本函数底部那条唯一规则定 —— 按下标对齐的那族把它留在写下它的
    /// 位置上，其余一律代入。
    pub fn converted(&self, space: Space) -> Self {
        // **同空间同数字。**一次恒等换算既不得重取整一个通道，也不得靠一次矩阵往返去决定哪些曾经是 `none`：
        // `to_space/xyz_d50/xyz_d50.hrx::missing/x` 逐字保住 `color(xyz-d50 none 0.2 0.3)`，而把那个三元组跑两遍
        // XYZ 正是把它变成发明出的数字的东西。
        let carried = match self.representation {
            Representation::InSpace(carried) => Some(carried),
            _ => None,
        };
        if let Some(existing) = carried {
            if existing.space == space {
                return self.clone();
            }
        }
        // **矩形/极坐标一对就是同一个三元组，不是一次换算。**`oklab` ↔ `oklch` 与 `lab` ↔ `lch` 共用每一个通道，
        // 只在后两个怎么写上不同，所以不涉及任何矩阵。归档直接把这点显示出来：`to_space/oklab/oklch.hrx::missing/b`
        // 写 `color.to-space(oklab(10% 0.2 none), oklch)` 而答 `oklch(10% 0.2 0deg)` —— 亮度与彩度逐位携过去。
        // 把它穿过 XYZ 无法复现：那次往返会把一个恰为零的通道留在 ±1e-17，而一个负的余数让 `atan2` 折到
        // `359.999…deg`，打出来就是 `360deg`。那就是修正 cone 表时一行变红的来历。
        let polar_pair = |carried: InSpace| {
            let triple = carried.filled();
            match (carried.space, space) {
                (Space::Oklab, Space::Oklch) | (Space::Lab, Space::Lch) => Some(to_polar(triple)),
                (Space::Oklch, Space::Oklab) | (Space::Lch, Space::Lab) => Some(from_polar(triple)),
                _ => None,
            }
        };
        let channels = match carried.and_then(polar_pair) {
            Some(triple) => triple,
            None => of_xyz_d65(space, self.xyz()),
        };
        // 源侧那些未书写的位置，对两种载体都按同一个方式读（那个划分命名在 `Color::triple`）。一个 legacy `rgb`
        // 三元组参与同一条按下标对齐的律，因为它*就是*一个 srgb 三元组：`to_space/rgb/srgb.hrx::missing/red` 打
        // `color(srgb none 0.0784313725 0.1176470588)`，与一个写下的 `color(srgb none …)` 一样。
        let source = self.triple();
        // **按下标对齐的那族把自己的 `none` 留在写下它的位置上。**缺的那个通道仍向数学贡献 `0`，而归档另两个通道
        // 就是 `0` 产生的那两个数 —— `to_space/xyz/srgb.hrx::missing/x` 打
        // `color(srgb none 0.6558430379 0.5622938017)`。其余任何一对都代入，于是同一个三元组变成三个数；
        // 在进一个矩形空间的路上那个代入是绿的（`to_space/lab/srgb.hrx::missing/lightness`），而在进
        // `lab`/`oklab` 的路上那些行仍红在第十位小数，那天花板是 `tasks.md` 9.14 记下的，不是这条律失败。
        let same_family = match (carried, self.triple()) {
            (Some(existing), _) => !space.is_derived() && !existing.space.is_derived(),
            // 没有空间三元组，但有一个字节三元组：那仍是一个 srgb 颜色。
            (None, Some(_)) => !space.is_derived(),
            // 一个 `hsl`/`hwb` 颜色的轴与这次换算能打出的任何东西都不按下标对齐。
            (None, None) => false,
        };
        // 每一个通道都是 `none` 就是一个完全没有 power 的颜色（powerless），而在进一个空间时它仍保持 powerless
        // （`to_space/srgb/lab.hrx::missing/all` 答 `lab(none none none)`，而 `rgb(none none none)` 过同一个读法
        // 到达同一个答案 —— `to_space/rgb/lab.hrx::missing/all`）；一个 legacy `hsl` 目标没有办法打那个，于是代入
        // （`to_space/srgb/hsl.hrx::missing/all` 答 `hsl(0, 0%, 0%)`）。
        let powerless = source.is_some_and(|given| given.iter().all(Option::is_none));
        let triple = match source {
            Some([first, second, third]) if same_family => {
                let [low, mid, high] = channels;
                [first.map(|_| low), second.map(|_| mid), third.map(|_| high)]
            }
            _ => channels.map(|value| (!powerless).then_some(value)),
        };
        Self::in_space(space, triple, self.alpha)
    }

    /// 这个颜色携带的那个空间，如果它携带一个。`color.change` 先问这个：一个**在自己空间里**的颜色是在那个空间的
    /// 通道上被书写，不是写在字节或 hsl 轴上（`change/srgb.hrx::red/unitless` 经 `$red` 写 `color(srgb 0.2 0.5 0.7)`）。
    pub fn carried_space(&self) -> Option<Space> {
        match self.representation {
            Representation::InSpace(carried) => Some(carried.space),
            _ => None,
        }
    }

    /// 一个携带自己空间的颜色的那一个通道，按调用点名的方式写入：`None` 就是归档的那个 `none`
    /// （`change/oklab.hrx::lightness/none` → `oklab(none 0.2 -0.3)`、
    /// `change/lch.hrx::chroma/none` → `lch(50% none 30deg)`），而没有任何东西被夹紧 ——
    /// `change/xyz.hrx::x/out_of_range` 保住它收到的 `1.2`，`change/oklab.hrx::a/out_of_range` 保住 `1`。
    ///
    /// **一个负的彩度不会被存成负数**：`color.change(lch(50% 20 30deg), $chroma: -10)` 答
    /// `lch(50% 10 210deg)`（`change/lch.hrx::chroma/negative`）—— 取幅值，色相折半圈 —— 而
    /// `color.change(oklch(50% 0.2 30deg), $chroma: -0.1)` 答 `oklch(50% 0.1 210deg)`
    /// （`change/oklch.hrx::chroma/negative`）。两边折法相同，所以一臂服务这一对；其他通道都不折
    /// （`lab` 与 `oklab` 的负轴保持为负，按它们各自的 `*/out_of_range` 行）。
    pub fn with_space_channel(&self, channel: usize, value: Option<f64>) -> Option<Self> {
        let carried = match self.representation {
            Representation::InSpace(carried) => carried,
            _ => return None,
        };
        let mut channels = carried.channels;
        // 一个写下的 `-0` 以 `0` 回来，与字节夹紧做的是同一个折法
        // （`color.change(lab(50% 0 0), $a: -0)` → `lab(50% 0 0)`，`change/unusual_numbers::negative_zero`）。
        channels[channel] = value.map(super::channel::without_negative_zero);
        let hue = match (carried.space, channel) {
            // 两个极坐标空间把彩度命名在下标 1，而把自己的色相携在下标 2。
            (Space::Lch | Space::Oklch, 1) => value.is_some_and(|chroma| chroma < 0.0),
            _ => false,
        };
        if hue {
            channels[1] = value.map(|chroma| chroma.abs());
            // 色相转过半圈再回到一圈之内，这与空间自己的色相实参从 `angle_of` 得到的那个折法是同一个
            // （`change/lch.hrx::hue/negative` 把 `-20deg` 读成 `340deg`）。一个未书写的色相继续未书写：
            // 无东西的 `-10` 没有可转的角。
            channels[2] = channels[2].map(|degrees| (degrees + 180.0).rem_euclid(360.0));
        }
        Some(Self::in_space(carried.space, channels, self.alpha))
    }

    /// 这个颜色作为一个 **legacy srgb** 颜色，那正是 `color.to-space($c, rgb)` 要的。形体是 hex 那个，因为那
    /// 就是语料打的：`color(srgb 0.2 0.4 0.8)` 到 `rgb` 是 `#3366cc`（`to_space/srgb/rgb.hrx::middle`），
    /// 而一个小数通道在 [`crate::color::text`] 已有的那条律下退到百分比。
    pub fn as_legacy_rgb(&self) -> Self {
        // **恒等保住照写的东西。**一个已经存着 srgb 字节、且带着未书写通道的颜色，不得在出口路上把那个通道代入：
        // 语料要 `color.to-space(rgb(none 20 30), rgb)` 而打 `rgb(none 20 30)`（`to_space/rgb/rgb.hrx::missing/red`），
        // 而同一个调用作用在一个*空间*颜色上（`to_space/srgb/rgb.hrx::missing/red`）打 `rgb(0%, 20%, 30%)` ——
        // 那里那个 `none` 属于空间三元组而 legacy 目标代入，所以这两行合起来才说出：这道守卫是按表示定键的，
        // 不是只按缺不缺。
        if self.has_unwritten_channel() {
            return self.clone();
        }
        let bytes = of_xyz_d65(Space::Srgb, self.xyz()).map(|channel| channel * 255.0);
        Self {
            channels: bytes.map(|value| Channel {
                value: Some(value),
                percent: false,
            }),
            alpha: self.alpha,
            representation: Representation::Hex,
            authored: None,
        }
    }

    /// 这个颜色作为一个 legacy 的 **hsl** 颜色：`color(srgb 0.2 0.4 0.8)` 到 `hsl` 是 `hsl(220, 60%, 50%)`
    /// （`to_space/srgb/hsl.hrx::middle`），即已有的打印机已经在产的那个逗号形。
    pub fn as_legacy_hsl(&self) -> Self {
        let bytes = of_xyz_d65(Space::Srgb, self.xyz()).map(|channel| channel * 255.0);
        let [hue, saturation, lightness] = crate::axes::rgb_to_axes(bytes[0], bytes[1], bytes[2]);
        Self {
            channels: [
                Channel {
                    value: Some(hue),
                    percent: false,
                },
                Channel {
                    value: Some(saturation),
                    percent: true,
                },
                Channel {
                    value: Some(lightness),
                    percent: true,
                },
            ],
            alpha: self.alpha,
            representation: Representation::Hsl,
            authored: None,
        }
    }
}
