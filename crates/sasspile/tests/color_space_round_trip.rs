//! 本文件守的律：**色彩空间换算的两个方向是一张表，不是两张。**
//!
//! `to_space/oklab/oklch.hrx` 与 `to_space/oklch/oklab.hrx` 是语料自己的往返 —— 读一个
//! `oklab`、印一个 `oklch`、再回来 —— 其中十三行曾经红在一个再怎么校准矩阵也修不了的
//! 原因上：`color/matrix.rs` 同时带着正向锥矩阵*和*一份手抄的逆矩阵，两者不一致。
//! 一个出去又回来的颜色，回来时不再是进去时的样子。逆矩阵现在由正向表算出，所以这对表
//! 构造上互逆 —— 无论哪张表带哪些数字，往返都不可能再腐蚀值。
//!
//! 本文件**故意**不钉的：oklab↔XYZ 的绝对数字。`to_space/srgb/oklab.hrx` 对
//! `color(srgb 0.5 0.5 0.5)` 答 `oklab(59.8180730527% 0 0)`，而本编译器仍给它印出非零的
//! `a` 与 `b`，所以锥表的*数字*还不是语料的 —— 这个缺口由那些红行守着，在此抄一个
//! 近似值只会把我的错误冻成要求。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// 一条语料读数，逐字抄自 `to_space/oklab/oklch.hrx::middle`：明度存活，两个色度变成一个
/// 色度与一个色相。这只依赖立方根矩阵与极坐标变换，所以无论锥表怎么校准都成立。
#[tokio::test]
async fn oklab_reads_as_oklch_the_way_the_corpus_prints_it() {
    let built = css("a {b: color.to-space(oklab(50% 0.2 -0.3), oklch)}").await;
    assert!(
        built.contains("oklch(50% 0.3605551275 303.690067526deg)"),
        "expected the archive's own oklch reading in: {built}"
    );
}

/// 往返就是那个破过的不变量。两半都经编译器，所以断言不可能被本文件发明的值满足 ——
/// 若第二份手抄逆矩阵表卷土重来，无论哪张表带哪些数字，它都会响亮地失败。
#[tokio::test]
async fn oklab_to_oklch_and_back_returns_the_colour_it_started_from() {
    let direct = css("a {b: color.to-space(oklab(50% 0.2 -0.3), oklab)}").await;
    let round_trip =
        css("a {b: color.to-space(color.to-space(oklab(50% 0.2 -0.3), oklch), oklab)}").await;
    assert_eq!(direct, round_trip);
}
