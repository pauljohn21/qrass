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
per-file walkthrough and the do/don't doctrine live in **`docs/rxrust-source-analysis/`**;
deep API notes in [reference.md](reference.md); `docs/rxrust-ai-manual.md` is the older copy.

## The rule that keeps getting broken

Express control flow **as** rxrust operators; do not re-implement it and wrap it:

## Pick the verb BEFORE you write (this is the step I keep skipping)

Decide "which item flows, through which operator" using the framework's own inventory — read from
rc.6 `src/observable.rs`, not from memory: `map map_to filter filter_map scan scan_map reduce
reduce_initial take take_while take_while_inclusive take_until skip skip_while skip_until
skip_last take_last first first_or last last_or distinct distinct_key distinct_until_changed
distinct_until_key_changed contains pairwise merge combine_latest with_latest_from merge_all
concat_all flat_map switch_map zip buffer buffer_count collect collect_into default_if_empty
start_with tap finalize retry on_error map_err on_complete multicast publish ref_count share
group_by box_it into_future into_stream observe_on subscribe_on`.
**If none of them expresses the step, that is a blocker to report — not a licence to invent a
mechanism.** Writing first and consulting the source later is exactly how this project reworked
itself N times.

| Need | Use (directly) | Never (retrofit) |
|------|----------------|------------------|
| A loop / one item → many (`@each`, nesting fan-out) | `from_iter(items).flat_map(|i| sub_stream(i))` | buffer the body and replay with `for`/`while`/`for_each`; `from_fn`+`VecDeque` work-queue |
| **State that must travel as a value** (env, selector prefix) | `scan(seed, \|acc, item\| -> acc)` — the emitted item **IS** the accumulation, so state is read/moved downstream as data | `scan_map` with the state hidden in an `Acc` object; per-item `Arc<Env>`-style snapshots welded onto the data to fake "state follows the stream" |
| State purely private, only an output wanted | `scan_map(seed, \|&mut acc, item\| -> out)` / `reduce_initial` — no `Clone` needed | `Arc<Mutex<S>>` acc; an `Acc` that *drives* traversal with push/pop + control flags (`chain_taken`) |
| One producer → many consumers (`@use`, `@mixin`) | `Shared::subject` / `behavior_subject`+`peek` / `publish().ref_count()` / `share()` — bus usage is `let mut e = bus.clone(); e.next(v)` then `bus.complete()` (consumes) | collect to a `Vec` and hand-feed each consumer |
| Two **independent** streams whose latest values must meet | `with_latest_from` / `combine_latest` | manual pairing buffers |
| ~~State of the event's own position~~ | **no verb does this** — measured (`tests/multicast.rs`): `publish().ref_count()` + `with_latest_from` still gives `[(1,6),(2,6),(3,6)]`, every event paired with the FINAL state, because a synchronous source drains into the first-subscribed branch. Use `scan` (row above). | reaching for pairing/multicast to fix it, then smuggling snapshots when pairing fails |
| Branch (`@if`) | `filter` on a value computed from moved inputs | a mutable `chain_taken`/state flag threaded by hand |
| Chain a stage per item | `flat_map` (there is **no `and_then`**) | sequential `run_stage` statements |
| Ordered merge vs concurrent merge | `concat_all()` = `merge_all(1)` for order-intrinsic; `flat_map`/`merge_all(n)` + a document-order key + one stable sort for real fan-out | `flat_map` and then complain about ordering; a `sort` where `concat_all` was the verb |
| Type erasure at a signature that truly needs it | `.box_it()` | naming the unnameable chain type (E0107) |
| Errors | the **Err channel is real**: `Err = SassError` + `map_err` / `on_error`, or in-stream `Result<T,E>` with `Err = Infallible`; close with `into_future()` → `Result<Result<T,E>,_>` | `.ok()` swallowing a failure into a fallback; `.unwrap_or_else(verbatim)` as an error policy; relying on `.subscribe(closure)` — it **silently drops** `error`/`complete` |

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

## Do / Don't recap

- Move values; `scan_map`/`reduce_initial` carry owned accumulators; drain only via `mpsc`/`into_future`.
- One-to-many = `Subject`/`share`/`ref_count`; loop = `flat_map`; branch = `filter`; state = owned acc.
- Errors = in-stream `Result` + `Err = Infallible`; `subscribe(closure)` alone swallows `error`/`complete`.
- **Never** wrap rxrust in a `Stage/fail/drain` framework layer, a `from_fn`+`VecDeque`/hand
  state machine, or a recursive AST interpreter — those all mean "改造框架" and get rejected.
- No Python/scripts; no `println!` (use `tracing` spans recorded inside the executing closure);
  tests in `tests/` (`#[test]`, no inline); files ≤ 500 lines.
