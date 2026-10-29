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
//! on a `Local` (`!Send`) or `Shared` context and with any `Err`.

use std::convert::Infallible;

use rxrust::context::Context;
use rxrust::observable::{CoreObservable, ObservableType};
use rxrust::observer::Observer;

use crate::SassError;
use crate::exec::{State, StyleRule};
use crate::lexer::{Event, LexAcc, MsgAcc, Tok};
use crate::trace::Phase;

// The phase handle each observer carries is `crate::trace::Phase`: a span that lives as long as the
// subscription, so `elapsed_ms` covers the streaming rather than just the wiring of the chain.

/// `char → Tok`: one emission per completed token, driven by [`LexAcc`].
#[derive(Clone)]
pub struct Lexer<S> {
    pub source: S,
}

pub struct LexerObserver<O> {
    observer: O,
    acc: LexAcc,
    timing: Phase,
    items: usize,
}

impl<O, Err> Observer<char, Err> for LexerObserver<O>
where
    O: Observer<Tok, Err>,
{
    fn next(&mut self, ch: char) {
        // Destructuring keeps the accumulator, the downstream observer, the span and the counter as
        // separate borrows, so the sink can emit straight into the stream while counting its own output.
        let LexerObserver {
            observer,
            acc,
            timing,
            items,
        } = self;
        let _current = timing.enter();
        acc.feed(ch, &mut |tok| {
            observer.next(tok);
            *items += 1;
        });
    }

    fn error(self, err: Err) {
        let LexerObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.error(err);
    }

    fn complete(self) {
        let LexerObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.complete();
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
        let wrapped = context.transform(|observer| LexerObserver {
            observer,
            acc: LexAcc::default(),
            timing: Phase::new("lex", "lexer"),
            items: 0,
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
    timing: Phase,
    items: usize,
}

impl<O, Err> Observer<Tok, Err> for StructurizeObserver<O>
where
    O: Observer<Event, Err>,
{
    fn next(&mut self, tok: Tok) {
        let StructurizeObserver {
            observer,
            acc,
            timing,
            items,
        } = self;
        let _current = timing.enter();
        acc.feed(tok, &mut |event| {
            observer.next(event);
            *items += 1;
        });
    }

    fn error(self, err: Err) {
        let StructurizeObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.error(err);
    }

    fn complete(self) {
        let StructurizeObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.complete();
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
        let wrapped = context.transform(|observer| StructurizeObserver {
            observer,
            acc: MsgAcc::default(),
            timing: Phase::new("structure", "lexer"),
            items: 0,
        });
        source.subscribe(wrapped)
    }
}

/// `Event → StyleRule`: the program state (`scopes`, `env`) lives in this observer and is moved by
/// `&mut self` per event; only a rule the event settles is emitted. A fatal failure ends the stream
/// on the real `Err` channel (`Observer::error`), which is why the downstream observer is held as an
/// `Option` and consumed with `take()` — terminate exactly once, the way the framework's own
/// multi-source observers do it. Whether any kind is fatal today is the `FATAL` set in the crate root.
#[derive(Clone)]
pub struct Exec<S> {
    pub source: S,
}

pub struct ExecObserver<O> {
    observer: Option<O>,
    state: State,
    timing: Phase,
    items: usize,
}

impl<O> Observer<Event, Infallible> for ExecObserver<O>
where
    O: Observer<StyleRule, SassError>,
{
    fn next(&mut self, ev: Event) {
        if self.observer.is_none() {
            return; // already terminated on the Err channel
        }
        // The phase span is entered only around `step`, so the declaration spans it opens nest inside
        // it, and the guard is released before `close` moves the timing out.
        let settled = {
            let _current = self.timing.enter();
            self.state.step(ev)
        };
        let ExecObserver {
            observer,
            timing,
            items,
            ..
        } = self;
        match settled {
            Ok(Some(rule)) => {
                if let Some(downstream) = observer {
                    downstream.next(rule);
                    *items += 1;
                }
            }
            Ok(None) => {}
            Err(err) => {
                if let Some(terminated) = observer.take() {
                    timing.close(*items);
                    terminated.error(err);
                }
            }
        }
    }

    // The upstream is infallible; an error can only originate from this phase.
    fn error(self, never: Infallible) {
        match never {}
    }

    fn complete(self) {
        let ExecObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        if let Some(observer) = observer {
            observer.complete();
        }
    }

    fn is_closed(&self) -> bool {
        self.observer.as_ref().is_none_or(|o| o.is_closed())
    }
}

impl<S> ObservableType for Exec<S>
where
    S: ObservableType<Err = Infallible>,
{
    type Item<'a>
        = StyleRule
    where
        Self: 'a;
    type Err = SassError;
}

impl<S, C> CoreObservable<C> for Exec<S>
where
    C: Context,
    S: ObservableType<Err = Infallible> + CoreObservable<C::With<ExecObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Exec { source } = self;
        let wrapped = context.transform(|observer| ExecObserver {
            observer: Some(observer),
            state: State::default(),
            timing: Phase::new("exec", "exec"),
            items: 0,
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
    timing: Phase,
    items: usize,
}

impl<O, Err> Observer<StyleRule, Err> for RenderObserver<O>
where
    O: Observer<String, Err>,
{
    fn next(&mut self, rule: StyleRule) {
        let RenderObserver {
            observer,
            timing,
            items,
        } = self;
        let _current = timing.enter();
        observer.next(rule.to_css());
        *items += 1;
    }

    fn error(self, err: Err) {
        let RenderObserver {
            observer,
            mut timing,
            items,
        } = self;
        timing.close(items);
        observer.error(err);
    }

    fn complete(self) {
        let RenderObserver {
            observer,
            mut timing,
            items,
        } = self;
        timing.close(items);
        observer.complete();
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
        let wrapped = context.transform(|observer| RenderObserver {
            observer,
            timing: Phase::new("render", "exec"),
            items: 0,
        });
        source.subscribe(wrapped)
    }
}
