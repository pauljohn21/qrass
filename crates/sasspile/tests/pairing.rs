//! Framework-behaviour probes: what the system's own verbs actually do, measured rather than
//! assumed. These pin the semantics that decide where compiler state must live.

use rxrust::prelude::*;

/// Measured fact: pairing cannot carry per-position state. With both sides cold, the state side
/// runs to completion first, so **every** event pairs with the final value (`300`) — therefore
/// state that must travel with the stream has to BE the accumulated item `scan` emits, not a
/// value reconstructed by an operator pairing.
#[test]
fn with_latest_from_pairs_events_with_the_latest_state_value() {
    let mut got = Vec::new();
    Local::from_iter([1u32, 2, 3])
        .with_latest_from(Local::from_iter([100u32, 200, 300]))
        .subscribe(|pair| got.push(pair));
    assert_eq!(got, vec![(1, 300), (2, 300), (3, 300)]);
}

/// Measured fact: with synchronous cold inner observables the flattening is already in order —
/// for both `concat_all` and `flat_map` — so collecting into a `Vec` to re-sort is a redundant
/// batch step. An `order` key only earns its keep where work is truly handed to the scheduler.
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
