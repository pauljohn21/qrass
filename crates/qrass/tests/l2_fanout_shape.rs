//! Shape pins for the L2 `Local` fan-out and module bus (`src/branch_fanout.rs`,
//! `src/module_bus.rs`).
//!
//! The crate is `Local` end to end — the wasm contract: no stage may require
//! `Send`/`Sync` or the framework's `Arc`/`Mutex` plumbing, so the whole compiler
//! runs on a single-threaded `wasm32-unknown-unknown` runtime. rxrust constrains
//! which forms a `flat_map` fan-out or a `BehaviorSubject` hub may take in that
//! context, and the constraints are invisible from the qrass side, so they are
//! pinned here against the raw library:
//!
//!  * a `Local::from_iter(..).flat_map(|u| Local::of(..))` chain stays in **one**
//!    context from source to terminal and drains through a plain (non-`Send`)
//!    closure subscribe over an `mpsc` channel — the exact terminal `fan_out`
//!    owns. (Historical boundary: mixing contexts fails — `.into_local()` may
//!    **not** follow a `Shared` `flat_map`, because it retypes the outer chain to
//!    the `Local` scope while the inner observables stay `Shared` and the merge
//!    machinery then wants `MutRc<MergeAllState<..>>: Observer<SharedCtx<..>, _>`
//!    (E0277) — so the stage is *born* `Local` rather than converted.)
//!  * that closure subscribe is legal because `Of<T>` hard-codes its error channel
//!    to `Infallible` (`observable/of.rs`), so no `error`/`complete` signal can be
//!    silently dropped, and compiler errors can ride the value channel as data.
//!  * inner sources without a scheduler finish during `subscribe`, so the sender
//!    drops before `rx.iter()` and no runtime is involved — on host or wasm.
//!  * inline inner sources merge in outer order — rxrust's own
//!    `flat_map_merges_inner_observables` pins `vec![1, 11, 2, 12, 3, 13]` — but a
//!    merge-shaped consumer must not depend on that, hence the batch index each
//!    item carries and the sort the stage performs.
//!  * the hub spelling `module_bus` stores as a struct field (`Local` context,
//!    `MutRc` payload, `()` error channel) exposes a synchronous `Behavior::peek`,
//!    so a joining parent reads the settled singleton without subscribing.

use rxrust::prelude::*;
use rxrust::rc::MutRc;
use rxrust::subject::behavior_subject::{Behavior, BehaviorSubject};
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

/// The exact shape `fan_out` uses: `Local` end to end, terminal is a plain
/// closure subscribe feeding an `mpsc` channel — no `Send` bound anywhere.
#[test]
fn a_local_flat_map_drains_through_a_plain_closure() {
    let (tx, rx) = mpsc::channel::<(usize, i32)>();
    Local::from_iter(units())
        .flat_map(move |unit: Unit| Local::of((unit.idx, unit.v * 2)))
        .subscribe(move |item| {
            tx.send(item).ok();
        });
    let out: Vec<_> = rx.iter().collect();
    assert_eq!(out, vec![(0, 2), (1, 4), (2, 6)]);
}

/// A `!Send` capture survives the whole chain — the property the `Shared` spelling
/// could never offer and the wasm target needs: a captured `Rc` rides the fan-out
/// closure without tripping any `Send` bound.
#[test]
fn a_local_chain_carries_a_non_send_capture() {
    use std::rc::Rc;
    let shared_nothing = Rc::new(2i32);
    let (tx, rx) = mpsc::channel::<(usize, i32)>();
    Local::from_iter(units())
        .flat_map(move |unit: Unit| Local::of((unit.idx, unit.v * *shared_nothing)))
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
    Local::from_iter(units())
        .flat_map(move |unit: Unit| {
            let outcome: Result<i32, &str> = if unit.idx == 1 {
                Err("unresolvable")
            } else {
                Ok(unit.v * 2)
            };
            Local::of((unit.idx, outcome))
        })
        .subscribe(move |item| {
            tx.send(item).ok();
        });
    let out: Vec<_> = rx.iter().collect();
    assert_eq!(
        out,
        vec![(0, Ok(2)), (1, Err("unresolvable")), (2, Ok(6)),],
        "a failure must not remove its branch from the batch"
    );
}

/// The stage reorders by batch index rather than trusting merge order; this is the
/// contract the sort enforces even if items arrive shuffled.
#[test]
fn results_are_reordered_by_batch_index_not_arrival_order() {
    let (tx, rx) = mpsc::channel::<(usize, i32)>();
    Local::from_iter(vec![
        Unit { idx: 2, v: 3 },
        Unit { idx: 0, v: 1 },
        Unit { idx: 1, v: 2 },
    ])
    .flat_map(move |unit: Unit| Local::of((unit.idx, unit.v * 2)))
    .subscribe(move |item| {
        tx.send(item).ok();
    });
    let mut out: Vec<_> = rx.iter().collect();
    out.sort_by_key(|(idx, _)| *idx);
    assert_eq!(out, vec![(0, 2), (1, 4), (2, 6)]);
}

/// The hub spelling `module_bus` stores as a field: a `Local` `BehaviorSubject`
/// whose settled value is read synchronously by `peek` — no observer registers,
/// so the rc.5 late-subscriber deadlock surface cannot arise.
#[test]
fn a_local_behavior_hub_peeks_its_settled_value() {
    type HubItem = Option<i32>;
    #[allow(clippy::type_complexity)]
    let mut hub: Local<
        BehaviorSubject<
            MutRc<HubItem>,
            SubjectPtr<'static, LocalCtx<(), LocalScheduler>, HubItem, ()>,
        >,
    > = Local::behavior_subject::<HubItem, ()>(None);
    assert_eq!(hub.peek(), None, "an unsettled hub reads its initial value");
    hub.next(Some(7));
    hub.clone().complete();
    assert_eq!(
        hub.peek(),
        Some(7),
        "a joining parent reads the settled singleton without subscribing"
    );
}
