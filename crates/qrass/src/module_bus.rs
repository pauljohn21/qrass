//! L2 module orchestration (design D1): the `@use` singleton as a `Local`
//! multicast.
//!
//! The manual prescribes that a module imported by several parents is *instantiated
//! once and broadcast*, not looked up and copied. The carrier is a `Local`
//! [`BehaviorSubject`] keyed by canonical path. Because the pipeline is synchronous
//! and single-threaded (the wasm contract below), a second parent only ever joins
//! after the first has settled the hub, so `share` reads that one instantiation
//! with `Behavior::peek` — no observer attaches and no per-parent subscription is
//! retained. The payload is `Rc<Module>` — with first-class callable values
//! (round 73 W1) `Module` is no longer `Sync`, so the read-only multicast
//! shares it single-threaded like every other Local payload in the crate.
//!
//! `Local` rather than `Shared` is the crate-wide wasm contract: the whole
//! pipeline must run on a single-threaded `wasm32-unknown-unknown` target, so no
//! stage may pay for — or require — the `Arc`/`Mutex` plumbing and `Send + Sync`
//! bounds a `Shared` carrier imposes. An earlier revision kept the hub `Shared`
//! to preserve a future concurrent-`@use`-branch step; that step was measured
//! (`src/branch_fanout.rs`: ~5% thread-parallel ceiling) and deliberately retired
//! in favour of wasm support. Per-item stage kernels were always `Local` — the
//! hub now shares that single context, so the crate has exactly one rxrust
//! context.
//!
//! Reading the singleton goes through rc.6's `Behavior::peek` rather than a
//! subscription: `share` takes the current value synchronously, registering no
//! observer. This is why the file needs neither [`crate::drain::drain`] nor an
//! `mpsc` terminal — the rc.5 late-subscriber deadlock (`Subject::is_closed` only
//! testing emptiness, so a subscriber attaching after `complete` never fired and
//! its `Sender` leaked) cannot arise when nothing subscribes. `peek` is only
//! exposed for a `BehaviorSubject` whose error channel is `()`, which is why the
//! hub is parameterized with `()` rather than `Infallible` (the payload itself
//! already carries the `None` "not settled yet" state).

use std::collections::HashMap;
use std::rc::Rc;

use rxrust::prelude::*;
use rxrust::rc::MutRc;
use rxrust::subject::behavior_subject::{Behavior, BehaviorSubject};
use tracing::{Level, span};

use crate::clock::StopWatch;
use crate::error::{ErrorKind, SassError};
use crate::module::Module;

/// What one hub multicasts: `None` while its body has not run yet, `Some` once the
/// singleton exists. Keeping the "not yet" state *inside* the payload is what lets a
/// request that arrives mid-instantiation join the same carrier instead of starting a
/// second one.
type HubItem = Option<Rc<Module>>;

/// The `Local` carrier for one canonical module path. `ObservableFactory` is bound
/// to `Context<Inner = ()>`, so `Local::behavior_subject` produces exactly this
/// spelling — which is why a hub can be a struct field instead of an unnameable chain.
/// The error channel is `()` (not `Infallible`) so the `Behavior` trait — and its
/// synchronous `peek` reader — is exposed for this carrier.
type Hub = Local<
    BehaviorSubject<
        // rc.6 wraps the subject payload in the context's `RcDeref` pointer
        // (`MutRc` = `Rc<RefCell<_>>` for `Local`); this is the framework's own
        // multicast plumbing, not a business carrier — the value we share stays the
        // read-only `Option<Rc<Module>>`.
        MutRc<HubItem>,
        SubjectPtr<'static, LocalCtx<(), LocalScheduler>, HubItem, ()>,
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
            .or_insert_with(|| Local::behavior_subject::<HubItem, ()>(None));
    }

    /// Broadcast the finished singleton and close the carrier. Called once, by the
    /// request that owns the instantiation.
    pub(crate) fn settle(&mut self, key: &str, module: Rc<Module>) -> Result<(), SassError> {
        let Some(hub) = self.hubs.get_mut(key) else {
            return Err(unsettled(key));
        };
        // `next` then `complete` are plain `Observer` calls on the carrier, made
        // outside any subscription callback — the Subject re-entrancy panic cannot
        // be reached from here.
        // rc.6: `Observer::next` takes `&mut self` and the payload is handed to
        // subscribers as an owned clone of the `RcDeref` target.
        hub.next(Some(module));
        hub.clone().complete();
        Ok(())
    }

    /// Join the multicast for `key` and take the instantiation it broadcasts. Every
    /// parent of a shared module lands here, and each gets the *same* `Arc`.
    pub(crate) fn share(&self, key: &str) -> Result<Rc<Module>, SassError> {
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
        let started = StopWatch::start();
        let hub = self.hubs.get(key).ok_or_else(|| unsettled(key))?;

        // rc.6 exposes the current value synchronously through `Behavior::peek`, so
        // the singleton is read directly — no observer is registered, no `mpsc`
        // channel is needed, and there is no late-subscriber / `recv()` hang surface
        // (the rc.5 workaround this replaces). The carrier stays a `Local`
        // `BehaviorSubject` (multicast structure preserved), not a bare lookup map.
        let item = hub.peek();

        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
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
