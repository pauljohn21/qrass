---
name: rxrust-source-analysis
description: Use when writing, reviewing, or debugging Rust code that uses the rxrust crate (1.0.0-rc.6) and the real behaviour of a primitive must be confirmed — operator signatures and emission timing, Local vs Shared context, Observer/Subscription contracts, error propagation, Subject multicast and ref_count, box_it type erasure, scheduler requirements, or why subscribe(closure) swallows errors.
---

# rxrust Source Analysis

## Overview

A source-level reference for `rxrust = 1.0.0-rc.6`, from a line-by-line read of the
crate's `src/` (measured: **88 `.rs` files / 29,755 lines**). **Core principle: never state an
rxrust behaviour from memory — every claim here carries `file:line` provenance, so quote
that instead of guessing.**

## When to Use

- An operator's signature bounds, item-by-value-or-ref, emission point, or
  error/completion behaviour must be checked.
- A `subscribe(|v| ...)` closure never sees `error`/`complete`.
- `and_then` is missing; stages must be chained.
- Deciding `Local` vs `Shared`, or `into_shared` / `into_local` / wasm32 `Send` friction.
- One output must feed several consumers (`Subject`, `behavior_subject`, `publish`, `ref_count`).
- An unnameable chain type across a function boundary (`box_it`).
- Reviewing rxrust code that smells like GC: `Arc<Mutex<business state>>`, hand-rolled
  `VecDeque` + `from_fn` drivers, materialized `Vec<Rule>` walked recursively.

Not for: rxrust 0.15 (different API) or non-Rust ReactiveX.

## Quick Reference — question → chapter

| Confirm what | Read |
|---|---|
| push model, `ObservableType`/`CoreObservable`/`Observable` contract, subscribe = nested Observer wrapping, cold vs hot, sync execution, error protocol | [01-philosophy-and-architecture.md](references/01-philosophy-and-architecture.md) |
| `Scope`/`Context`/`LocalCtx`/`SharedCtx`, `Local`/`Shared`, `MutRc`/`MutArc`/`CellRc`/`SharedCell` | [02-context-scope-rc.md](references/02-context-scope-rc.md) |
| `Observer`/`Emitter`/`FnMutObserver`, `DynObserver`, `Subscription` combinators, `SingleAssignment` install-vs-cancel race | [03-observer-subscription.md](references/03-observer-subscription.md) |
| `factory.rs` sources (`of`/`from_iter`/`create`/`defer`/`from_*`/`timer`/`interval`/`empty`/`never`/`throw_err`); wasm32 notes | [04-observable-trait-and-sources.md](references/04-observable-trait-and-sources.md) |
| any of the **51** `ops.rs` modules (grouped A–J) + the pick-an-operator table | [05-operators.md](references/05-operators.md) |
| `Subject<P>` re-entrancy, `Subscribers`, `BehaviorSubject` + `peek`, `fork`/`connect`/`ref_count` | [06-subject-multicast-connectable.md](references/06-subject-multicast-connectable.md) |
| `box_it`/`BoxedCoreObservable`/`DynCoreObservable`; `Task`/`TaskHandle`/`Scheduler`/`SleepProvider`, `LocalScheduler`/`SharedScheduler` | [07-type-erasure-and-scheduler.md](references/07-type-erasure-and-scheduler.md) |
| pipeline recipe for compilers, loop/state/tree/multicast idioms, pitfall table, Do/Don't | [08-recipes-and-pitfalls.md](references/08-recipes-and-pitfalls.md) |
| cancellation vs termination: slot-before-subscribe, terminate-exactly-once, cancel-only-on-early-termination, lock discipline, Subject re-entrancy, doc-vs-impl list | [09-cancellation-and-terminal-ordering.md](references/09-cancellation-and-terminal-ordering.md) |

Chapter index and reading order: [references/README.md](references/README.md).

## Version Pin (critical)

```toml
rxrust = "=1.0.0-rc.6"   # bare `cargo add rxrust` resolves the old 0.15 line
```

Default feature `scheduler` gates `Local`/`Shared` and the boxed aliases; `nightly`
changes `map`'s closure signature and borrowed-item output.

## Lookup contract

1. Classify the question: source, operator, observer/subscription, context, multicast, erasure, or scheduler.
2. Open the mapped chapter only.
3. Cite the `file:line` the chapter gives; do not re-derive it.
4. If the chapter is silent, say so and read the crate source — do not extrapolate.

**Red flags — stop and look it up:** "rxrust has `and_then`", "`subscribe` will report errors",
"this needs `Arc<Mutex>`", "I'll wrap a `VecDeque` in `from_fn`",
"I'll `collect` the output and feed each consumer" (that is what multicast is for, `06 §8`).

## Notes

- For a misbehaving stream, gather tracing evidence first
  (**REQUIRED BACKGROUND:** superpowers:systematic-debugging).
- Chapter 08 §5 is the `sasspile`/`sasslint` acceptance loop of the origin project —
  a worked example, not a requirement elsewhere. Chapters 01–07 are project-neutral.
- `rxrust` is already the engine (`flat_map`, owned accumulators, cold-stream
  laziness); do not rebuild a stream engine on top of it.
