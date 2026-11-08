//! 本文件守的律：**`srgb` 的传递在近黑处有线性分段，语料如此陈述。**
//!
//! 这与邻近空间的答案相反，所以各自分别量过（`tasks.md` 9.21 记三条读数）：删去 `rec2020` 的
//! `4.5×` 臂多赢 41 行，而删去这些 `12.92` 臂丢 156 行 —— `ledger diff rec2020-audit-closure
//! srgb-no-linear-arm` 印出 `regress=156 net=0`，`ledger shift` 把移动的行分类为
//! `farther=109 closer=9`。所以这些臂不是从关于该空间的散文里继承的风格，而是归档算出的东西。
//!
//! 决定它的行是那些根本到不了分段的行 —— 黑色的换算，每个通道都是 `0`，承载值的不是幂分支：
//! `to_space/hwb/srgb_linear.hrx::black` 在无臂时答 `color(srgb-linear 0.0008338051 0.0008338051 0.0008338051)`，
//! 而归档印 `color(srgb-linear 0 0 0)`；`to_space/srgb/xyz_d50.hrx::black` 答
//! `color(xyz-d50 0.0008040347 0.0008338051 0.0006879764)`，而归档印 `color(xyz-d50 0 0 0)`。
//!
//! 本文件**故意**不钉的：分段的**阈值**（印刷侧 `0.04045`、线性侧 `0.0031308`）。删臂只检验分段
//! 存在，它对终点一字不提，而本量具拟合的行从不进入该区域 ——
//! `cargo run -q -p sasspile --example calibrate_space -- srgb xyz_d50 2.4 1.055` 报
//! `smallest_curved_printed_channel = 0.1`，所以归档行只能从上方界定阈值。移动阈值也不是本量具
//! 当前能 A/B 的（它能在幂与偏移下线性化，不能在分段下），所以那两个数仍是转录 —— 并在
//! `color/matrix.rs` 里如此声明，不在此断言。

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// 两条 `black` 行，输入行逐字取自各 case 的 `input.scss`，期望行取自其 `output.css`。
/// 二者都只在留着线性臂时才通过。
#[tokio::test]
async fn srgb_keeps_the_linear_segment_the_archive_computes() {
    let built = css("a {b: color.to-space(hwb(0deg 0% 100%), srgb-linear)}").await;
    assert!(
        built.contains("color(srgb-linear 0 0 0)"),
        "`to_space/hwb/srgb_linear.hrx::black` prints `color(srgb-linear 0 0 0)`; without the `12.92` arm this \
         compiler answered:\n{built}"
    );

    let built = css("a {b: color.to-space(color(srgb 0 0 0), xyz-d50)}").await;
    assert!(
        built.contains("color(xyz-d50 0 0 0)"),
        "`to_space/srgb/xyz_d50.hrx::black` prints `color(xyz-d50 0 0 0)`; without the arm this compiler \
         answered:\n{built}"
    );
}
