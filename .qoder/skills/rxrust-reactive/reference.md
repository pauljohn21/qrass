# rxrust 1.0.0-rc.6 reference (source-verified)

Deep API notes backing [SKILL.md](SKILL.md). Every signature below was read from
`rxrust-1.0.0-rc.6` source (observable.rs / context.rs / factory.rs / observer.rs /
ops/* / observable/boxed.rs / rc.rs / subject.rs). For the full per-file walkthrough and
the do/don't doctrine see `docs/rxrust-source-analysis/`. If the rxrust version changes,
re-verify against source.

**rc.5 → rc.6 delta (verified against source):** the core model is unchanged — `Item<'a>`
is still a GAT, `and_then` is still absent (use `flat_map`/`concat_map`), `subscribe(closure)`
still swallows `error`/`complete`, and non-timed operators still run inline at `subscribe`.
What is **new/renamed** is `src/rc.rs`: shared-mutable primitives are abstracted behind the
`RcDeref`/`RcDerefMut`/`SharedCell` traits over `MutRc`/`MutArc`/`CellRc`/`CellArc` (built on
the `rclite::{Rc, Arc}` crates, not `std`), and `Subject`/`behavior_subject` payloads now ride
that layer (see §9 and the new §13). `collect_into(initial)` is added (§5.2).

## 1. Architecture: three trait layers + Context wrapper

```text
ObservableType          types only:  type Item<'a> where Self:'a;  type Err;
  ▲
CoreObservable<O>       subscribe kernel: type Unsub: Subscription;
                        fn subscribe(self, observer: O) -> Self::Unsub;  (consumes chain)
  ▲
Context                 execution wrapper: type Inner; type Scheduler: Clone+Default;
                        type With<T>;  lift / transform / swap / into_inner / scheduler
                        type BoxedCoreObservable<'a, Item, Err>;  (GAT alias)
  ▲
Observable              user API — blanket impl<T: Context where T::Inner: ObservableType>
                        map/filter/scan_map/flat_map/... all hang here
```

- **`Item` is a GAT** (`type Item<'a>`), not an associated type. Value streams:
  `Item<'a> = T`; mut-ref streams: `Item<'a> = &'a mut T`. Consequence: **a generic
  function cannot bound `O: Observable<Item = T>`** (E0107). Either assemble the
  chain inline in one function, or erase with `.box_it()` (§7).
- `Local<T> = LocalCtx<T, LocalScheduler>`, `Shared<T> = SharedCtx<T, SharedScheduler>`
  (context.rs L358/361). `T` is the Inner core observable, **not** the Item.
- The type of a user chain = `LocalCtx<deeply nested core type>` — unnameable; must
  `.box_it()` to cross function boundaries.
- `Observable::subscribe(f)` where `f: for<'a> FnMut(Self::Item<'a>)` (observable.rs
  L141) lifts into `FnMutObserver` — **error/complete are no-ops, silently dropped.**

## 2. Local vs Shared decision table

| Scenario | Pick | Constraint |
|---|---|---|
| Single-thread, synchronous pipeline | `Local` | no Send/Sync needed |
| Truly cross-thread emit/subscribe | `Shared` | Item/Err/closures `Send(+Sync)` |
| Switch context | `.into_shared()` / `.into_local()` (context.rs L450/462) | swaps wrapper only; execution model unchanged |

Decision for THIS compiler: `Local` is the default for the synchronous pipeline; `Shared`
+ threads are used only for genuinely cross-thread work (per the OpenSpec design, not a
framework rule). Do not force `Shared` where there is no concurrent work — it only adds
`Send + Sync + 'static` bounds.

## 3. Execution model (source semantics)

- `create/of/from_iter/empty/never/throw_err` and all non-timed operators run
  **fully inline at `subscribe`**. So `rx.recv()` right after `.subscribe()` always
  has a value; no runtime required.
- Only timed sources (`timer/interval`) and timed operators (`delay/buffer_time/
  debounce/sample/throttle`) call the Scheduler (Local→`spawn_local`, needs tokio).
- `flat_map` inner concurrency defaults to `usize::MAX` (sync inner == sequential).
  Strict order → `concat_map` (same impl, concurrent=1). Drop old inner on new →
  `switch_map`.

## 4. Factories (`ObservableFactory`, blanket-provided on `Local`/`Shared`)

```text
Local::create<Item, Err, F, U>(f)   F: FnOnce(&mut dyn Emitter<Item,Err>) -> U, U: Subscription
Local::of(v)                        single value + complete
Local::empty()                      completes immediately
Local::never()                      never emits (control with take/drop)
Local::throw_err(e)                 errors immediately
Local::from_iter(iter)              I: IntoIterator, emits each item synchronously
Local::from_fn(f)                   calls f per subscription to produce an Item
Local::defer(f)                     rebuilds a chain per subscription: F: Fn() -> O(Context)
Local::subject::<Item,Err>()        multicast (§9)
Local::subject_mut_ref::<Item,Err>()multicast, mut-ref dispatch
Local::behavior_subject(initial)    cached-latest multicast (Item: Clone)
Local::from_future(fut)             async single-value source (needs runtime)
Local::from_future_result(fut)      Future<Output=Result<Item,Err>> → value/error split
Local::from_stream(st)              Stream → multi-value async source
Local::timer(d)/timer_at(i)         timed single emit (needs runtime)
Local::interval(period)             timed multi emit (needs runtime)
Local::merge_observables(vec)       merge many same-type streams (concurrent)
Local::concat_observables(vec)      subscribe many sequentially (order guaranteed)
```

The `create` closure parameter is a **trait object** `&mut dyn Emitter<Item, Err>`
(`rxrust::observer::Emitter`, **NOT in prelude** — import explicitly). `Emitter` has
`next(&mut self, Item)`, `error(&mut self, Err)`, `complete(&mut self)` — all
`&mut self` (unlike `Observer` whose error/complete consume `self`). Return `U:
Subscription` — a synchronous source can return `()`. Use turbofish
`::<Item, Err, _, _>` when inference can't fix Item/Err (e.g. empty or error-only
streams). Return `ClosureSubscription(move || { /*teardown*/ })` if cleanup is needed.

## 5. Operator catalog (verified signatures — HRTB `for<'a> FnMut` throughout)

### 5.1 Transformation
```text
map(F)          F: for<'a> FnMut(Item<'a>) -> Out                 value in, value out (move)
map_to(v)       v: Clone                                          constant per emission
map_err(F)      map the error channel (does not swallow)
filter(F)       F: for<'a> FnMut(&Item<'a>) -> bool               ★ borrows &Item
filter_map(F)   F: for<'a> FnMut(Item<'a>) -> Option<Out>         ★ value in, Option out
tap(F)          F: for<'a> FnMut(&Item<'a>)                       ★ read-only side effect
```

### 5.2 Stateful fold (ownership-friendly — the compiler's core weapons)
```text
scan(seed, F)            Output: Clone required; F: FnMut(Output, Item) -> Output  (emits each)
scan_map(seed, F)        F: for<'a> FnMut(&mut Acc, Item<'a>) -> Output   ★ Acc needs NO Clone; emits Output each item
reduce_initial(Out, F)   F: for<'a> FnMut(Out, Item<'a>) -> Out           ★ emits final Out once on complete
reduce(F)                F: FnMut(Item<'a>, Item<'a>) -> Item<'a>         no seed; empty stream emits nothing
collect::<C>()           C: Default + Extend<Item>; emits one C after complete  ★ single terminal item
collect_into(col)        rc.6: fold items into a pre-supplied owned collection (no C: Default needed)  ★ emits col
last()                   emits only the final value
pairwise()               emits (prev, cur) tuples
```
Rule: need **per-item intermediate state** → `scan_map`; need **one final value** →
`reduce_initial`/`collect`; Acc is non-Clone → must use `scan_map`/`reduce_initial`.

### 5.3 Stage linking (higher-order — how the compile pipeline connects)
```text
flat_map(F)      F: for<'a> FnMut(Item<'a>) -> Inner;  Inner: Context<Inner: ObservableType<Err = Self::Err>>
                 each upstream next → F produces a new stream, subscribed, its next flows downstream
concat_map(F)    like flat_map but order-guaranteed (inner concurrency = 1)
switch_map(F)    F: for<'a> FnMut(Item<'a>) -> Out; Out: Context<Inner: ObservableType<Err=Self::Err> + 'static>; cancels old inner
merge_all(n)     flatten a stream-of-streams (upstream emits Observables)
```
`flat_map`'s inner `Err` must equal the outer `Err` — **both branches (normal stage /
error short-circuit) must be same Item and same Err.** This is exactly why stage
output is unified as a `Result<T, E>` data Item with `Err = Infallible` (§6, §8).
**`and_then` was DELETED in rc.5** — the equivalent is `flat_map`/`concat_map`
(identical semantics for synchronous sources).

### 5.4 Truncation / lifecycle
```text
take(n)/take_last(n)/skip(n)/skip_while(F)/skip_until(N)/skip_last(n)
take_while(F)             F: FnMut(&Item)->bool;   take_while_inclusive keeps the triggering item
take_until(N)             N: Observable<Err = Self::Err>   first next/error on N truncates
default_if_empty(v)       fallback when stream empty
finalize(F)               F: FnOnce()   runs on ANY terminal path (complete/error/cancel) ★ good for span close
on_error(F)               F: FnOnce(Self::Err)   ★ error side-channel, stream unchanged
on_complete(F)            F: FnOnce()
retry(P)                  P: RetryPolicy<Err>: Clone { fn should_retry(&Err, attempt)->Option<Duration> }
```

### 5.5 Combine
```text
zip(B)                 B: Observable<Err=Self::Err>;  emits (A,B) pairs
with_latest_from(S2)   S2: Observable<Err=Self::Err>;  A is the clock, takes B's latest
combine_latest(S2, F)  F: for<'a> FnMut(A::Item<'a>, B::Item<'a>) -> Out
merge(S2)              two streams; S2's Item<'a> must match Self's exactly
```

### 5.6 Aggregate / buffer / distinct
```text
buffer_count(n)        emit Vec every n items
average(Item: Averageable), count, max, min    numeric aggregates
distinct / distinct_until_changed
group_by(...)          grouped multicast
contains(v) / start_with(Vec)
```

## 6. Terminals — three shapes

```text
(a) subscribe(closure)      next only. Fine when the value channel already encodes Result errors.
(b) subscribe_with(Observer)implement Observer (next=&mut self; error=self; complete=self; is_closed),
                            or wrap a closure in FnMutObserver. Use when you must see error/complete.
(c) into_future()           needs async runtime; Output =
                            IntoFutureResult<T,E> = Result<Result<T,E>, IntoFutureError>
                            (outer Err = Empty / MultipleValues pipeline error).
```

**Standard terminal (synchronous + error-observable): mpsc sink.**

```rust
use std::sync::mpsc;
let (tx, rx) = mpsc::channel::<Result<Out, SassError>>();
stream
    .map(final_extract)                          // Item<'static> = Result<Out, SassError>
    .subscribe(move |r| { tx.send(r).ok(); });   // inline sync; value ready on return
rx.recv().unwrap_or_else(|_| Err(missing_output_error))
```

Error protocol: **compiler errors never enter the observable `Err` channel** (which is
`Infallible`). They flow as in-stream `Result` data to the terminal, sidestepping the
`subscribe(closure)` error-swallowing problem (§1). To *additionally* observe errors on
the side, use `.on_error(move |e| …)`.

**Shared / async terminal: prefer `into_future().await` over a std `mpsc` sink.** When the
source is `Shared::from_stream(..)` (which schedules a task, so `subscribe` returns an
awaitable handle, not an inline run),收口 with the framework's own `into_future()` driven by
a tokio runtime — do NOT wrap a synchronous stream back with `std::sync::mpsc`+`recv`
(that reintroduces a std layer around the reactive core). `reduce` = `FnMut(Item,Item)->Item`
(NO seed); to fold to a different accumulator type use `reduce_initial(seed, FnMut(Acc,Item)->Acc)`.

```rust
// Shared + from_stream, terminal via rxrust's own into_future (no std channel):
Shared::from_stream(futures::stream::iter(items))
    .reduce_initial(String::new(), |mut acc, chunk| { acc.push_str(&chunk); acc })
    .into_future()
    .await
```

**Ordering gotcha (verified):** `Shared::from_stream` preserves order (one task polls the
stream sequentially), but `flat_map(|item| Shared::from_stream(..))` spawns a *separate task
per item* → emission interleaves and document order is lost. To fan out per-item work and
keep order, thread per-step state (env / an `@if` chain flag) as an owned `scan_map`
accumulator over ONE ordered source, then `flat_map(Shared::from_iter(..))` (`from_iter`
runs inline & in order) to flatten. Cross-sibling state is a *moved accumulator value*, not
per-item-independent.

`subscribe_with` custom sink (when errors DO ride the Err channel, e.g. throw_err):
```rust
use rxrust::observer::Observer;
struct Sink<T, E>(mpsc::Sender<Result<T, E>>);
impl<T, E> Observer<T, E> for Sink<T, E> {
    fn next(&mut self, v: T) { self.0.send(Ok(v)).ok(); }
    fn error(self, e: E)     { self.0.send(Err(e)).ok(); }
    fn complete(self) {}
    fn is_closed(&self) -> bool { false }
}
```

## 7. Type erasure: the `box_it` family (observable/boxed.rs)

The only legal way to pass "a stream" across a function boundary:

```text
.box_it()          → LocalBoxedObservable<'a, Item, Err = ()>
                       = Local<Box<dyn DynCoreObservable<
                           LocalCtx<BoxedObserver<'a, Item, Err>, LocalScheduler>> + 'a>>
.box_it_clone()    → LocalBoxedObservableClone<…>  (re-subscribable erased stream)
.box_it_mut_ref()  → Item = &'a mut T dispatch
Shared side:       SharedBoxedObservable<…>  (requires Send)
```

Aliases need `feature="scheduler"` (default on) and are re-exported in the prelude.
`LocalBoxedObservable` carries its `Local` context, so you can continue the operator
chain on it, use it as `flat_map` inner, or end with `subscribe`. Choose `'static`
when Item/Err are both owned; write `Err = Infallible` explicitly (the `()` default
won't match a `create` source typed with `Infallible`).

Anti-pattern: `fn stage<O: Observable<Item = T, Err = E>>(…)` → E0107 (GAT). box_it is
the escape.

## 8. Compose directly with operators — NO framework layer

Do NOT introduce `type Stage<T>` / `fn fail<T>` / `fn load_stage` / `fn drain(...)`. That is a
self-made framework layer (`rust.md`: 禁自造框架层 `Stage<T>`/`fail`/`drain`/`drain_one`) which
bends rxrust into running *your* control-flow abstraction instead of using its operators.
Compose ONE cold operator chain straight on the source, with a single terminal `subscribe`.
Per-item stage *functions* that take an owned value and return an observable are fine, and
`box_it()` at a fn boundary is fine — the ban is the `Stage<T>`/`fail`/`drain` framework *type*
and any hand-built state machine (`from_fn` + `VecDeque`, or a shared mutable `Acc` that *drives*
traversal).

```rust
use rxrust::prelude::*;
use std::sync::mpsc;

// One cold chain over owned values; no Stage/fail/drain layer, no shared mutable driver.
// Loops (`@each`) are `flat_map`; branches (`@if`) are `filter`; ONE terminal subscribe.
fn compile(source: String) -> String {
    let (tx, rx) = mpsc::channel();
    lexer::token_stream(source)                     // chars -> owned Tok, moved downstream
        .collect::<Vec<_>>()                         // owned batch (data), not an imperative replay
        .flat_map(|toks| parse_eval_render(toks))    // operators over the owned value
        .subscribe(move |css| { tx.send(css).ok(); });
    rx.recv().unwrap()
}

// a loop is literally `flat_map`, written inline — no wrapper, no buffer-then-replay:
//   Shared::from_iter(items).flat_map(|item| body_stream(item)).box_it()
```

Time each stage with `tracing::span!`, recording elapsed inside the closure that runs (§11).

## 9. Subject multicast (subject.rs)

```rust
let subject = Local::subject::<i32, Infallible>();
subject.clone().next(1);                       // Subject itself implements Emitter
subject.clone().subscribe(|v| /* … */);        // each subscriber shares inner Rc
subject.complete();
```

- `Subject` is the **framework's internal multicast plumbing**, not a business-state
  container. Use it whenever one producer's output must feed multiple consumers at once —
  e.g. a module compiled once and fanned out to every `@use`, or a `@mixin` definition
  consumed by many `@include`s. In rc.6 the subject payload is a
  `SubjectPtr<…>` over the `rc.rs` shared handles: `Local::subject::<Item,Err>()` uses
  `MutRc` (`Rc<RefCell<…>>`), the `Shared` form `MutArc` (`Arc<Mutex<…>>`); `Item: Clone`
  is required by `behavior_subject`. This is the framework's own sharing — it is NOT
  permission to wrap business data (tokens/AST/accumulators) in `Rc<RefCell>`/`Mutex`;
  keep those as owned values moved through `scan_map`/`reduce_initial` (§11).
- `behavior_subject(initial)` caches latest (Item: Clone).
- `Connectable` / `ref_count` / `fork` (connectable.rs) exist for deferred multicast
  wiring; read them only when needed.

## 10. RxJS / rxrust 0.15 differences (guard against old habits)

| Old habit | rc.6 reality |
|---|---|
| `pipe(op1, op2)` | no pipe; chain methods directly |
| `and_then` operator | **DELETED.** Equivalent: `flat_map` / `concat_map` (same for sync sources) |
| 0.15 `Arc<Mutex<Option<T>>>` + `try_unwrap` drain | mpsc channel or `into_future` |
| 0.15 `Observable<Item = T>` assoc type | GAT `Item<'a>`; generic-bounding syntax totally different |
| `share()` / `publish()` | `Subject` / `ref_count()` / `Connectable` |
| 0.15 three-closure `Observer` constructor | `Observer` trait (next=&mut, error/complete=self); convenience via `subscribe_with(FnMutObserver(..))` or self-impl |
| free `zip(a, b)` | instance `a.zip(b)` / factory `merge_observables(Vec)` / `concat_observables(Vec)` |
| JS `switchMap` cancel semantics | same-name `switch_map` ✓ exists |

## 11. Ownership red lines (project rules — violation = task failure)

1. **No** `Arc<Mutex<_>>` for business data (token streams, AST, source, accumulators).
   Move by value: `scan_map(&mut Acc, item)` / `reduce_initial(owned_acc, …)` /
   `collect` then move the whole bundle.
2. **No** `println!`/`eprintln!`; across stages use `tracing::span!`
   (stage/module/elapsed_ms), recording timing **inside the executing closure** (§8),
   or `finalize` to close.
3. Closures capture owned values + `move`; one-shot consumption uses
   `Option<T>::take()` to satisfy `FnMut`.
4. No inline tests in `src/`; all tests in `tests/*.rs`; one file ≤ 500 lines.
5. Error protocol: compiler errors (SassError) **always flow as in-stream `Result`
   data**; the `Err` channel stays `Infallible`; use `on_error` only for side listening.

## 12. Common compile errors → remediation

| Symptom | Root cause | Fix |
|---|---|---|
| E0107 on `Observable<Item = T>` | Item is a GAT | assemble inline in one fn, or return `LocalBoxedObservable` alias |
| `expected String, found &str` in `tx.send` | channel type shadowed by map closure | annotate `mpsc::channel::<Result<T, E>>()` |
| flat_map branches type-mismatch | inner streams must share Item/Err | `.box_it()` both branches to the same erased stream type |
| `move out of captured value` in a map closure | map is `FnMut`, called many times | `Option::take()` to pull the owned value once |
| `Emitter` not found in a create closure | not in prelude | `use rxrust::observer::Emitter;` |
| no value at `rx.recv()` after subscribe | upstream used a timed operator / async source | drop timed ops or enter a runtime; sync sources never miss |
| `into_shared` Send not satisfied | Shared needs Send+Sync+'static across the chain | use `Local` unless you truly need cross-thread execution |
| want to mutate inside `filter` | filter receives `&Item` | use `map`/`filter_map` for value semantics |
| errors swallowed by `subscribe` | `FnMutObserver` drops error/complete | in-stream `Result` protocol / `subscribe_with` / `on_error` |

## 13. Shared-state primitives (`rc.rs`, new in rc.6)

rxrust abstracts its own shared-mutable bookkeeping behind three traits (all built on
`rclite::{Rc, Arc}`, not `std::rc`/`std::sync`):

```text
RcDeref: Clone      { type Target; type Ref<'a>: Deref; fn rc_deref(&self) -> Ref<'_>; }
RcDerefMut: RcDeref { type MutRef<'a>: DerefMut; fn rc_deref_mut(&self);
                      fn try_rc_deref_mut(&self) -> Option<MutRef<'_>>; }  // None on re-entrancy
SharedCell<T: Copy+Eq>: Clone+From<T> { get; set; compare_exchange; }

MutRc<T>   = Rc<RefCell<T>>    Local shared-mutable state
MutArc<T>  = Arc<Mutex<T>>     Shared (Send) shared-mutable state
CellRc<T>/CellArc<T> = Rc<Cell<T>>/Arc<AtomicCell<T>>   Copy flags/counters (cheaper than RefCell)
```

`try_rc_deref_mut` returning `None` is how the framework detects synchronous re-entrancy
and defers via the scheduler instead of panicking/deadlocking. **These are framework
internals** (subject fan-out, `Connectable`, internal counters). The compiler's business data
must stay move-only: if you reach for `MutRc`/`MutArc`/`Subject` to hold a token vector or an
accumulator, that is the GC/shared-state smell R2/§11 forbid — carry it as the `scan_map`
/`reduce_initial` owned accumulator instead.

## Subscription & RAII

`Subscription::unsubscribe(self)` consumes; `()` implements `Subscription` (always
closed) — that's why `create` can return `()`. `unsubscribe_when_dropped()` returns a
`#[must_use] SubscriptionGuard` that unsubscribes on scope exit — not binding the guard
means immediate unsubscribe.
