//! Shape pin for the L2 branch fan-out (`src/branch_fanout.rs`).
//!
//! rxrust 1.0.0-rc.5 constrains which form a `Shared` `flat_map` fan-out may take,
//! and the constraints are invisible from the qrass side, so they are pinned here
//! against the raw library:
//!
//!  * `.into_local()` may **not** follow `flat_map`: it retypes the outer chain to
//!    the `Local` scope while the inner observables stay `Shared`, and the merge
//!    machinery then wants `MutRc<MergeAllState<..>>: Observer<SharedCtx<..>, _>`
//!    (E0277). The only drain that works is a `Send` closure subscribe over an
//!    `mpsc` channel — which is why the stage owns its terminal instead of calling
//!    `crate::drain::drain`.
//!  * that closure subscribe is legal because `Of<T>` hard-codes its error channel
//!    to `Infallible` (`observable/of.rs`), so no `error`/`complete` signal can be
//!    silently dropped, and compiler errors can ride the value channel as data.
//!  * inner sources without a scheduler finish during `subscribe`, so the sender
//!    drops before `rx.iter()` and no tokio runtime is involved.
//!  * inline inner sources merge in outer order — rxrust's own
//!    `flat_map_merges_inner_observables` pins `vec![1, 11, 2, 12, 3, 13]` — but a
//!    concurrent consumer must not depend on that, hence the batch index each item
//!    carries and the sort the stage performs.

use rxrust::prelude::*;
use std::sync::mpsc;

#[derive(Clone, Debug)]
struct Unit {
    idx: usize,
    v: i32,
}

fn units() -> Vec<Unit> {
    vec![
        Unit { idx: 0, v: 1 },
        Unit { idx: 1, v: 2 },
        Unit { idx: 2, v: 3 },
    ]
}

/// The exact shape `fan_out` uses: `Shared` end to end, terminal is a `Send`
/// closure subscribe feeding an `mpsc` channel.
#[test]
fn a_shared_flat_map_drains_through_a_send_closure() {
    let (tx, rx) = mpsc::channel::<(usize, i32)>();
    Shared::from_iter(units())
        .flat_map(move |unit: Unit| Shared::of((unit.idx, unit.v * 2)))
        .subscribe(move |item| {
            tx.send(item).ok();
        });
    let out: Vec<_> = rx.iter().collect();
    assert_eq!(out, vec![(0, 2), (1, 4), (2, 6)]);
}

/// Errors are data: the `Of` error channel is `Infallible`, so a failed branch is
/// delivered through the value channel and a plain `subscribe` cannot miss it.
#[test]
fn a_branch_failure_rides_the_value_channel() {
    let (tx, rx) = mpsc::channel::<(usize, Result<i32, &'static str>)>();
    Shared::from_iter(units())
        .flat_map(move |unit: Unit| {
            let outcome: Result<i32, &str> = if unit.idx == 1 {
                Err("unresolvable")
            } else {
                Ok(unit.v * 2)
            };
            Shared::of((unit.idx, outcome))
        })
        .subscribe(move |item| {
            tx.send(item).ok();
        });
    let out: Vec<_> = rx.iter().collect();
    assert_eq!(
        out,
        vec![
            (0, Ok(2)),
            (1, Err("unresolvable")),
            (2, Ok(6)),
        ],
        "a failure must not remove its branch from the batch"
    );
}

/// The stage reorders by batch index rather than trusting merge order; this is the
/// contract the sort enforces even if items arrive shuffled.
#[test]
fn results_are_reordered_by_batch_index_not_arrival_order() {
    let (tx, rx) = mpsc::channel::<(usize, i32)>();
    Shared::from_iter(vec![
        Unit { idx: 2, v: 3 },
        Unit { idx: 0, v: 1 },
        Unit { idx: 1, v: 2 },
    ])
    .flat_map(move |unit: Unit| Shared::of((unit.idx, unit.v * 2)))
    .subscribe(move |item| {
        tx.send(item).ok();
    });
    let mut out: Vec<_> = rx.iter().collect();
    out.sort_by_key(|(idx, _)| *idx);
    assert_eq!(out, vec![(0, 2), (1, 4), (2, 6)]);
}
