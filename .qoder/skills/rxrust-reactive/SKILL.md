---
name: rxrust-reactive
description: Write Rust reactive streams with the rxrust crate (1.0.0-rc.6) by using its operators directly — flat_map pipelines, Subject multicast, in-stream Result errors, box_it type erasure, a single terminal subscribe. Use when building or reviewing rxrust code (operators, chaining stages, multicasting, draining a stream to an owned result, Observable errors). Enforces ownership over Arc<Mutex> and forbids any self-made Stage/fail/drain framework layer.
---

# rxrust Reactive Streams (Rust, ownership-first, use-the-operators)

rxrust is the Rust port of ReactiveX. Its core is **Rust ownership**: items move *by value*
through operators. Use the operators directly — the pipeline IS a chain of them. Do **not**
build a framework abstraction on top of rxrust and then make rxrust run *your* design.

Version pin (critical):

```toml
rxrust = "=1.0.0-rc.6"   # bare `cargo add rxrust` resolves the OLD 0.15 line
```

Keep default features (`scheduler` gates `Local`/`Shared` + the `LocalBoxedObservable`
aliases). The 1.0 API differs fundamentally from both RxJS and rxrust 0.15. The authoritative
per-file walkthrough and the do/don't doctrine live in **`docs/rxrust-source-analysis/`** (01–09;
`09-cancellation-and-terminal-ordering.md` holds the cross-operator invariants — slot-before
subscribe, terminate-exactly-once, cancel-only-on-early-termination, lock discipline, Subject
re-entrancy); deep API notes in [reference.md](reference.md); `docs/rxrust-ai-manual.md` is the
older rc.5 copy kept for history only.

## The rule that keeps getting broken

Express control flow **as** rxrust operators; do not re-implement it and wrap it:

## Pick the verb BEFORE you write (this is the step I keep skipping)

Decide "which item flows, through which operator" using the framework's own inventory — read from
rc.6 `src/observable.rs` + `src/factory.rs`, not from memory: `map map_to filter filter_map scan
scan_map reduce reduce_initial collect collect_into take take_while take_while_inclusive take_until
skip skip_while skip_until skip_last take_last first first_or last last_or average count max min sum
distinct distinct_key distinct_until_changed distinct_until_key_changed contains pairwise merge
merge_all concat_all concat_map flat_map switch_map group_by zip combine_latest with_latest_from
sample throttle throttle_time debounce delay delay_at delay_subscription delay_unsubscription
buffer buffer_count buffer_time buffer_time_max observe_on subscribe_on start_with default_if_empty
tap finalize retry on_error on_complete map_err multicast publish ref_count into_future into_stream
box_it` — plus the factory sources (`of from_iter from_fn create defer empty never throw_err timer
interval from_future* from_stream* subject* behavior_subject* merge_observables concat_observables`).
There is **no `share()`, no `and_then`, no `take_until_by`** in rc.6 — verified by `grep "fn share"`.
**If none of them expresses the step, that is a blocker to report — not a licence to invent a
mechanism.** Writing first and consulting the source later is exactly how this project reworked
itself N times.

| Need | Use (directly) | Never (retrofit) |
|------|----------------|------------------|
| A loop / one item → many (`@each`, nesting fan-out) | `from_iter(items).flat_map(|i| sub_stream(i))` | buffer the body and replay with `for`/`while`/`for_each`; `from_fn`+`VecDeque` work-queue |
| A **phase** that produces 0..N items per input (lexing, structuring, rendering) | author a real operator kernel (see §Write a phase as a kernel) and `observer.next(..)` per item | `scan_map` returning a `Vec` + `flat_map(from_iter)` — a container where the framework emits items |
| **State that must travel as a value** (env, selector prefix) | `scan(seed, \|acc, item\| -> acc)` — the emitted item **IS** the accumulation, so state is read/moved downstream as data | `scan_map` with the state hidden in an `Acc` object; per-item `Arc<Env>`-style snapshots welded onto the data to fake "state follows the stream" |
| State purely private, only an output wanted | `scan_map(seed, \|&mut acc, item\| -> out)` / `reduce_initial` — no `Clone` needed | `Arc<Mutex<S>>` acc; an `Acc` that *drives* traversal with push/pop + control flags (`chain_taken`) |
| One producer → many consumers (`@use`, `@mixin`) | `Shared::subject` / `behavior_subject`+`peek` / `publish().ref_count()` — bus usage is `let mut e = bus.clone(); e.next(v)` then `bus.complete()` (consumes). **Never re-emit inside a subscriber callback**: a re-entrant `next`/`error`/`complete` on the same Subject **panics**; put `delay(Duration::ZERO)` between them | collect to a `Vec` and hand-feed each consumer |
| Two **independent** streams whose latest values must meet | `with_latest_from` / `combine_latest` | manual pairing buffers |
| ~~State of the event's own position~~ | **no verb does this** — measured (`tests/multicast.rs`): `publish().ref_count()` + `with_latest_from` still gives `[(1,6),(2,6),(3,6)]`, every event paired with the FINAL state, because a synchronous source drains into the first-subscribed branch. Use `scan` (row above). | reaching for pairing/multicast to fix it, then smuggling snapshots when pairing fails |
| Branch (`@if`) | `filter` on a value computed from moved inputs | a mutable `chain_taken`/state flag threaded by hand |
| Chain a stage per item | `flat_map` (there is **no `and_then`**) | sequential `run_stage` statements |
| Ordered merge vs concurrent merge | order-intrinsic → `concat_map`/`concat_all()`/`concat_observables()` (`concurrent: 1`, verified order-preserving). Real fan-out + deterministic output = ONE global pass (`collect` + a pure fold keyed by a deterministic `order` field) — never scattered `sort_by_key` after each merge | `flat_map` then "complain about ordering"; losing values because `merge_all(n)` **drops emissions of an inner while it is still queued** (verified: `test_merge_all_concurrency`) |
| Type erasure at a signature that truly needs it | `.box_it()` | naming the unnameable chain type (E0107) |
| Errors | the **Err channel is real**: `Err = SassError` + `map_err` / `on_error`, or in-stream `Result<T,E>` with `Err = Infallible`; close with `into_future()` → `Result<Result<T,E>,_>` | `.ok()` swallowing a failure into a fallback; `.unwrap_or_else(verbatim)` as an error policy; relying on `.subscribe(closure)` — it **silently drops** `error`/`complete` |

## Write a phase as a kernel (the framework's own shape — not a framework layer)

This is what `crates/sasspile/src/phases.rs` does for `Lexer` / `Structurize` / `Exec` / `Render`;
copy the shape from `ops/map.rs` when a phase emits 0..N items per input:

```text
struct Phase<S, Params> { source, params }      // lazy data, #[derive(Clone)], NO per-subscription state
struct PhaseObserver<O, State> { observer, state }   // state born per subscribe, moved not shared
impl ObservableType  → type Item<'a> = Out where Self: 'a; type Err = S::Err;
impl CoreObservable<C: Context>
  → subscribe(self, ctx): let w = ctx.transform(|o| PhaseObserver { observer: o, .. }); source.subscribe(w)
  → type Unsub = S::Unsub   (multi-source/higher-order: SingleAssignment<C::RcMut<State<U>>>::channel(),
                              slot before subscribing; unsubscribe only on early termination)
next(): emit each item with self.observer.next(..)      error(self)/complete(self): consume + forward
is_closed(): delegate downstream — that is how a source learns to stop early
```

A kernel that can fail **and** finish must deliver one terminal, not two: `Observer::error/complete` take
`self`, so hold the downstream observer as `Option<O>` and deliver through `self.observer.take()`
(`crates/sasspile/src/phases.rs` does exactly that). Without the `Option`, a phase that reports an error and
then reaches its normal end forwards both, and rxrust has no assert to catch it — the second delivery is
just a silent extra event on a stream the subscriber already closed.

Keep `Err` generic and use `C::RcMut`/`C::RcCell` for anything shared, so one kernel compiles for
`Local` (wasm32 / `!Send`) and `Shared`. Destructure `let PhaseObserver { observer, state } = self;`
when a closure needs the observer while the accumulator is borrowed.
This is **using** the framework's primitives (the same three impls every `ops/*.rs` file has),
not a `Stage`/`fail`/`drain` orchestration layer — the ban is on abstractions that replace its
control flow with yours.

## Do NOT build a framework layer

No `type Stage<T>`, no `fn fail<T>`, no `fn load_stage`, no `fn drain`/`drain_one`, no
`run_stage` orchestrator. That is a self-made framework that bends rxrust into running your
control flow (`rust.md` bans it; it is the recurring root of this project's rework). Compose
**one cold operator chain** straight on the source and end in a **single** `subscribe`.
Observable-typed *function boundaries* (`fn lex(input) -> BoxedObservable<Event>` then
`fn exec(events: BoxedObservable<Event>) -> BoxedObservable<Rule>`) are the same layer with the
edges sanded off: every such seam costs a `box_it()` and turns the chain into a shell that merely
ticks my own machine. Keep the chain in one place (the entry point) and let helpers move **values**,
not observables. `box_it()` stays legitimate where a signature genuinely cannot name the type.
Also forbidden: a recursive `stream(node)` that walks a materialized AST to build nested
observables — that is the same retrofit in another costume.

## Choosing Local vs Shared

- `Local` — single-threaded, no `Send`/`Sync`; works with `!Send` data. Non-timed operators run
  inline, so a synchronous chain needs no runtime.
- `Shared` — requires `Send + Sync + 'static` on Item/Err/closures/captures. This project's
  OpenSpec **chose** it, so follow that decision rather than a reflex; and remember choosing it
  is not the same as getting concurrency: `SharedScheduler` on native is `tokio::spawn`, but a
  per-item task with no `await` and microseconds of work is polled on the spawning worker
  (measured: one `ThreadId` for 32 items). Fan-out pays when the item carries real work
  (directive bodies, module compilation) — verify with `tracing` thread ids, never by asserting it.
- `.into_shared()` / `.into_local()` swap the wrapper only; non-timed sources still run inline.

## Execution model (what runs synchronously)

- `from_iter`, `of`, `create`, `empty`, `defer`, `from_fn`, and all non-timed combinators
  (`map`/`filter`/`scan_map`/`reduce_initial`/`collect`/`flat_map`/`concat_map`/`merge`/`zip`/
  `tap`…) run **fully inline during `.subscribe()`** — `rx.recv()` right after works with no runtime.
- Only **timed** operators (`delay`/`timer`/`interval`/`buffer_time`/`debounce`/`sample`/
  `throttle`/`observe_on`/`subscribe_on`) use the scheduler. **Avoid them in a synchronous pipeline.**

## Item by value vs by reference (check before writing)

- `map`/`filter_map`/`flat_map`/`scan_map`/`reduce` take item **by value** (move).
- `filter`/`tap` take `&Item`. `scan_map` = `(&mut Acc, Item) -> Out` each item;
  `reduce_initial` = `(Out, Item) -> Out` final once.

## Minimal source shape (a `create`-based lexer, not a driver)

```rust
use std::convert::Infallible;
use rxrust::observer::Emitter;          // NOT in prelude — import explicitly
use rxrust::prelude::*;

Local::create::<i32, Infallible, _, _>(|emitter: &mut dyn Emitter<i32, Infallible>| {
    emitter.next(1); emitter.next(2); emitter.complete();
})                                       // returns () = a Subscription
.collect::<Vec<_>>()
.subscribe(|v: Vec<i32>| { /* … */ });
```

## Single sources already fixed here (adding a second copy IS the regression)

Each of these was duplicated at some point and each duplication produced a measured wrong answer, so check
for an existing one before writing a helper:

| Law | Lives in | What duplicating it cost |
|---|---|---|
| convertible units (family + factor) | `crate::unit::UNITS` via `unit::conversion` | `math.rs` kept ten-decimal length literals; `hypot(3cm, 4mm*10, 5q*40, …)` → `11.6189500386cm` was unanswerable (`q` exists only in the shared table) |
| comparing two numbers under the unit law | `builtins/math.rs::compare` (+ `comparable` / `compare_named` renderings) | two message shapes were about to grow two comparisons; `min`/`max` name nothing while `clamp`/`hypot` name both parameters |
| `change`/`adjust`/`scale` channel writing | `builtins/color/modify.rs::route(given, writing)` × `Space` | reading a scale amount with the rgb-channel reader made `100%` mean 255 bytes and looked like a formula error |
| "must have no units" refusal | `builtins/math.rs::unitless` | per-function wording drift |
| number printing | `value.rs::format_number` (10 decimal places, sign of `-0` kept) + `Unit::calc_product` for non-finite values | `Unit::calc_form` prints `NaNdeg`; two printing laws was the bug |
| a name with a module prefix | `lexer::Tok::MemberVar` (decided at the character that starts `$`) | split `Word("math.")`+`Var("pi")` cannot be re-joined downstream: `math.$pi`, `math . $pi` and `5.$pi` are indistinguishable |
| how much one lexeme moves the bracket depth | `lexer::Tok::net`, folded over a run by `brackets::depth_of` | five copies of one law, and the argument-cut copy lacked the `Punct('(')` arm — a standalone group's `(` counted 0 while its `)` counted `-1`, so a comma inside `( … )` could read as top-level. No corpus row distinguishes the old reading from the new one (`changed=0`), which is exactly why the law sits on the lexeme instead of in each consumer |
| a hue is periodic | `axes::axes_to_rgb` (sector from `(hue / 60).rem_euclid(6.0)`) with an angle argument folded at the read by `color::angle_of` | the whiteness/blackness branch added two already-folded hues, handed the conversion 410 and took the **last** sector instead of 90's — green and blue came out swapped; the same file already folded with `rem_euclid(2.0)`, so the wrap was known and only partially applied |
| where a `@for`'s body ends | `structure::MsgAcc{depth, cut}`, which emits one owned `Event::For{head, body}`; both readers of the header (`loops::is_for` and `loops::for_loop`) go through `brackets::meaningful` and one `for_keyword` | `Exec` was discovering its own extent: a `Vec<Event>` buffer, an `open` brace counter and a `replay` re-entering `step`, and a block inside a loop body was dropped as inert (less CSS than the source held, invisible to a pass count). The phase that reads the braces already carries the depth as data — `brackets::Run{level, owner}` is the same shape for parentheses. What the split does **not** buy: the per-value expansion is still a re-feed in `exec::unroll`, not `flat_map` over a body sub-stream |

Related discipline that is not about rxrust but is measured here: **quote a number only from the command that
printed it**, and **a zero from an instrument pointed at the wrong place is not a finding** (`.hrx` files are
archives — there are no `input.scss` files on disk to grep). See
`docs/superpowers/specs/2026-10-28-cross-session-anti-gc-regression-design.md` for how these guards are meant
to survive a fresh session.

## Do / Don't recap

- Move values; `scan` (state **is** the emitted item) or `scan_map`/`reduce_initial` (private acc)
  carry the accumulator; close only through the framework's terminals — `collect`/`reduce_initial` +
  `into_future()`, `into_stream()`, or `await`ing the subscription handle. `std::sync::mpsc` is a
  *bridge* for an API that needs a `Sender`, **not** a terminal.
- One-to-many = `Subject` / `publish().ref_count()` (there is **no `share()`** in rc.6);
  loop = `flat_map`; branch = `filter`; state = owned accumulator value.
- Errors = in-stream `Result` + `Err = Infallible`; `subscribe(closure)` alone swallows `error`/`complete`.
- **Never** wrap rxrust in a `Stage/fail/drain` framework layer, a `from_fn`+`VecDeque`/hand
  state machine, or a recursive AST interpreter — those all mean "改造框架" and get rejected.
- No Python/scripts; no `println!` (use `tracing` spans recorded inside the executing closure);
  tests in `tests/` (`#[test]`, no inline); files ≤ 500 lines.
