# qrass — workspace hosting **sasspile**

A clean-room **SCSS → CSS** compiler written as a reactive *push* pipeline: every stage is an
rxrust `Observable` that moves **owned** tokens, expressions and values forward through
operators (`from_iter → scan/scan_map → flat_map → filter → collect → reduce_initial →
into_future`). There is no materialized AST walked by recursion, and no `Arc<Mutex<_>>`
business state anywhere in the path.

> History: this repository once contained `crates/qrass` (a pull-shaped compiler). It was
> **deleted** when the project adopted the push doctrine; the crate directory no longer
> exists and its code must not be used as a template. The only implementation is
> `crates/sasspile`.

## Workspace

| Crate | Role | Size (measured 2026-10-25) |
| --- | --- | --- |
| `crates/sasspile` | the compiler — **library only, no CLI**; one public exit | 6 `src/*.rs`, 891 lines, 6 test targets |
| `crates/ledger` | A/B harness: diffs two hrx snapshots, reports `regress` | 310 lines |
| `crates/sasslint` | deterministic rule gate for the project's hard rules (+ `sasslint-mcp` bin) | 226 lines, 5 tests |
| `crates/docsizer` | doc-counting tooling, dormant (its tutorial programme was cancelled) | 942 lines, 10 tests |

Source layout of the compiler: `lib.rs` (the single chain), `lexer.rs`, `exec.rs`, `expr.rs`,
`value.rs`, `env.rs`. The directories `src/builtins/` and `src/directives/` are currently
empty placeholders for later phases.

## Status (all numbers re-measured on 2026-10-25)

| Gate | Command | Result |
| --- | --- | --- |
| Tests | `cargo test --workspace` | **39 passed / 0 failed** (1 ignored) |
| Rule gate | `cargo run -p sasslint --bin sasslint -- crates/sasspile/src` | **`count=0`** |
| sass-spec `.hrx` snapshot | `cargo test -p sasspile --test hrx` | `total=11486 pass=595 skipped=1232` (**5.2 %**) |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **clean (exit 0)**, verified the same day |

The 5.2 % pass rate is **not** a regression signal: the compiler is deliberately incomplete
while the architecture is rebuilt phase by phase (P0–P8). What a round must never do is lose
previously passing cases — that is decided by `ledger diff <base> <run>` requiring **`regress = 0`**,
not by the raw rate. `tests/hrx.rs` asserts nothing case-by-case for the same reason.

## Conformance authority

- **Only** the `.hrx` goldens under the `sass-spec` submodule define behaviour
  (`input.scss` → `output.css` / error block).
- `element-plus` is used as a **non-regression self-baseline** — never as a source of
  expected output produced by another implementation.
- dart-sass source or documentation must **not** be consulted to infer behaviour; it is a
  reference for *what the language is*, not for *what our output should be*.

```bash
git submodule update --init element-plus sass-spec
```

## Library

```rust
use sasspile::compile;

async fn build(scss: String) -> Result<String, sasspile::SassError> {
    compile(scss).await          // the sole public entry; takes content, not a path
}
```

Diagnostics go through `tracing` (never `println!`), so the returned `String` is pure CSS:

```bash
RUST_LOG=info cargo test -p sasspile --test hrx -- --nocapture   # `trace` adds per-stage spans
```

## Pipeline

One cold chain, written in `compile` and nowhere else — no stage function takes or returns an
observable type, no `box_it()` seams, no `Stage`/`fail`/`drain` orchestration layer:

```
chars ─▶ scan_map(LexAcc) ─▶ scan_map(MsgAcc) ─▶ scan(State) ─▶ filter_map(into_rule)
       ─▶ flat_map(per-rule work, ordered by an owned `order` key)
       ─▶ collect ─▶ pure fold ─▶ reduce_initial ─▶ into_future().await
```

Cross-cutting passes that genuinely need the whole rule set (`@extend` fixpoint, media
bubbling) are expressed as `collect` + a **pure forward fold**, then pushed back into the
stream via `flat_map` — never a mutable object graph visited by a walker.

## Anti-GC red lines (read before writing any pipeline code)

Every line below is a shape this repository actually shipped, had to rework, and now blocks by rule +
`sasslint` gate. They are here (not only in the long docs) because a fresh session must not
rediscover them:

1. **Each phase is a real operator kernel**, not a batch shell: `struct Phase { source }` +
   `PhaseObserver { observer, state }` + `impl ObservableType` + `impl CoreObservable<C: Context>`,
   emitting `0..N` items one by one via `observer.next(..)`. Banning: `scan_map` returning a `Vec`
   followed by `flat_map(from_iter)` — that hides a one-to-many behind a container.
2. **Order comes from the stream, not from sorting.** An `order`/sequence key plus `sort_by_key` is a
   symptom that the *emission moment* was modelled wrong (Sass settles a block's pending declaration
   run when a nested block opens, and the rest when it closes — so one block can emit several rules).
   Sorting also cannot recover elements: `merge_all` **drops** emissions of an inner that is still
   queued. Use `concat_map`/`concat_all` or a serial chain.
3. **Errors ride the real `Err = SassError` channel** (`Observer::error` → `into_future()` →
   `Ok(Err(..))`). `.ok()` / `unwrap_or_else(source_text)` as a fallback is not an error policy: it
   silently turns a real error into wrong output and masks the spec's `errors/**` expectations. Only
   constructs the spec defines as verbatim (unreducible `calc`, custom properties, unknown at-rules)
   stay as-is.
4. **No fake concurrency**: wrapping microseconds of pure CPU work in `from_stream(stream::once(..))`
   is not parallelism (measured: same worker thread). Real fan-out belongs to independent module
   subgraphs (P5) only.
5. **Close only through the framework's terminals**: `collect`/`reduce_initial` + `into_future()`,
   `into_stream()` (cancels on `Drop`), or awaiting the `TaskHandle` a scheduled source returns.
   `std::sync::mpsc` is a bridge for a caller that needs a `Sender`, never the terminal.
6. **Multicast is not re-entrant**: `next`/`error`/`complete` on a `Subject` from inside its own
   subscriber callback panics; insert `delay(Duration::ZERO)` for feedback loops. And `Subject::
   is_closed()` means "no subscribers", not "terminated".

**Verification is not self-evident either.** `ledger` stores only failures, so re-importing a run name
must clear that run's old `run_fail` rows (it does now, with tests) — otherwise a fixed case reports a
phantom REGRESS. Before trusting `regress=0`, redo the A/B on an empty `--db`, and for behaviour-neutral
refactors prove it by a byte-identical snapshot (md5/`diff`), not by a matching pass count.

## Where the rules and knowledge live

| Document | Authority |
| --- | --- |
| `.qoder/rules/rust.md` | hard project rules (always-on): rxrust as the engine, tracing spans, ≤500-line files, tests in `tests/`, no `println!`, no Python/scripts, SSH push |
| `docs/rxrust-source-analysis/` (01–09 + README) | **authoritative** rxrust rc.6 source reading; `09-…-ordering.md` holds the cancellation / terminal-ordering invariants and the list of places where the library's own doc comments contradict its implementation |
| `.qoder/skills/rxrust-reactive/` | the do/don't doctrine applied when writing pipelines (`SKILL.md` + `reference.md`) |
| `docs/rxrust-ai-manual.md` | **historical** (written against rc.5); superseded by the two above — verify against source before trusting a detail |
| `openspec/changes/redesign-sasspile-rxrust-shared-stream/` | the architecture plan and requirement deltas being implemented (P0–P8) |
| `reports/` | assembly snapshots |

## Ground rules

- Rust edition 2024 (toolchain 1.99); rxrust pinned to **`=1.0.0-rc.6`** (a bare
  `cargo add rxrust` still resolves the incompatible 0.15 line).
- Ownership over shared mutability: state travels as a value carried by the stream.
- `Shared`/threads only where work is genuinely independent; the fan-out ordering caveat
  (a queued inner observable **loses** emissions — `sort` cannot recover them) is documented
  in `docs/rxrust-source-analysis/09-…`.
- No `println!` / `eprintln!` — `tracing` macros, with spans around cross-function stages.
- No inline tests: everything under `tests/`. ≤ 500 lines per source or test file.
- CodeGraph CLI (`callers`, `impact`, `explore`, `node`) for code queries.
- Push over SSH: `git push github main`.

## Known gaps (recorded, not hidden)

- Language coverage is early-phase: value/type semantics, selectors/`@extend`, module
  `@use`/`@forward`/`@import`, callables and CSS at-rules are each specified in the OpenSpec
  change but only partially implemented.
- No CLI binary yet — the crate is library-only by design (`compile` is the sole exit).
- `crates/docsizer` and `docs/rxrust-from-scratch/` are leftovers of a cancelled tutorial
  programme; they remain tracked pending an explicit decision and are not part of the
  compiler's gate set.

## License

MIT — see [LICENSE](LICENSE). The vendored submodules (`sass-spec`, `element-plus`) keep
their own licenses.
