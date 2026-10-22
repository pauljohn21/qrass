//! Standard terminal for a cold stage stream (manual §6): drain it through an
//! `mpsc` channel and fold the emitted items **by value**, short-circuiting on
//! the first error. Having one implementation keeps the synchronous APIs —
//! `lex`, `parse`, `eval_all`, used by tests and by inner module loads — from
//! growing a second, imperative copy of each stage's fold.

use std::convert::Infallible;
use std::sync::mpsc;

use rxrust::prelude::*;

use crate::error::SassError;

/// Collect every item of a stage stream into one owned vector, or return the
/// first error it emitted. Non-timed sources execute fully inline during
/// `subscribe`, so the sender is dropped once the stream completes and
/// `rx.iter()` terminates without a runtime.
pub(crate) fn drain<T: 'static>(
    stream: LocalBoxedObservable<'static, Result<T, SassError>, Infallible>,
) -> Result<Vec<T>, SassError> {
    // Closure `subscribe` is legal here only because the stream's error channel
    // is `Infallible` — every compiler error already rides the value channel.
    let (tx, rx) = mpsc::channel::<Result<T, SassError>>();
    stream.subscribe(move |item| {
        tx.send(item).ok();
    });
    rx.iter().try_fold(Vec::new(), |mut acc, item| match item {
        Ok(value) => {
            acc.push(value);
            Ok(acc)
        }
        Err(err) => Err(err),
    })
}

/// Terminal for a single-emission fold stream (a `reduce_initial` chain emits
/// exactly its final accumulator): drain it and take that one value.
/// Panics only for a stream that breaks the one-emission contract, which the
/// pure transforms feeding it never can.
pub(crate) fn drain_one<T: 'static>(
    stream: LocalBoxedObservable<'static, Result<T, SassError>, std::convert::Infallible>,
) -> T {
    drain(stream)
        .ok()
        .and_then(|mut items| items.pop())
        .expect("a reduce_initial fold emits exactly once")
}
