//! 框架行为探针：系统自身动词的实际行为，实测而非假设。它们钉住那些决定编译器状态
//! 必须住在哪里的语义。

use rxrust::prelude::*;

/// 实测事实：pairing 不能携带按位置的状态。两侧皆冷时，状态侧先跑到完成，于是**每个**
/// 事件都与最终值（`300`）配对 —— 因此必须随 stream 一起走的状态就得是 `scan` 发出的
/// 累积项本身，而不是配对算子重构出来的一个值。
#[test]
fn with_latest_from_pairs_events_with_the_latest_state_value() {
    let mut got = Vec::new();
    Local::from_iter([1u32, 2, 3])
        .with_latest_from(Local::from_iter([100u32, 200, 300]))
        .subscribe(|pair| got.push(pair));
    assert_eq!(got, vec![(1, 300), (2, 300), (3, 300)]);
}

/// 实测事实：同步冷内层 observable 下，摊平已经是有序的 —— 对 `concat_all` 与 `flat_map`
/// 都成立 —— 因而收进 `Vec` 再重排是一步多余的批处理。`order` 键只在工作真正交给调度器的
/// 地方才值回它的位置。
#[test]
fn flattening_synchronous_inners_is_already_ordered() {
    let inner = |n: u32| Local::from_iter([n * 10, n * 10 + 1]);
    let expected = vec![10, 11, 20, 21, 30, 31];

    let mut ordered = Vec::new();
    Local::from_iter([1u32, 2, 3])
        .map(inner)
        .concat_all()
        .subscribe(|v| ordered.push(v));

    let mut concurrent = Vec::new();
    Local::from_iter([1u32, 2, 3])
        .flat_map(inner)
        .subscribe(|v| concurrent.push(v));

    assert_eq!(ordered, expected);
    assert_eq!(concurrent, expected);
}
