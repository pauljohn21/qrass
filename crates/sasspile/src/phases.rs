//! Pipeline phase kernels — each compile phase is a **real rxrust operator**, in the framework's
//! own shape:
//!
//! * `struct Phase<S> { source }` — lazy data, no per-subscription state, `Clone`;
//! * `PhaseObserver<O> { observer, state }` — the state lives here and is born with the
//!   subscription; `next` does the phase's work and forwards **each** produced item with its own
//!   `observer.next(..)`, `error`/`complete` are consumed-once pass-throughs, `is_closed`
//!   delegates downstream so the source can stop early;
//! * `impl ObservableType` + `impl CoreObservable<C: Context>` — `subscribe` wraps the context's
//!   inner observer with `C::transform` (which preserves the scheduler) and hands the wrapped
//!   context to the source.
//!
//! `Err` stays generic: a phase never decides the chain's error channel, so the same kernel works
//! on a `Local` (wasm32, `!Send`) or `Shared` context and with any `Err`.

use rxrust::context::Context;
use rxrust::observable::{CoreObservable, ObservableType};
use rxrust::observer::Observer;

use crate::exec::{State, StyleRule};
use crate::lexer::{Event, LexAcc, MsgAcc, Tok};

/// `char → Tok`: one emission per completed token, driven by [`LexAcc`].
#[derive(Clone)]
pub struct Lexer<S> {
    pub source: S,
}

pub struct LexerObserver<O> {
    observer: O,
    acc: LexAcc,
}

impl<O, Err> Observer<char, Err> for LexerObserver<O>
where
    O: Observer<Tok, Err>,
{
    fn next(&mut self, ch: char) {
        // Destructuring keeps the accumulator and the downstream observer as two disjoint
        // borrows, so the sink can emit straight into the stream.
        let LexerObserver { observer, acc } = self;
        acc.feed(ch, &mut |tok| observer.next(tok));
    }

    fn error(self, err: Err) {
        self.observer.error(err);
    }

    fn complete(self) {
        self.observer.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Lexer<S>
where
    S: ObservableType,
{
    type Item<'a>
        = Tok
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Lexer<S>
where
    C: Context,
    S: CoreObservable<C::With<LexerObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Lexer { source } = self;
        let span = tracing::debug_span!("phase", stage = "lex");
        let _entered = span.enter();
        let wrapped = context.transform(|observer| LexerObserver {
            observer,
            acc: LexAcc::default(),
        });
        source.subscribe(wrapped)
    }
}

/// `Tok → Event`: structural events on the delimiters, driven by [`MsgAcc`].
#[derive(Clone)]
pub struct Structurize<S> {
    pub source: S,
}

pub struct StructurizeObserver<O> {
    observer: O,
    acc: MsgAcc,
}

impl<O, Err> Observer<Tok, Err> for StructurizeObserver<O>
where
    O: Observer<Event, Err>,
{
    fn next(&mut self, tok: Tok) {
        let StructurizeObserver { observer, acc } = self;
        acc.feed(tok, &mut |event| observer.next(event));
    }

    fn error(self, err: Err) {
        self.observer.error(err);
    }

    fn complete(self) {
        self.observer.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Structurize<S>
where
    S: ObservableType,
{
    type Item<'a>
        = Event
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Structurize<S>
where
    C: Context,
    S: CoreObservable<C::With<StructurizeObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Structurize { source } = self;
        let span = tracing::debug_span!("phase", stage = "structure");
        let _entered = span.enter();
        let wrapped = context.transform(|observer| StructurizeObserver {
            observer,
            acc: MsgAcc::default(),
        });
        source.subscribe(wrapped)
    }
}

/// `Event → StyleRule`: the program state (`scopes`, `env`, order counter) lives in this
/// observer and is moved by `&mut self` per event; only a rule the event settles is emitted.
#[derive(Clone)]
pub struct Exec<S> {
    pub source: S,
}

pub struct ExecObserver<O> {
    observer: O,
    state: State,
}

impl<O, Err> Observer<Event, Err> for ExecObserver<O>
where
    O: Observer<StyleRule, Err>,
{
    fn next(&mut self, ev: Event) {
        let ExecObserver { observer, state } = self;
        if let Some(rule) = state.step(ev) {
            observer.next(rule);
        }
    }

    fn error(self, err: Err) {
        self.observer.error(err);
    }

    fn complete(self) {
        self.observer.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Exec<S>
where
    S: ObservableType,
{
    type Item<'a>
        = StyleRule
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Exec<S>
where
    C: Context,
    S: CoreObservable<C::With<ExecObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Exec { source } = self;
        let span = tracing::debug_span!("phase", stage = "exec");
        let _entered = span.enter();
        let wrapped = context.transform(|observer| ExecObserver {
            observer,
            state: State::default(),
        });
        source.subscribe(wrapped)
    }
}

/// `StyleRule → String`: serialise one settled rule to expanded CSS text, 1:1, in the order the
/// rules arrive. No task hand-off and no re-sorting: the chain is serial here, so the stream's own
/// order **is** document order. The tail it replaces (`from_stream` per rule + `collect` +
/// `sort_by_key`) was concurrency theatre over pure CPU work, and worse: a queued inner in
/// `merge_all` **loses** the emissions made while it waits, which no sort can bring back.
#[derive(Clone)]
pub struct Render<S> {
    pub source: S,
}

pub struct RenderObserver<O> {
    observer: O,
}

impl<O, Err> Observer<StyleRule, Err> for RenderObserver<O>
where
    O: Observer<String, Err>,
{
    fn next(&mut self, rule: StyleRule) {
        self.observer.next(rule.to_css());
    }

    fn error(self, err: Err) {
        self.observer.error(err);
    }

    fn complete(self) {
        self.observer.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Render<S>
where
    S: ObservableType,
{
    type Item<'a>
        = String
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Render<S>
where
    C: Context,
    S: CoreObservable<C::With<RenderObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Render { source } = self;
        let span = tracing::debug_span!("phase", stage = "render");
        let _entered = span.enter();
        let wrapped = context.transform(|observer| RenderObserver { observer });
        source.subscribe(wrapped)
    }
}
