//! 多播探针：一个生产者供多个消费者时，`publish().ref_count()` / `Shared::subject` 实际交付什么。
//! 实测，因为这是每个指令都需要的形状（`@mixin` → 多个 `@include`，一个 `@use` 模块
//! → 多个导入者）。

use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rxrust::prelude::*;

/// 一个生产者，两条分支，同步源：某分支看到的是事件自己位置的状态，还是第一个订阅者
/// 抽干了共享上游？
#[test]
fn ref_count_branches_on_a_synchronous_source() {
    let shared = Local::from_iter([1u32, 2, 3]).publish().ref_count();
    let sums = shared.clone().scan(0u32, |acc, ev| acc + ev);

    let sink: Rc<RefCell<Vec<(u32, u32)>>> = Rc::new(RefCell::new(Vec::new()));
    let collector = Rc::clone(&sink);
    shared
        .with_latest_from(sums)
        .subscribe(move |p| collector.borrow_mut().push(p));
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    // 实测：`[(1,6),(2,6),(3,6)]` —— 每个事件都与**最终**状态配对，因为同步源下先订阅的分支
    // 抽干了共享上游。所以 pairing 永远带不上按位置的状态；`scan`（状态即发出的项）
    // 是框架给出的答案。
    tracing::info!(pairs = ?sink.borrow(), "ref_count branch pairing");
    assert_eq!(*sink.borrow(), vec![(1, 6), (2, 6), (3, 6)]);
}

/// 框架显式的总线:消费者先订阅,一个生产者推,每个消费者收到整个序列 ——
/// 全程没有收集与手喂。
#[test]
fn shared_subject_feeds_every_consumer_from_one_producer() {
    let bus = Shared::subject::<u32, Infallible>();
    let a: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let b: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

    let (sink_a, sink_b) = (Arc::clone(&a), Arc::clone(&b));
    bus.clone()
        .subscribe(move |v| sink_a.lock().unwrap().push(v));
    bus.clone()
        .subscribe(move |v| sink_b.lock().unwrap().push(v));

    let mut producer = bus.clone();
    Shared::from_iter([1u32, 2, 3]).subscribe(move |v| producer.next(v));
    bus.complete();

    assert_eq!(*a.lock().unwrap(), vec![1, 2, 3]);
    assert_eq!(*b.lock().unwrap(), vec![1, 2, 3]);
}
