//! V0-04 练习答案（可编译；当作验证 crate 的 `tests/answers.rs` 跑）。
//!
//! 注意：§6 的三条编译错**不能**放在这里——它们会让整个测试 crate 编译失败、
//! 连带其他断言都跑不起来。那三条是在验证 crate 里另起一次性文件复现的（课文有原文）。

use minirx_check::{
    CellArc, CellRc, MutArc, MutRc, RcDeref, RcDerefMut, SharedCell, bump, peek,
};

/// 练习 3：借用被拒时返回默认值而不是 panic。
///
/// 两种结果（拿到守卫 / 拿不到）由一个 `match` 一次收口：出口唯一，不叠 `if` + 提前 `return`。
pub fn bump_or_default<D: RcDerefMut<Target = i32>>(handle: &D, by: i32) -> i32 {
    match handle.try_rc_deref_mut() {
        Some(mut guard) => {
            *guard += by;
            *guard
        }
        None => 0,
    }
}

/// 练习 2：一个"读改写"的共享计数器，只用 `SharedCell` 的能力。
pub fn bump_shared<C: SharedCell<i32>>(counter: &C) -> i32 {
    let current = counter.get();
    match counter.compare_exchange(current, current + 1) {
        Ok(previous) => previous + 1,
        // 被别的线程抢先改了：重新读一次，不 panic 也不死锁。
        Err(fresh) => fresh,
    }
}

/// 练习 1：`Clone` 要手写的原因——`T` 不 `Clone` 时指针照样能克隆。
#[derive(Default)]
pub struct NoClone;

/// 手写版：对任何 `T` 都成立（这是 `rc.rs:70-72` 的做法）。
pub fn clone_any(handle: MutRc<NoClone>) -> (MutRc<NoClone>, usize) {
    let second = handle.clone();
    (second, 1)
}

/// 练习 4：三种句柄在同一批函数下都跑得通——泛型签名里没有任何容器名字。
pub fn shared_shape() {
    let local: MutRc<i32> = MutRc::from(1);
    let remote: MutArc<i32> = MutArc::from(1);
    let cell_local: CellRc<i32> = CellRc::from(1);
    let cell_shared: CellArc = CellArc::from(1);

    assert_eq!(bump(&local, 1), 2);
    assert_eq!(bump(&remote, 1), 2);
    assert_eq!(peek(&local), 2);
    assert_eq!(bump_shared(&cell_local), 2);
    assert_eq!(bump_shared(&cell_shared), 2);
}

#[test]
fn answers_compile_and_agree() {
    shared_shape();

    let busy: MutRc<i32> = MutRc::from(7);
    let held = busy.rc_deref_mut();
    assert_eq!(bump_or_default(&busy, 3), 0, "被占用时不 panic，交回默认值");
    drop(held);
    assert_eq!(bump_or_default(&busy, 3), 10);

    let counter: CellArc = CellArc::from(5);
    assert_eq!(bump_shared(&counter), 6);
    assert_eq!(counter.get(), 6);

    let (second, ones) = clone_any(MutRc::from(NoClone));
    let _ = second.rc_deref();
    assert_eq!(ones, 1);
}
