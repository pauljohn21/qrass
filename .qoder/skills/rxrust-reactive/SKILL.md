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

| Need | Use (directly) | Never (retrofit) |
|------|----------------|------------------|
| A loop / one item → many (`@each`, nesting fan-out) | `from_iter(items).flat_map(|i| sub_stream(i))` | buffer the body and replay with `for`/`while`/`for_each`; `from_fn`+`VecDeque` work-queue |
| Cross-element state (env, prefix) | `scan_map(seed, |&mut acc, item| …)` / `reduce_initial` — acc is owned, no `Clone` | `Arc<Mutex<S>>` acc; a shared mutable `Acc` object that *drives* traversal via push/pop + control flags |
| One producer → many consumers (`@use`, `@mixin`) | `Shared::subject` / `behavior_subject`+`peek` / `publish().ref_count()` / `share()` | collect to a `Vec` and hand-feed each consumer |
| Branch (`@if`) | `filter` on a value computed from moved inputs | a mutable `chain_taken`/state flag threaded by hand |
| Chain a stage per item | `flat_map` (there is **no `and_then`**) | sequential `run_stage` statements |
| Pass a stream across a fn boundary | `.box_it()` → `SharedBoxedObservable`/`LocalBoxedObservable` | naming the unnameable chain type (E0107) |
| Errors | in-stream `Result<T,E>` + `Err = Infallible`; `subscribe` sends to `mpsc` | rely on `.subscribe(closure)` — it **silently drops** `error`/`complete` |

## Do NOT build a framework layer

No `type Stage<T>`, no `fn fail<T>`, no `fn load_stage`, no `fn drain`/`drain_one`, no
`run_stage` orchestrator. That is a self-made framework that bends rxrust into running your
control flow (`rust.md` bans it; it is the recurring root of this project's rework). Compose
**one cold operator chain** straight on the source and end in a **single** `subscribe`.
Per-item stage functions that take an owned value and return an observable, and `box_it()` at
a boundary, are fine. Also forbidden: a recursive `stream(node)` that walks a materialized
AST to build nested observables — that is the same retrofit in another costume.

## Choosing Local vs Shared

- `Local` — single-threaded, no `Send`/`Sync`; works with `!Send` data. Right default for a
  synchronous compile: non-timed operators run inline, zero locks.
- `Shared` — requires `Send + Sync + 'static` on Item/Err/closures/captures. Use it only for
  genuinely cross-thread work (this project's `@use` module fan-out / parallel independent
  streams per its OpenSpec). **Do not force it**: it buys nothing for a synchronous chain and
  pushes `Arc`/`Mutex`/`Send` onto every value — which is the GC/shared-state smell, not
  "using the framework". Immutable finished results may still be shared as `Arc<T>` (read-only),
  never `Arc<Mutex<business state>>`.
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
