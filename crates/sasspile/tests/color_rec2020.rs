//! 本文件守的律：**`rec2020` 的通道相对线性光是一个纯有符号 `2.4` 伽马 —— 无偏移，也无线性分段。**
//!
//! `color/matrix.rs` 曾在该传递的两条臂上带一个偏移幂
//! `((v + (1.09929682680944 - 1)) / 1.09929682680944) ^ (1/0.45)`。这两个数都不是语料所算的，形状也不是
//! 我该猜的：`cargo run -q -p sasspile --example calibrate_space -- rec2020 xyz <exponent> <alpha>` 在给定
//! 曲线假设下拟合 primaries，并报出归档自己那些行上的最差残差，假设不成立时以非零码退出。
//! `2.4 / 1.0` 落在 `4.914e-11`（语料第十位小数的半步）；`1/0.45 / 1.09929682680944` —— 原先的律 ——
//! 差 `0.214`，而每个邻近候选都在 `0.019` 与 `0.29` 之间，比之高八到九个数量级。`tasks.md` 9.20 存有
//! 该扫描，以及把 `4.5×` 臂（带着它们 `net=85`，删掉 `net=126`）与表的拟合第 2 行（在它之前 `net=126`，
//! 之后 `net=141`）分开的两组 A/B 读数 —— 不同对象，不同的配对。
//!
//! 曲线旁边的表需要同样的处理，而在任何数字被碰之前，对归档的两次独立读数就指向同一行：
//! `to_space/rec2020/xyz.hrx::white` 乘的是 `(1,1,1)`，任何传递都搬不动它，所以它印出的三个数**就是**
//! 行和。手抄的第 1 行加和为 `0.9999999304`，而语料写的是 `1` —— `7e-8`，不可能是一个舍入故事。
//!
//! 本文件**故意**不钉的：`to_space/rec2020/xyz.hrx::out_of_range/far`，它在 `-999999` 量级上红着，
//! 拟合分辨不出其印刷末位；以及 `to_space/rec2020/lab.hrx::white`，它所得的是
//! `lab(100% -0.0000000002 -0.0000000025)` —— 与 `to_space/display_p3/lab.hrx::white` 读得的**是同一组
//! 数**，所以本批不声称知道哪个空间拥有那个误差（归属未被分离；`tasks.md` 9.20 将其列为未决）。还有
//! 中间变体（手抄第 2 行）曾赢回的十六行：每一行都差第十位小数一个单位，且它们落在换算的**两侧** ——
//! `to_space/rec2020/{srgb,display_p3,…}::*` 读正向矩阵，而 `to_space/*/rec2020::*` 读算出的逆矩阵。
//! 一张表，只取一侧；与归档更吻合的一侧被保留，损失被点名而非藏起来。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// `rec2020 → xyz`。`white` 那行与曲线无关（它固定行和）；`middle` 那行毫不含糊地把 `2.4` 与 `1/0.45`
/// 分开，因为中段通道正是错误指数不再是舍入问题的所在。
#[tokio::test]
async fn rec2020_reaches_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("rec2020 1 1 1", "color(xyz 0.9504559271 1 1.0890577508)"),
        (
            "rec2020 0.2 0.4 0.8",
            "color(xyz 0.1282769355 0.1154243414 0.6241614492)",
        ),
    ];
    for (source, printed) in cases {
        let built = css(&format!("a {{b: color.to-space(color({source}), xyz)}}")).await;
        assert!(
            built.contains(printed),
            "for `color({source})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// 另一个方向，经同一张表的算出逆矩阵与同一曲线的编码臂。像 `0.3678067823` 这样的通道不落在任何合理
/// toe 的任一分段上，所以这一行是编码侧等价于 `middle` 的那一行。
#[tokio::test]
async fn srgb_reaches_rec2020_on_the_archives_own_numbers() {
    let built = css("a {b: color.to-space(color(srgb 0.2 0.4 0.8), rec2020)}").await;
    assert!(
        built.contains("color(rec2020 0.3678067823 0.4291867351 0.7812845626)"),
        "`to_space/srgb/rec2020.hrx::middle` states those channels, this compiler answered:\n{built}"
    );
}
