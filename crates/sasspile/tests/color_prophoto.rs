//! 本文件守的律：**`prophoto-rgb` 的 primaries 表是归档自己的，而且它早就携带了自己的白点** ——
//! 所以没有任何东西会对这个空间再适应第二次。
//!
//! 当此空间的表还是手抄表时，`Space::white()` 曾为 `prophoto-rgb` 答 D50，于是经由此空间的换算被做了
//! 两次色适应，而语料只做一次。在存活的曲线假设（`linear = printed^1.8`，`color/matrix.rs` 对
//! `to_space/prophoto_rgb/xyz.hrx` 检验过它 —— 数字与分段 caveat 见 `tasks.md` 9.18）下拟合这张表，
//! 它自己就落在 D65 白上：`to_space/prophoto_rgb/xyz.hrx::white` 写的是
//! `color(xyz 0.9504559271 1 1.0890577508)`，而原先合成的那一对会答
//! `color(xyz 0.9845637886 0.9989843031 1.089497388)` —— 在归档读到第十位小数的一行上，X 差 `3.4e-2`。
//! （`4.2e-2` 那个数是 `[0][0]` 项自己的差，不属于这一行；两者是不同对象，本文件初稿曾把它们混为一谈。）
//!
//! 九个表项每一个与手抄表的差都超过各自的双向散布，所以整张表现在都是归档的；`white()` 因而只把
//! `xyz-d50`、`lab` 与 `lch` 列为 D50，prophoto 颜料的参考白住在矩阵里，而不是第二处声明。
//!
//! 本文件**故意**不钉的（附理由）：`to_space/xyz/prophoto_rgb.hrx::white`，其答案在两个通道上各差一个单位
//! （本实现 `1.0429146547 0.9845782986`，归档 `1.0429146548 0.9845782985`）—— `tasks.md` 9.14/9.16
//! 点名的第十位小数天花板，不是新缺陷。凡往返 `rec2020` 的行也不钉：那个空间的表仍是手抄表，而本批
//! 全部十条 `farther` 行都牵涉它。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// `prophoto-rgb → xyz`。`white` 那行固定白点（曲线把 1 映射到 1，所以算出来的就是表的行和，
/// 别无其他）；`middle` 与 `out_of_range/near` 钉住各单项，包括带符号曲线必须携带通过的负通道。
#[tokio::test]
async fn prophoto_rgb_reaches_xyz_on_the_archives_own_numbers() {
    let cases = [
        ("1 1 1", "0.9504559271 1 1.0890577508"),
        ("0.2 0.4 0.8", "0.1183353555 0.1633232963 0.7325727364"),
        ("-1 0.4 2", "-0.4478815578 -0.0732156915 3.8173184875"),
    ];
    for (input, printed) in cases {
        let built = css(&format!(
            "a {{b: color.to-space(color(prophoto-rgb {input}), xyz)}}"
        ))
        .await;
        assert!(
            built.contains(&format!("color(xyz {printed})")),
            "for `color(prophoto-rgb {input})` the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}
