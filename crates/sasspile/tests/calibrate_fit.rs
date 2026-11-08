//! 仪器自身的数学，脱离档案库进行测试。
//!
//! `tests/common/fit.rs` 因行数预算从 `examples/calibrate_space.rs` 拆出，其头部
//! 声称拆分能让数学被独立测试——但只有某个目标把它纳入测试后才成立，而此前没有任何
//! 目标这样做。本文件就是那样一个目标。它重要在于：本仓库粘贴进 `color/primaries.rs`
//! 的每一个拟合数字都出自这些函数：如果 `solve` 或 `verdict` 被悄悄写错，那些表格
//! “测量而来，而非抄录”的论证就会落空。
//!
//! 两个方向都有意覆盖：一个应被接受的拟合，和一个应被拒绝的拟合。只会永远说
//! `true` 的工具无法区分被证伪的假设与未被检验的假设，这正是 `tasks.md` 9.17
//! 为“打印一张貌似合理的表格却对错误曲线零退出”的仪器版本记录的失败。

// 两个模块都整体引入，因此只有 `examples/calibrate_space.rs` 触及的项（档案库读取器、
// `smallest_linear`、`Side::To` 拟合、一个假设的指数）在**本目标**内不可达。死代码分析
// 是按编译单元而非按仓库进行的；另一种选择——在此处放每个模块的第二份拷贝——
// 正是本仓库已两次吃亏的重复真相错误。
#[allow(dead_code)]
#[path = "common/fit.rs"]
mod fit;
#[allow(dead_code)]
#[path = "common/hrx_read.rs"]
mod hrx_read;

use crate::fit::{apply, invert, residual, smallest_curved_printed, solve, verdict};
use crate::hrx_read::{Hypothesis, Reading, Side};

/// 合成档案的一行：名称、三个输入、三个输出。
fn row(case: &str, input: [f64; 3], output: [f64; 3]) -> Reading {
    Reading {
        case: case.into(),
        input,
        output,
        printed_input: input,
        printed_output: output,
    }
}

/// `solve` 读取的行，矩阵在此处选取使答案已知：`white` (`1,1,1`) 给出行的和，
/// `out_of_range/far` 直接给出一列，`out_of_range/near` 分离剩下两列。
fn skeleton(table: &[[f64; 3]; 3]) -> Vec<Reading> {
    [
        ("white", [1.0, 1.0, 1.0]),
        ("out_of_range/near", [-1.0, 0.4, 2.0]),
        ("out_of_range/far", [-999_999.0, 0.0, 0.0]),
    ]
    .into_iter()
    .map(|(case, input)| row(case, input, apply(table, input)))
    .collect()
}

const TESTED: [[f64; 3]; 3] = [[0.4, 0.3, 0.3], [0.2, 0.7, 0.1], [0.0, 0.1, 0.9]];

#[test]
fn a_relation_that_is_the_tested_matrix_solves_back_to_it() {
    let rows = skeleton(&TESTED);
    let solved = solve(&rows);
    // 行以全精度生成，所以拟合精确到浮点舍入，而不是档案库的
    // 第十位小数——这正是使下面的 `verdict` 对此输入有意义之处。
    (0..3).for_each(|r| {
        (0..3).for_each(|c| {
            assert!(
                (solved[r][c] - TESTED[r][c]).abs() < 1e-12,
                "entry ({r},{c}): solved {:?}, built {:?}",
                solved[r][c],
                TESTED[r][c]
            );
        })
    });
    assert!(verdict(&solved, &rows).0, "a correct fit must be accepted");
}

#[test]
fn a_hypothesis_that_does_not_describe_the_rows_is_refuted_not_reported() {
    // 由一个矩阵造出的行，在关于另一个矩阵的声明下拟合：接受上面拟合的同一台仪器
    // 必须对此判否，并在判否时点名最差的一行。
    let rows = skeleton(&TESTED);
    let other = [[0.9, 0.05, 0.05], [0.1, 0.8, 0.1], [0.0, 0.2, 0.8]];
    let fitted = solve(&rows);
    let (holds, worst) = verdict(&other, &rows);
    assert!(
        !holds,
        "a matrix that is not the rows' relation must be refused"
    );
    assert!(
        worst > 1.0,
        "its ratio must exceed the resolution bound, got {worst}"
    );
    // 以及另一侧的对照：真实关系通过同一调用。
    assert!(
        verdict(&fitted, &rows).0,
        "the accepted fit must not be refused by the same check"
    );
    let (case, absolute, ..) = residual(&other, &rows);
    assert_ne!(case, "<no rows>", "a refuted fit still names its worst row");
    assert!(
        absolute > 1e-3,
        "the miss it reports should be large, got {absolute}"
    );
}

#[test]
fn the_inverse_direction_of_a_fit_agrees_with_the_forward_one() {
    // 仪器为何打印逐项离散量：对同一关系的两次独立读数是知道单个数字能移动多少的
    // 唯一方式，而这个数字就是推翻抄录数字的门槛（`tasks.md` 9.16）。这里两次读数
    // 都来自精确算术，所以它们必须一致。
    let rows = skeleton(&TESTED);
    let forward = solve(&rows);
    let back = invert(&forward);
    let unit = apply(&back, apply(&forward, [1.0, 2.0, 3.0]));
    (0..3).for_each(|index| {
        assert!(
            (unit[index] - [1.0, 2.0, 3.0][index]).abs() < 1e-12,
            "round-tripping through the inverse drifted: {unit:?}"
        );
    });
}

/// 曲线侧探针，在不从档案库读取的行上：它必须完全忽略线性侧，在无侧为曲线时
/// 什么都不报告，并为被点名的侧作答——而不是为碰巧跨两侧最小的通道作答
/// （`tasks.md` 9.18 记录过一个把它们混在一起、读起来像覆盖的版本）。
#[test]
fn the_smallest_channel_probe_follows_the_side_that_carries_the_curve() {
    let rows = [row(
        "white",
        [0.25, 0.5, 1.0],
        [0.002_961_599_999_999_999_7, 0.2, 0.8],
    )];
    let curved_input = Some(Hypothesis {
        side: Side::From,
        exponent: 2.4,
        offset: None,
    });
    assert_eq!(
        smallest_curved_printed(&rows, curved_input),
        Some(0.25),
        "the curved side's own printed minimum, not the other side's 0.00296"
    );
    assert_eq!(
        smallest_curved_printed(&rows, None),
        None,
        "no curve, no segment"
    );
}
