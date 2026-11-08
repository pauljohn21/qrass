//! 本文件守的律：**白点适应是归档自己的矩阵，不是一份转录。**
//!
//! `color/matrix.rs` 曾把 Bradford 表从 CSS Color 4 散文照抄（`0.9555766 -0.0230393 0.0631636 …`）。
//! 语料算的不是它：归档的 D50→D65 行和是 `0.9956342097 1.0026671299 1.3221722918`
//! （`to_space/xyz_d50/xyz.hrx::white`），而抄表行和是 `0.9957009 0.9993688 1.3289869`——最差项差
//! `6.9e-3`（`1.3303659262` 对 `1.3372652`）。一个中性色几乎看不出来；凡够到 D50 的换算都看得出，
//! 而每个 `lab`、`lch` 与 `xyz-d50` 换算都够得到。替代者是**从归档解出**的：
//! `cargo run -q -p sasspile --example calibrate_space` 从磁盘读 `to_space/xyz_d50/xyz.hrx`，第一列取自
//! `out_of_range/far`（`-999999` 输入让该项可分辨到 `5e-17`），另两列取自 `white` 与
//! `out_of_range/near` 这一对（行列式 `1.6`），然后报告求解未用到的行上的最差残差：`5.0e-11`，
//! 恰是归档第十位小数的半个步进。
//!
//! 本文件**故意**不钉的，及其实测理由：库里只写下了一个方向，故 `to_space/xyz/xyz_d50.hrx` 的派生侧
//! 仍有两行红——`gray`，归档 `0.5014942182` 对本实现 `0.5014942183`（绝对 `1e-10`）；
//! `out_of_range/far`，其最差通道 `−29627.7791422469` 对本实现 `−29627.7791449956`（绝对 `2.75e-6`，
//! 相对 `9.3e-11`）。`to_space/srgb/xyz_d50.hrx::white` 同样红：归档印
//! `color(xyz-d50 0.9642956764 1 0.8251046025)`，我们印 `0.9642956765`。三者都差在第十位小数的
//! 半个单位，而这正是按十位小数行拟合的表可以预期的落点；它们点名在
//! `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md`，而不是抄成断言——钉住我
//! 自己的答案会把它冻成要求。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// 表所拟合的方向：`to_space/xyz_d50/xyz.hrx`。该文件的每条具体行都在此断言——求解用掉其中三条，
/// 故其余各行是独立证据，且重抄错一位数字就会在第十位小数上失败。该文件的 `missing/*` 行不在
/// 此处：它们要的是另一种能力（一个能挺过换算的 `none`），不是这些数字。
#[tokio::test]
async fn xyz_d50_converts_to_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("0 0 0", "0 0 0"),
        ("1 1 1", "0.9956342097 1.0026671299 1.3221722918"),
        ("0.5 0.5 0.5", "0.4978171049 0.501333565 0.6610861459"),
        ("0.2 0.4 0.8", "0.2324626969 0.4151573703 1.0585524842"),
        ("-1 0.4 2", "-0.8381943171 0.474450751 2.6402147779"),
        (
            "-999999 0 0",
            "-955472.4660146538 28369.6809641542 -12314.0025504671",
        ),
        (
            "0.1 0.2 0.3 / 0.4",
            "0.1099054241 0.205474541 0.3962396495 / 0.4",
        ),
        (
            "0.1 0.2 0.3 / 0.0",
            "0.1099054241 0.205474541 0.3962396495 / 0",
        ),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(xyz-d50 {input}), xyz)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz {printed})")),
            "for `color(xyz-d50 {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 库里派生的方向（上表的 `invert`），经 `to_space/xyz/xyz_d50.hrx` 读出。该文件八条具体行中有六条
/// 能在归档的分辨率下复现；`gray` 与 `out_of_range/far` 由红行自己按住，理由在本文件头部陈述。
#[tokio::test]
async fn xyz_converts_to_xyz_d50_through_the_computed_inverse() {
    let cases = [
        ("0 0 0", "0 0 0"),
        ("1 1 1", "1.0206843969 1.0029884365 0.7576864323"),
        ("0.2 0.4 0.8", "0.1786108937 0.3884402932 0.6056728936"),
        ("-1 0.4 2", "-1.1391355769 0.3323983638 1.5190136801"),
        (
            "0.1 0.2 0.3 / 0.4",
            "0.0943246735 0.1959275265 0.2276490187 / 0.4",
        ),
        (
            "0.1 0.2 0.3 / 0.0",
            "0.0943246735 0.1959275265 0.2276490187 / 0",
        ),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(xyz {input}), xyz-d50)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz-d50 {printed})")),
            "for `color(xyz {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 这一行表明转录是错的、而不只是不精，且那个读数是在旧数字还在位时实测的：用 CSS Color 4 表时，
/// `color(xyz-d50 1 1 1)` 曾答 `color(xyz 0.9957009 0.9993688 1.3289869)`——三个通道全错，最差
/// `6.8e-3`——而归档的 `to_space/xyz_d50/xyz.hrx::white` 写的是
/// `color(xyz 0.9956342097 1.0026671299 1.3221722918)`。
#[tokio::test]
async fn the_white_row_is_what_a_transcription_used_to_miss() {
    let built = css("a {b: color.to-space(color(xyz-d50 1 1 1), xyz)}").await;
    assert!(
        built.contains("color(xyz 0.9956342097 1.0026671299 1.3221722918)"),
        "the archive's white row is three channels the copied table got wrong:\n{built}"
    );
}

/// 两张表必须**共同**守住的不变量，也是 `color/matrix.rs` 的 D50 白改为导出、而非照抄的理由：
/// 一个中性色换算到 `lab` 必须 `a = b = 0` 且 `L = 100`，逐位一致（`to_space/srgb/lab.hrx::white`）。
/// 用照抄的白时，本编译器曾答 `lab(100.0000000018% 0.0000001274 0.0000000081)`——那个字面量与适应
/// 实际落到的三元组之间 `8.3e-10` 的失配，被 `xyz_to_lab` 里的 `500 · (fx − fy)` 放大。导出它让
/// 这个相等成为结构性事实，于是第二个白字面量再也回不来。
#[tokio::test]
async fn a_neutral_converts_to_a_neutral_lab() {
    let built = css("a {b: color.to-space(color(srgb 1 1 1), lab)}").await;
    assert!(
        built.contains("lab(100% 0 0)"),
        "the archive states `lab(100% 0 0)` for its own white; this compiler answered:\n{built}"
    );
    let grey = css("a {b: color.to-space(color(srgb 0.5 0.5 0.5), lab)}").await;
    assert!(
        grey.contains("lab(53.3889647411% 0 0)"),
        "`to_space/srgb/lab.hrx::gray` states `lab(53.3889647411% 0 0)`; this compiler answered:\n{grey}"
    );
}
