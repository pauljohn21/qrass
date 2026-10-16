//! L2 module orchestration (design D1): the `@use`/`@forward` singleton as a
//! `Shared` multicast.
//!
//! The manual prescribes that a module imported by several parents is *instantiated
//! once and broadcast*, not looked up and copied. The carrier is a `Shared`
//! [`BehaviorSubject`] keyed by canonical path: `BehaviorSubject::subscribe` emits
//! its current value **before** attaching to later emissions, so a parent that joins
//! after the body ran still receives that one instantiation, and because the payload
//! is `Arc<Module>` the sharing stays read-only (manual §2 "缓存共享用
//! `Arc<不可变 Module>` 只读多播").
//!
//! `Shared` rather than `Local` is deliberate: the hub is `Send + Sync`, so
//! [`crate::eval::EvalState`] keeps the capability the *next* step of D1 needs
//! (concurrent instantiation of disjoint `@use` branches). Per-item stage kernels
//! stay `Local` — the manual's other half of the rule — because a hub carries one
//! coarse value per module, so the `Arc`/`Mutex` plumbing is not a hot path.
//!
//! Why this file owns its own terminal instead of calling [`crate::drain::drain`]:
//! `Subject::subscribe` registers the observer in the subscriber list and a *late*
//! subscriber is never notified by an earlier `complete()` (`Subject::is_closed`
//! only tests emptiness). A closure `subscribe` would therefore keep the channel's
//! `Sender` alive inside the hub forever and `rx.iter()` would block. The terminal
//! below takes the single emission and then unsubscribes, which drops the observer
//! — and its `Sender` — deterministically.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, mpsc};
use std::time::Instant;

use rxrust::prelude::*;
use rxrust::subject::behavior_subject::BehaviorSubject;
use tracing::{Level, span};

use crate::error::{ErrorKind, SassError};
use crate::module::Module;

/// What one hub multicasts: `None` while its body has not run yet, `Some` once the
/// singleton exists. Keeping the "not yet" state *inside* the payload is what lets a
/// request that arrives mid-instantiation join the same carrier instead of starting a
/// second one.
type HubItem = Option<Arc<Module>>;

/// The `Shared` carrier for one canonical module path. `ObservableFactory` is bound
/// to `Context<Inner = ()>`, so `Shared::behavior_subject` produces exactly this
/// spelling — which is why a hub can be a struct field instead of an unnameable chain.
type Hub = Shared<
    BehaviorSubject<
        HubItem,
        SubjectPtr<'static, SharedCtx<(), SharedScheduler>, HubItem, Infallible>,
    >,
>;

/// Registry of the module singletons, one multicast carrier per canonical path.
#[derive(Default)]
pub(crate) struct ModuleBus {
    hubs: HashMap<String, Hub>,
}

impl ModuleBus {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether `key` already has a carrier — settled *or* still being instantiated.
    pub(crate) fn is_open(&self, key: &str) -> bool {
        self.hubs.contains_key(key)
    }

    /// Open the carrier for `key` **before** its body runs, so a re-entrant request
    /// joins this multicast rather than starting a duplicate instantiation.
    pub(crate) fn open(&mut self, key: &str) {
        self.hubs
            .entry(key.to_string())
            .or_insert_with(|| Shared::behavior_subject::<HubItem, Infallible>(None));
    }

    /// Broadcast the finished singleton and close the carrier. Called once, by the
    /// request that owns the instantiation.
    pub(crate) fn settle(&mut self, key: &str, module: Arc<Module>) -> Result<(), SassError> {
        let Some(hub) = self.hubs.get_mut(key) else {
            return Err(unsettled(key));
        };
        // `next` then `complete` are plain `Observer` calls on the carrier, made
        // outside any subscription callback — the Subject re-entrancy panic cannot
        // be reached from here.
        <Hub as Observer<HubItem, Infallible>>::next(hub, Some(module));
        <Hub as Observer<HubItem, Infallible>>::complete(hub.clone());
        Ok(())
    }

    /// Join the multicast for `key` and take the instantiation it broadcasts. Every
    /// parent of a shared module lands here, and each gets the *same* `Arc`.
    pub(crate) fn share(&self, key: &str) -> Result<Arc<Module>, SassError> {
        // Cross-function boundary in the load pipeline: one span per join, so a
        // diamond reads as N `module_share` children under their `module_load` parent.
        let span = span!(
            Level::DEBUG,
            "module_share",
            stage = "multicast",
            module = %key,
            peeked = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = Instant::now();
        let hub = self.hubs.get(key).ok_or_else(|| unsettled(key))?;

        let (tx, rx) = mpsc::channel::<HubItem>();
        // `into_local` only re-wraps the context (it changes no execution model, per
        // the manual), which is what lets a closure `subscribe` apply: the error
        // channel is `Infallible` because nothing here fails out-of-band.
        let subscription = hub
            .clone()
            .into_local()
            .subscribe(move |item| {
                tx.send(item).ok();
            });
        let item = rx.recv().map_err(|_| unsettled(key))?;
        // Release the observer we just registered, dropping its `Sender`; without
        // this the hub would accumulate one leaked subscriber per parent.
        subscription.unsubscribe();

        span.record(
            "elapsed_ms",
            tracing::field::debug(started.elapsed().as_millis() as u64),
        );
        let module = item.ok_or_else(|| unsettled(key))?;
        span.record("peeked", module.css.len());
        Ok(module)
    }

    /// How many carriers are currently open — the size of the singleton set.
    pub(crate) fn len(&self) -> usize {
        self.hubs.len()
    }
}

/// The internal invariant violation: a module was requested through the bus before
/// its instantiation had been opened / published. Unreachable in a well-formed run,
/// reported rather than panicked so the pipeline keeps its in-stream `Result` protocol.
fn unsettled(key: &str) -> SassError {
    SassError::new(
        ErrorKind::Load,
        format!("internal: module hub for `{key}` was joined before it was settled"),
    )
    .note(key.to_string())
}
