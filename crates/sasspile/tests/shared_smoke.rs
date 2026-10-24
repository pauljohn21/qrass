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
