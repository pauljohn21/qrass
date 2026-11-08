//! 校准量具的纯数学部分 —— 从 `examples/calibrate_space.rs` 拆出，因为那个文件已到本仓库 500 行上限的
//! 491 行，而拆点正是量具自己用词划出的那条：**读归档**（`hrx_read.rs`）、**求解与判定**（本文件）、
//! **解析参数并打印**（示例）。这里不碰文件系统，也不对任何色彩空间作声言：每个常数要么是调用方
//! 传进来的受检指数，要么是从归档打印方式推出的分辨率界。
//!
//! 把数学单独放还意味着它能在不跑命令的情况下测试；那几个界常数尤其决定了拟合出的数字能否替换
//! 转录来的数字，而这个决定正是 `tasks.md` 9.14 到 9.20 每次订正的主题。

use crate::hrx_read::{Hypothesis, Reading, Side};

/// 拟合自身的噪声能比归档分辨率高出多少，才必须判定其背后的假设被推翻。每行的下限取两者中较粗的：
/// 归档第十位小数的半步、以及它打印的值的浮点步长；`16.0` 的余量是那个下限乘以**实测**的
/// 同一拟合对镜像的最坏逐项散布（`1.6e-11` 半步上的 `2.6e-10` ≈ 16），也就是单个良态数字
/// 能漂移的量。被推翻的假设以非零码退出，所以只读数字的人无法把它贴进表。
pub const RESOLUTION_HALF_STEP: f64 = 5e-11;
pub const SPREAD_HEADROOM: f64 = 16.0;

/// 一条输出行的第一个表项，直接取自 `out_of_range/far` 行 —— 它的输入按归档自己的打印是
/// `(-999999, 0, 0)`（一侧被线性化时则是其线性化等价）：单一非零通道意味着打印输出**就是**
/// 表项乘输入，所以相除能在该行的完整相对精度下还原表项。`1e6` 量级的输入也正是 far 行（而不是
/// `white`）对该列约束最紧的原因：归档自己的误差是它最后一位打印数字里的一步，
/// 除以 `1e6` 后在表项上缩到约 `1.2e-16` 的相对量级。
pub fn entry_one(row: &Reading, index: usize) -> f64 {
    assert!(
        row.input[1] == 0.0 && row.input[2] == 0.0,
        "the `out_of_range/far` row is only a column by itself when its other two channels are 0; got {:?}",
        row.input
    );
    row.output[index] / row.input[0]
}

/// 一条输出行的另外两个表项，取自 `white` 与 `out_of_range/near` 行。选这两行是出于**条件数**，
/// 不是图方便：第一表项已知时，`white` 给出 `b + c`（无论怎么打印其输入都是 `(1,1,1)`，
/// 因为曲线把 0 和 1 固定住），`near` 给出 `n₁·b + n₂·c`，所以两者只能分离到 `n₂ − n₁` 允许的
/// 程度 —— 归档的 `(-1, 0.4, 2)` 是 `1.6`。保留一般形式是因为**线性化**后的 `near` 输入
/// 不再是那个三元组。`(0.1 0.2 0.3)`/`(0.2 0.4 0.8)` 这一对（最初尝试用的就是它）的
/// `n₂ − n₁ = 0.04`，归档的 `5e-11` 打印舍入被它除穿，把 `1.25e-9` 压到一个每个效应都按
/// 第十位小数读的表项上；它产出的表在量级为一的行上正好错过那位小数。所以有下面的下限：
/// 它拒掉 `0.04`，保留 `1.6`。
pub fn row_of(white: &Reading, near: &Reading, far: &Reading, index: usize) -> [f64; 3] {
    let first = entry_one(far, index);
    let sum = white.output[index] - first;
    let &[n0, n1, n2] = &near.input;
    let rest = near.output[index] - first * n0;
    let denominator = n2 - n1;
    assert!(
        denominator.abs() >= 1.0,
        "`white` and `out_of_range/near` separate the last two entries by only {denominator}, which would \
         spread the archive's {RESOLUTION_HALF_STEP} print rounding over them by {}× — too ill-conditioned to read a digit from",
        1.0 / denominator.abs()
    );
    let second = (sum * n2 - rest) / denominator;
    let third = (rest - n1 * sum) / denominator;
    [first, second, third]
}

/// 解出的矩阵，一次一条输出行：归档的 `out_of_range/far` 给出第一表项，`white` 与
/// `out_of_range/near` 给出另外两个，其余每条具体行留给 [`residual`] 去核对。
///
/// 行是按**用例名**找到的，而不是按字面三元组，因为线性化后的通道已不是文件里打印的数字
/// （最初尝试查找 `[-1.0, 0.4, 2.0]`，在 `a98-rgb` 上正是因为这一点 panic）。**并非**
/// 每个 `to_space/**` 文件都带这三个用例：`to_space/xyz_d50/a98_rgb.hrx` 只有 `in_range`、
/// `out_of_range` 与 `missing/*`，而它的 `in_range` 在两种 xyz-d50 颜色之间换算，
/// 所以根本不含 xyz_d50→a98-rgb 的关系。因此 panic 消息会打印它确实找到的用例，
/// 缺一个说明配对错了，不是归档坏了。
pub fn solve(rows: &[Reading]) -> [[f64; 3]; 3] {
    let one = |case: &str| {
        rows.iter().find(|row| row.case == case).unwrap_or_else(|| {
            panic!(
                "no `{case}` row in this file; its cases are {:?} — if the file is a different relation, the pair is wrong",
                rows.iter()
                    .map(|row| row.case.as_str())
                    .collect::<Vec<&str>>()
            )
        })
    };
    let (white, near, far) = (
        one("white"),
        one("out_of_range/near"),
        one("out_of_range/far"),
    );
    std::array::from_fn(|index| row_of(white, near, far, index))
}

/// 给定拟合所依据的假设，拟合的行是否在归档可表达的范围内被复现。这是一个**判定字段**，
/// 不是供人眼估的数字，因为量具两种情况下都会打印出看起来合理的矩阵：曲面族上错误的指数
/// （`rec2020` 按 `2.2`，最坏残差 `0.19`）与成立的假设（`a98-rgb` 按 `563/256`，
/// 最坏 `5.0e-11`）在同一个字段名上相差八个数量级，而这份输出早先的形状没有任何东西告诉读者
/// 手里拿的是哪一种。每通道的界取两者中较粗的：归档的第十位小数半步、以及它打印的值的
/// 浮点步长，再乘 [`SPREAD_HEADROOM`]。
pub fn verdict(matrix: &[[f64; 3]; 3], rows: &[Reading]) -> (bool, f64) {
    rows.iter().fold((true, 0.0_f64), |(holds, worst), row| {
        let (worst_here, holds_here) = apply(matrix, row.input)
            .into_iter()
            .zip(row.output)
            .map(|(predicted, printed)| {
                let floor = RESOLUTION_HALF_STEP.max(printed.abs() * f64::EPSILON * 2.0);
                let ratio = (predicted - printed).abs() / (floor * SPREAD_HEADROOM);
                (ratio, ratio <= 1.0)
            })
            .fold((0.0_f64, true), |seen, here| {
                (seen.0.max(here.0), seen.1 && here.1)
            });
        (holds && holds_here, worst.max(worst_here))
    })
}

/// 只在曲面一侧的最小非零**打印**通道。分段转移函数按自己通道的打印值决定分支
/// （`src/color/matrix.rs` 里 `prophoto-rgb` 对 `1/16`），所以是它 —— 不是两侧合并的数、
/// 也不是线性化后的值 —— 回答这些行有没有真正走到分段分支。两侧都不曲时返回 `None`：
/// 没有分段可走。（最初版本合并两侧，报出 prophoto 的 `0.0276` —— 那是 *XYZ 输出*，
/// 而归档里作为源打印的 prophoto 通道从不落到 `0.1` 以下。）
pub fn smallest_curved_printed(rows: &[Reading], curved: Option<Hypothesis>) -> Option<f64> {
    let minimum = rows
        .iter()
        .flat_map(move |row| match curved {
            Some(hypothesis) if hypothesis.side == Side::From => row.printed_input,
            Some(_) => row.printed_output,
            None => [0.0; 3],
        })
        .map(f64::abs)
        .filter(|magnitude| *magnitude > 0.0)
        .fold(f64::INFINITY, f64::min);
    (curved.is_some())
        .then_some(minimum)
        .filter(|_| minimum.is_finite())
}

/// 拟合实际求解的线性化空间里的最小非零通道。与住在这一侧的分段阈值（像 `1/512` 或 `0.081`
/// 这样的编码臂）比较时，回答问题的是这个字段；打印值不是。
pub fn smallest_linear(rows: &[Reading]) -> f64 {
    rows.iter()
        .flat_map(|row| row.input.into_iter().chain(row.output))
        .map(f64::abs)
        .filter(|magnitude| *magnitude > 0.0)
        .fold(f64::INFINITY, f64::min)
}

/// 3×3 矩阵作用于一个向量。
pub fn apply(matrix: &[[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| {
        (0..3)
            .map(|column| matrix[row][column] * vector[column])
            .sum::<f64>()
    })
}

/// 一组行上最坏的 `|predicted - printed|`，**按通道**报告，同时带上同一通道的相对误差与打印量级。
/// 若两个最大值各自独立取，`-999999` 通道上的 `1e-5` 误差就能借用到 `0.5` 通道的相对误差 ——
/// 一个舍入故事被讲成另一个。归档打印为 `0` 的通道没有可报的相对误差，所以该行按绝对误差命名、
/// 份额留 `0`：那个字段里的 `inf` 读起来像测量值，其实不是。
pub fn residual(matrix: &[[f64; 3]; 3], rows: &[Reading]) -> (String, f64, f64, f64) {
    rows.iter()
        .flat_map(|row| {
            apply(matrix, row.input)
                .into_iter()
                .zip(row.output)
                .map(move |(predicted, printed)| {
                    let error = (predicted - printed).abs();
                    let magnitude = printed.abs();
                    (
                        row.case.clone(),
                        error,
                        if magnitude == 0.0 {
                            0.0
                        } else {
                            error / magnitude
                        },
                        magnitude,
                    )
                })
        })
        .fold(
            (String::from("<no rows>"), 0.0, 0.0, 0.0),
            |seen, channel| if channel.1 > seen.1 { channel } else { seen },
        )
}

/// 用行列式的代数余子式求 3×3 逆，好让量具把一次拟合的两个方向对齐、打印它们逐项的散布。
/// 库自己以同样方式算逆（`matrix::invert`，是它的私有件）；这一份只为比较读数而存在，
/// 绝不产出库随后从另一方向照抄的表。
pub fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let cofactor = |row: usize, column: usize| -> f64 {
        let rest = |skip: usize| -> Vec<usize> { (0..3).filter(|index| *index != skip).collect() };
        let (rows, columns) = (rest(row), rest(column));
        let minor = m[rows[0]][columns[0]] * m[rows[1]][columns[1]]
            - m[rows[0]][columns[1]] * m[rows[1]][columns[0]];
        let sign = if (row + column) % 2 == 0 { 1.0 } else { -1.0 };
        sign * minor
    };
    let whole = det(*m);
    // 行列式上的伴随阵：表项 (i, j) 是 (j, i) 的代数余子式。
    std::array::from_fn(|i| std::array::from_fn(|j| cofactor(j, i) / whole))
}

/// 按行写法的 3×3 行列式。
pub fn det(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}
