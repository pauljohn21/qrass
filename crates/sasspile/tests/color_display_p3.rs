//! 本文件守的律：**`display-p3` 的 primaries 表是归档的，而拟合只能替换它能分辨的部分。**
//!
//! Z 输出行的蓝列两项分别差语料算出的数 `5.0778e-9` 与 `6.8716e-9`，而那一对曾让 165 行红着。
//! `cargo run -q -p sasspile --example calibrate_space -- display_p3_linear xyz` 从
//! `to_space/display_p3_linear/xyz.hrx` 解出它们，把自己的行复现到 `5.0e-11` —— 归档
//! 第十位小数的半步 —— 并印出两个方向间的散布，那是这里单个数字的噪声底。
//!
//! 七项已在此噪声内一致的表项**保留其发表数字**。这不是保守，是量过的：把整张拟合矩阵贴上
//! 而非只贴两项可分辨的，得分 `net=136 regress=0`；从另一侧取的拟合分更高（`net=168`），却在
//! `to_space/display_p3_linear/srgb.hrx::out_of_range/far` 上破了 `regress=0` —— 那行的
//! `-999999` 输入把拟合自身的噪声放大过第十位小数 —— 而受限覆盖是 `regress=0 net=165`。
//! 一个分辨不出某位数字的拟合，不是该位数字错的证据。
//!
//! 下方每个值都抄自具名 case 的 `output.css`，此处每个 case 在改动前红、改动后绿
//! （`ledger failures missing-rectangular-v2 <key>` 对 `… display-p3-two-entries <key>`）。
//! **故意**不钉：`to_space/xyz/display_p3.hrx::gray`，其第三通道归档为 `0.7071783379`、本实现为
//! `0.7071783378` —— 与 `tests/color_adaptation.rs` 点名的三行同一末位天花板，而这张表到不了它。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// XYZ → display-p3：编码侧，蓝列的错过去显在第三通道（`0.9623860263` 对归档的 `0.9623860213`）。
#[tokio::test]
async fn xyz_converts_to_display_p3_on_the_archives_own_numbers() {
    let cases = [
        ("1 1 1", "1.067061664 0.9807650127 0.9623860213"),
        ("0.2 0.4 0.8", "-0.4800324283 0.7723805169 0.8767615059"),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(xyz {input}), display-p3)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(display-p3 {printed})")),
            "for `color(xyz {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 拟合所取自的那对表的正向 —— 线性侧，数字与矩阵之间没有传递曲线。
#[tokio::test]
async fn display_p3_linear_converts_to_xyz_through_the_fitted_column() {
    let cases = [
        ("1 1 1", "0.9504559271 1 1.0890577508"),
        ("0.5 0.5 0.5", "0.4752279635 0.5 0.5445288754"),
        ("0.2 0.4 0.8", "0.3621550952 0.3859198528 0.8532008479"),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(display-p3-linear {input}), xyz)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz {printed})")),
            "for `color(display-p3-linear {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 同一方向的编码源，于是曲线与表一起被检查，而不是逐个。
#[tokio::test]
async fn display_p3_converts_to_xyz_with_the_curve_applied() {
    let built = css("a {b: color.to-space(color(display-p3 0.2 0.4 0.8), xyz)}").await;
    assert!(
        built.contains("color(xyz 0.171095654 0.1473658922 0.6363562895)"),
        "`to_space/display_p3/xyz.hrx::middle` states those three numbers, this compiler answered:\n{built}"
    );
}
