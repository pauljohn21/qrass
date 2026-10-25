//! Multicast probes: what `publish().ref_count()` / `Shared::subject` actually deliver when one
//! producer feeds several consumers. Measured, because this is the shape every directive needs
//! (`@mixin` → many `@include`, one `@use` module → many importers).

use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rxrust::prelude::*;

/// One producer, two branches, synchronous source: does a branch see the state of the event's
/// own position, or does the first subscriber drain the shared upstream?
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
    // Measured: `[(1,6),(2,6),(3,6)]` — every event pairs with the FINAL state, because with a
    // synchronous source the first-subscribed branch drains the shared upstream. So pairing can
    // never carry per-position state; `scan` (state as the emitted item) is the framework's answer.
    tracing::info!(pairs = ?sink.borrow(), "ref_count branch pairing");
    assert_eq!(*sink.borrow(), vec![(1, 6), (2, 6), (3, 6)]);
}

/// The framework's explicit bus: consumers subscribe first, one producer pushes, every consumer
/// receives the whole sequence — no collecting and hand-feeding anywhere.
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
