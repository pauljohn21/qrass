//! 本文件守的律：**`a98-rgb` 的通道相对线性光是一个 `563/256` 伽马，方向是归档所放的那一侧。**
//!
//! `color/matrix.rs` 曾以 `1/2.19921` 解码、`2.19921` 编码 —— 正好与语料所算的相反 —— 还把伽马取了整。
//! 两者都没在启发它们的读数里显形，因为互倒的指数看起来是对称的：曲线固定 0 与 1，只有一行经这一对
//! 换算才能把它们分开。分开候选的是各自假设下拟合的残差 ——
//! `cargo run -q -p sasspile --example calibrate_space -- a98_rgb xyz <exponent>`，现在假设不成立时以非零
//! 码退出 —— 其数字记于 `openspec/…/tasks.md` 9.17。同一探针指向 `prophoto-rgb` 时说它的方向本来就对，
//! 所以这是一条臂的错误，不是一套约定。
//!
//! 曲线旁边的表也在存活假设下拟合了：手抄值只在其与拟合的差不超过该项自身双向散布处保留，而 Z 输出行
//! 不是这样的一项 —— 旧表正是因此在归档自己的白点不变量上失败：`to_space/a98_rgb/xyz.hrx::white` =
//! `color(xyz 0.9504559271 1 1.0890577508)`。
//!
//! 本文件**故意**不钉的：`to_space/a98_rgb/xyz.hrx::middle` 与 `to_space/xyz/a98_rgb.hrx::middle`，各差
//! 第十位小数一个单位（拟合残差恰好落在那一行上时为 `5.0139e-11`），以及所有剩余误差属于另一个空间的
//! 表的行 —— 那些由它们自己的红行与 `tasks.md` 守着，而不是把我的答案抄进断言。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// 共同固定曲线*与*表的两行。`white` 与曲线无关（幂函数把 1 映射到 1），所以它给出的是行和；
/// `out_of_range/near` 的通道符号混合（`(-1, 0.4, 2)`），只有当伽马落在解码侧时才会落到归档的数字上 ——
/// 用以前那个反接的指数，本编译器曾把该行答成 `color(xyz -0.1947713003 0.2194350107 1.3255759776)`
/// （存于 run `display-p3-solved-final`），而语料印的是 `color(xyz 0.3124701203 0.1320464482 4.5349169732)`。
#[tokio::test]
async fn a98_rgb_reaches_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("a98-rgb 1 1 1", "color(xyz 0.9504559271 1 1.0890577508)"),
        (
            "a98-rgb -1 0.4 2",
            "color(xyz 0.3124701203 0.1320464482 4.5349169732)",
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

/// 另一个方向，用同一曲线与同一张表的算出逆矩阵。
#[tokio::test]
async fn xyz_reaches_a98_rgb_on_the_archives_own_numbers() {
    let built = css("a {b: color.to-space(color(xyz 1 1 1), a98-rgb)}").await;
    assert!(
        built.contains("color(a98-rgb 1.0579328451 0.9761412908 0.9581457442)"),
        "`to_space/xyz/a98_rgb.hrx::white` states those channels, this compiler answered:\n{built}"
    );
}
