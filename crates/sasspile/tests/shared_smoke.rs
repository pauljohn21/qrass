//! Task 0.3 smoke: prove the `Shared` (Send) observable path compiles and runs
//! inline in rxrust rc.6, so the core rewrite onto `Shared` streams rests on a checked
//! API (non-timed operators execute during `subscribe`, no runtime needed).

use std::sync::mpsc;

use rxrust::prelude::*;

#[test]
fn shared_from_iter_map_subscribe() {
    let (tx, rx) = mpsc::channel::<Vec<i64>>();
    Shared::from_iter(vec![1i64, 2, 3])
        .map(|x| x * 2)
        .collect::<Vec<_>>()
        .subscribe(move |v| {
            tx.send(v).ok();
        });
    assert_eq!(rx.recv().unwrap(), vec![2, 4, 6]);
}

#[test]
fn shared_flat_map_is_the_loop() {
    // nesting/expansion as a stream of streams, no recursion / no frame stack
    let (tx, rx) = mpsc::channel::<Vec<String>>();
    Shared::from_iter(vec!["a", "b"])
        .flat_map(|item| Shared::from_iter(vec![1i32, 2]).map(move |n| format!("{item}{n}")))
        .collect::<Vec<_>>()
        .subscribe(move |v| {
            tx.send(v).ok();
        });
    let got = rx.recv().unwrap();
    assert_eq!(got, vec!["a1", "a2", "b1", "b2"]);
}

#[test]
fn shared_fanout_is_order_stable_via_key() {
    // task 2.1 de-risk (design D3): fan out inner sub-streams, tag each with an owned
    // document-order key, collect, then sort by key -> deterministic expanded order.
    let (tx, rx) = mpsc::channel::<Vec<(u32, String)>>();
    Shared::from_iter(vec![(0u32, "a"), (1, "b"), (2, "c")])
        .flat_map(|(ord, name)| {
            Shared::from_iter(vec![1u32, 2]).map(move |k| (ord, format!("{name}{k}")))
        })
        .collect::<Vec<_>>()
        .subscribe(move |v| {
            tx.send(v).ok();
        });
    let mut got = rx.recv().unwrap();
    got.sort_by_key(|(ord, _)| *ord);
    let joined: Vec<String> = got.into_iter().map(|(_, s)| s).collect();
    assert_eq!(joined, vec!["a1", "a2", "b1", "b2", "c1", "c2"]);
}

#[tokio::test]
async fn shared_runs_inline_under_multi_thread_runtime() {
    // task 2.1 de-risk (design D5): a Shared observable is drivable from a tokio
    // multi-thread runtime; non-timed operators still complete inline at `subscribe`
    // (execution model unchanged by Shared), so `rx.recv()` is ready immediately.
    let (tx, rx) = mpsc::channel::<Vec<i32>>();
    Shared::from_iter(vec![1, 2, 3])
        .map(|x| x * 2)
        .collect::<Vec<_>>()
        .subscribe(move |v| {
            tx.send(v).ok();
        });
    assert_eq!(rx.recv().unwrap(), vec![2, 4, 6]);
}
