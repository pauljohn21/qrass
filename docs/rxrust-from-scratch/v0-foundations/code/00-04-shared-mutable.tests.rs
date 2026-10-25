//! V0-04 的断言：同一份代码在 `Rc<RefCell<_>>` 与 `Arc<Mutex<_>>` 上都成立；
//! 重复借用/重复加锁不 panic 而是 `None`；`Rc` 不能跨线程（答案文件里给出原文）。

use minirx_check::{
    CellArc, CellRc, MutArc, MutRc, RcDeref, RcDerefMut, SharedCell, bump, parallel_bump, peek,
};

#[test]
fn one_generic_function_serves_both_share_shapes() {
    let local: MutRc<i32> = MutRc::from(10);
    assert_eq!(bump(&local, 5), 15);
    assert_eq!(peek(&local), 15);
    assert_eq!(*local.rc_deref(), 15);

    let remote: MutArc<i32> = MutArc::from(10);
    assert_eq!(bump(&remote, 5), 15);
    assert_eq!(peek(&remote), 15);
}

#[test]
fn clones_share_one_content_not_a_copy_of_it() {
    let original: MutRc<Vec<i32>> = MutRc::from(vec![1, 2]);
    let second = original.clone();

    original.rc_deref_mut().push(3);
    assert_eq!(*second.rc_deref(), vec![1, 2, 3], "两个名字，一份数据");
}

#[test]
fn try_variants_report_refusals_instead_of_panicking() {
    let counter: MutRc<i32> = MutRc::from(0);
    let held = counter.rc_deref_mut();
    assert!(
        counter.try_rc_deref_mut().is_none(),
        "已被可变借用时 try_* 必须返回 None（这就是它存在的理由）"
    );
    drop(held);
    assert!(counter.try_rc_deref_mut().is_some());

    let shared: MutArc<i32> = MutArc::from(0);
    let guard = shared.rc_deref_mut();
    assert!(shared.try_rc_deref_mut().is_none(), "同一线程重复 lock 会被 try_lock 拒");
    drop(guard);
}

#[test]
fn cells_offer_cas_without_borrow_guards() {
    let local: CellRc<i32> = CellRc::from(1);
    assert_eq!(local.get(), 1);
    local.set(4);
    assert_eq!(local.compare_exchange(4, 9), Ok(4));
    assert_eq!(local.compare_exchange(0, 9), Err(9));

    let shared: CellArc = CellArc::from(1);
    assert_eq!(shared.compare_exchange(1, 2), Ok(1));
    assert_eq!(shared.get(), 2);
}

#[test]
fn every_worker_reaches_the_lock() {
    let handles = (0..4).map(|_| MutArc::from(0)).collect();
    assert_eq!(parallel_bump(handles, 25), 4);
}
