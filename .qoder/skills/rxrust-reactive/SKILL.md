---
name: rxrust-reactive
description: Build reactive streams in Rust with the rxrust crate (1.0.0-rc.5) using ownership semantics and a flat_map-driven pipeline. Use when writing rxrust Observables/operators, chaining compilation/processing stages, multicasting via Subject, draining a stream to an owned result, or handling Observable errors. Enforces ownership over Arc<Mutex>, in-stream Result error protocol, and LocalBoxedObservable type erasure across function boundaries.
---

# rxrust Reactive Streams (Rust, ownership-first, flat_map pipelines)

rxrust is the Rust port of ReactiveX. Its core is **Rust ownership**: items move
*by value* through operators. Do **not** carry business data in
`Arc<Mutex<_>>` — that shared mutability is only the framework's internal plumbing
for multicasting (Subject). Write pipelines that move owned values and drain them
through channels.

Version pin (critical):

```toml
rxrust = "=1.0.0-rc.5"   # bare `cargo add rxrust` resolves the OLD 0.15 line
```

Keep default features (the `scheduler` feature gates `Local`/`Shared` and the
`LocalBoxedObservable` aliases). The 1.0 API differs fundamentally from both RxJS
and rxrust 0.15.

## Golden rules

| Need | Do (ownership) | Don't (GC-style) |
|------|----------------|------------------|
| Streaming state + per-item output | `scan_map(seed, \|&mut acc, item\| -> out)` — acc needs NO `Clone` | `scan(Arc<Mutex<S>, …) + flat_map` |
| Fold to one value | `reduce_initial(owned_acc, \|acc, item\| -> acc)` or `collect` | `Arc<Mutex<acc>>` + lock-back |
| Chain a stage per item | **`flat_map`** — each emission drives the next stage's stream | sequential `run_stage` statements; there is **no `and_then`** in rc.5 |
| Pass a stream across fn boundaries | `.box_it()` → `LocalBoxedObservable<'a, Item, Err>` | naming the unnameable chain type (E0107) |
| Errors | **in-stream `Result<T,E>` + `Err = Infallible`** protocol, drained at terminal | `.subscribe(closure)` — it **silently drops** `error`/`complete` |
| Move result out | `mpsc::channel(); …subscribe(move\|v\| {tx.send(v).ok();}); rx.recv()` | `Arc<Mutex<Option<T>>>` + `try_unwrap` |

## Pipeline architecture (the qrass template)

Stages are cold stream **functions**; stages connect via **`flat_map`** (flow-driven,
not sequential statements); errors ride as in-stream `Result` data; the builder holds
only owned config; a single mpsc terminal collects. See [reference.md](reference.md)
§8 for the full `Stage<T>` / `fail<T>` / `flat_map` orchestration recipe with tracing
spans.

```rust
type Stage<T> = LocalBoxedObservable<'static, Result<T, SassError>, Infallible>;
fn fail<T>(e: SassError) -> Stage<T> { Local::of(Err(e)).box_it() }

load_stage(entry)
    .flat_map(|r| match r { Ok(l) => lex_stage(l),   Err(e) => fail(e) })
    .flat_map(|r| match r { Ok(l) => parse_stage(l), Err(e) => fail(e) })
    .flat_map(|r| match r { Ok(p) => eval_stage(p),  Err(e) => fail(e) })
    .map(|r| r.map(|done| done.css))
    .subscribe(move |out| { tx.send(out).ok(); });
```

`flat_map` requires both branches to be the **same** `Stage<T>` — a normal stage and
`fail(e)` unify because both erase `Item = Result<T, SassError>`, `Err = Infallible`.

## Choosing Local vs Shared

- `Local` — single-threaded, no `Send`/`Sync` bounds, works with `!Send` types.
  **qrass uses `Local` for the entire pipeline** (design D1, wasm contract): the
  per-item stage kernels (char→token lex, token→AST parse, eval emission, render
  fold) *and* the module orchestration — module singleton = `Local` multicast
  (`Local::behavior_subject`, one instantiation fan-out to N parents, `Arc<Module>`
  read-only publish), `@use` branch fan-out via `Local::from_iter + flat_map`
  executed inline. Rc-based plumbing, zero locks, runs on `wasm32-unknown-unknown`.
- `Shared` — requires Item, Err, and every closure/captured value to be `Send + Sync + 'static`.
  Kept available in rxrust for genuinely cross-thread streams, but qrass deliberately
  does not use it: the thread-parallel ceiling measured ~5%, and `Shared` would break
  the wasm target (its `Arc`/`Mutex` + `Send` bounds and the tokio scheduler base are
  unusable in the browser). Immutable finished modules are still shared as
  `Arc<Module>` (read-only data, NOT `Arc<Mutex<business state>>`).
- Convert: `.into_shared()` / `.into_local()` (context.rs — swaps the wrapper, changes
  no execution model for non-timed sources; they still run inline).

## Execution model (what is synchronous)

- `from_iter`, `of`, `create`, `empty`, and all non-timed combinators
  (`map`/`filter`/`scan_map`/`reduce_initial`/`collect`/`flat_map`/`tap`/…) run
  **fully inline during `.subscribe()`**. So `rx.recv()` right after `.subscribe()`
  works without any runtime.
- Only **timed** operators (`delay`, `timer`, `interval`, `buffer_time`, `debounce`,
  `sample`, `throttle`, `observe_on`, `subscribe_on`) invoke the scheduler
  (`Local`→`tokio::task::spawn_local`). **Avoid them in synchronous pipelines.**

## Item by value vs by reference (operator-specific — check before writing)

- `map` / `filter_map` / `flat_map` closures take `Item<'a>` **by value** (move).
- `filter` / `tap` take `&Item<'a>` (borrow). To mutate-then-emit use `map`/`filter_map`.
- `scan_map` takes `(&mut Acc, Item<'a>) -> Output` — emits Output each item.
- `reduce_initial` takes `(Output, Item<'a>) -> Output` — emits final Output once on complete.

## Minimal source shape

```rust
use rxrust::prelude::*;
use std::convert::Infallible;
use rxrust::observer::Emitter;              // NOT in prelude — always import explicitly

Local::create::<i32, Infallible, _, _>(
    |emitter: &mut dyn Emitter<i32, Infallible>| {
        emitter.next(1); emitter.next(2); emitter.complete();
    })                                       // closure returns () which is a Subscription
    .collect::<Vec<_>>()
    .subscribe(|v| { /* v: Vec<i32> */ });
```

## Full operator catalog, terminals, Subject, RxJS/0.15 differences, compile-error
remediation, and the complete compiler-pipeline recipe are in
[reference.md](reference.md). The authoritative source-read verification lives in
`docs/rxrust-ai-manual.md`.

## Do / Don't recap

- Don't put tokens, ASTs, buffers, or accumulators in `Arc<Mutex<_>>`. Move them as
  `scan_map`/`reduce_initial` state and drain via `mpsc`.
- Don't rely on `.subscribe(closure)` for failing streams — it drops `error`/`complete`.
  Use the in-stream `Result` protocol, `subscribe_with`, `on_error`, or `into_future`.
- Don't copy RxJS patterns (`pipe`, free `of(...)` operators, reference-passed values)
  or 0.15 patterns (`Arc<Mutex>` accumulators, `and_then` — **deleted in rc.5**).
- Don't pull a tokio runtime into a purely synchronous pipeline; skip the timed operators.
