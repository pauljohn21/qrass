//! 把一个 `.hrx` 归档文件读成 `(case, input text, output text)` 三元组 —— 这是
//! `examples/calibrate_space.rs` 里关于**格式**、而非关于拟合的部分，拆出来是因为量具越过了本仓库
//! 500 行的文件上限，而拆点正是两种关注点本已分叉的地方。
//!
//! 它靠 `#[path]` 共享（与 `tests/common/tracing_bridge.rs` 同法）而非做成 crate 模块，所以
//! `src/` 里没有任何东西依赖它。`tests/hrx.rs` 有意保留自己的解析器：它把各节映射成携带
//! 测量夹具所需判定账目的 `Case`（`extra_inputs`、`has_options`、`error`）并返回
//! `PASS`/`FAIL` 行，那是与「给我这条用例的两个文本」不同的问题；合并会让测量夹具
//! 依赖一个校准工具。

use std::collections::BTreeMap;
use std::path::Path;

/// 一对中哪一侧经转移曲线打印。**二元事实，所以做成值**：最初版本把它当 `"from"`/`"to"`
/// 字符串标签在三处按相等性比较，编译期无法把笔误与正确读取区分开。收益正在于此 ——
/// 下面的交换与分支现在可以匹配枚举变体，加第三侧时会编译失败而不是静默落穿。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    From,
    To,
}

/// 受检的曲线假设：一对中哪一侧带曲线、以及用哪个**解码**指数（`linear = printed^exponent`）。
/// 量具从不声言这是归档的律 —— 它在该假设下拟合，再报告结果能否靠归档自己的行分辨出来。
///
/// `offset` 是**假想**的偏移幂转移里的 `α`，解码为
/// `linear = ((printed + α − 1)/α)^exponent`。它存在是因为关于 `rec2020` 的散文正好描述那个形状，
/// 得有人能检验它 —— 检验结论为否（`tasks.md` 9.20：归档的 `rec2020` 是 `α = 1.0`，
/// 即完全没有偏移），所以这个字段是扫描假设的通道，不是对库里任何空间的声言。
/// `None` 表示纯 gamma，`src/color/matrix.rs` 里每个曲面空间实际都是纯 gamma。
#[derive(Clone, Copy, Debug)]
pub struct Hypothesis {
    pub side: Side,
    pub exponent: f64,
    pub offset: Option<f64>,
}

/// 一条归档读数：用例名、输入三元组、打印的输出三元组，以及**同一对三元组在任何线性化之前**。
/// 保留原始对是因为分段转移按打印值决定分支（对照 `src/color/matrix.rs` 里 `prophoto-rgb` 的 `1/16`），
/// 而拟合跑在经线性化的值上 —— 两者缺一个，「分段分支从未被这些行走过」就只是读者的推断，
/// 而非打印出来的事实。
pub struct Reading {
    pub case: String,
    pub input: [f64; 3],
    pub output: [f64; 3],
    pub printed_input: [f64; 3],
    pub printed_output: [f64; 3],
}

/// `<===> <case>/input.scss` … `<===>` —— 归档采用的节约定。裸 `<===>` 后接 `====`
/// 是分组分隔符，不带用例。
pub fn sections(text: &str) -> BTreeMap<String, [String; 2]> {
    let lines: Vec<&str> = text.lines().collect();
    let mut bodies: BTreeMap<String, [String; 2]> = BTreeMap::new();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("<===>"))
        .for_each(|(index, line)| {
            let path = line["<===>".len()..].trim();
            if path.is_empty() || path.starts_with("====") {
                return;
            }
            let (key, file) = match path.rsplit_once('/') {
                Some((key, file)) => (key.to_string(), file),
                None => return,
            };
            let body: String = lines[index + 1..]
                .iter()
                .take_while(|next| !next.starts_with("<===>"))
                .map(|line| format!("{line}\n"))
                .collect();
            let slot = bodies.entry(key).or_default();
            match file {
                "input.scss" => slot[0] = body,
                "output.css" => slot[1] = body,
                _ => {}
            }
        });
    bodies
}

/// `color(<name> ` 之后的三个数；否则（行不完全是数字时）为 `None`。通道列表止于 `)` 或
/// alpha 分隔符 `/`，而 `none` 就是解析失败 —— 无能力通道正是这样从拟合里掉出去，
/// 而不是被读成它不是的某个数。
pub fn triple(text: &str, name: &str) -> Option<[f64; 3]> {
    let (_, tail) = text.split_once(&format!("color({name} "))?;
    let body = tail.split([')', '/']).next()?;
    let channels: Vec<f64> = body
        .split_ascii_whitespace()
        .take(3)
        .map(|token| token.parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()?;
    (channels.len() == 3).then(|| [channels[0], channels[1], channels[2]])
}

/// `color( )` 里写的空间名：目录拼作 `xyz_d50`，值拼作 `xyz-d50`，`srgb_linear` 写作
/// `srgb-linear`。
pub fn space_name(directory: &str) -> String {
    directory.replace('_', "-")
}

/// 一个 `to_space/<from>/<to>.hrx` 的具体行，每行通道按归档书写顺序读：输入侧 `color(<from> …)`、
/// 输出侧 `color(<to> …)`。
///
/// 当一侧经转移曲线打印时，由调用方说明哪一侧、用哪个**解码**指数（`linear = printed^exponent`），
/// 读数在此做线性化 —— 指数是调用方正检验的假设，绝不由本文件声言。
pub fn readings(
    spec: &Path,
    from: &str,
    to: &str,
    file: &str,
    curved: Option<Hypothesis>,
) -> Vec<Reading> {
    let path = spec.join(from).join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    sections(&text)
        .into_iter()
        .filter_map(|(case, [input, output])| {
            let input = triple(&input, &space_name(from))?;
            let output = triple(&output, &space_name(to))?;
            let (printed_input, printed_output) = (input, output);
            // 与 `src/color/matrix.rs` 的 `sp` 同样的带符号幂，且是**故意保留的第二份**：
            // `sp` 是颜色模块的私有件，而本文件必须保持能用库（尚）未声言的指数把一行线性化。
            // 保持诚实用的规矩是 `examples/calibrate_space.rs` 里关于重复 `invert` 的那条 ——
            // 读数可以用它产出，但库里的数字绝不从这里回抄。若 `sp` 与这里有一天不一致，
            // 拟合的行将不再复现，这正是 `verdict` 的用途。
            let power = |channels: [f64; 3], exponent: f64, offset: Option<f64>| {
                channels.map(|channel| {
                    if channel == 0.0 {
                        0.0
                    } else {
                        // 偏移幂在指数之前先移动量级；符号搭在外面两者之上，
                        // 与语料让负通道穿过曲线的方式相同。
                        let moved = match offset {
                            Some(alpha) => (channel.abs() + (alpha - 1.0)) / alpha,
                            None => channel.abs(),
                        };
                        channel.signum() * moved.powf(exponent)
                    }
                })
            };
            let (input, output) = match curved {
                None => (input, output),
                Some(hypothesis) => match hypothesis.side {
                    Side::From => (power(input, hypothesis.exponent, hypothesis.offset), output),
                    Side::To => (input, power(output, hypothesis.exponent, hypothesis.offset)),
                },
            };
            Some(Reading {
                case,
                input,
                output,
                printed_input,
                printed_output,
            })
        })
        .collect()
}

/// 归档的两个参考白，从打印它们的行读出：`to_space/srgb/xyz.hrx::white` 与
/// `to_space/srgb/xyz_d50.hrx::white`。把 D50 那个乘上解出的表，必须落到 D65 那个 ——
/// 正是这个检查抓住了把适应做两次的错误。
pub fn whites(spec: &Path) -> ([f64; 3], [f64; 3]) {
    let one = |file: &str, name: &str| {
        let path = spec.join("srgb").join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        sections(&text)
            .into_iter()
            .find(|(case, _)| case == "white")
            .and_then(|(_, [_, output])| triple(&output, name))
            .unwrap_or_else(|| panic!("{} has no `white` row", path.display()))
    };
    (one("xyz.hrx", "xyz"), one("xyz_d50.hrx", "xyz-d50"))
}
