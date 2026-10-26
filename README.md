# qrass — workspace hosting **sasspile**

A clean-room **SCSS → CSS** compiler written as a reactive *push* pipeline: each compile phase is a
real rxrust **operator kernel** (`struct Phase { source }` + `PhaseObserver { observer, state }` +
`ObservableType`/`CoreObservable<C: Context>`) that moves **owned** tokens, events, rules and values
forward and emits its own `0..N` items — `from_iter → Lexer → Structurize → Exec → Render →
reduce_initial → into_future`. There is no materialized AST walked by recursion, and no
`Arc<Mutex<_>>` business state anywhere in the path.

> History: this repository once contained `crates/qrass` (a pull-shaped compiler). It was
> **deleted** when the project adopted the push doctrine; the crate directory no longer
> exists and its code must not be used as a template. The only implementation is
> `crates/sasspile`.

## Workspace

| Crate | Role | Size (measured 2026-10-26) |
| --- | --- | --- |
| `crates/sasspile` | the compiler — **library only, no CLI**; one public exit | 8 `src/*.rs`, 1,815 lines, 12 test targets |
| `crates/ledger` | the A/B record: named runs of hrx snapshots in one SQLite table set, `ledger diff` → `regress` | 396 lines |
| `crates/sasslint` | deterministic gate for this project's hard rules (+ `sasslint-mcp` bin) | 596 lines, 10 tests |

Source layout of the compiler: `lib.rs` (the single chain), `phases.rs` (the four kernels),
`lexer.rs`, `exec.rs`, `expr.rs`, `value.rs`, `selector.rs`, `env.rs`. Two module directories,
`src/builtins/` and `src/directives/`, are still empty placeholders for P1.7/P3+.

`crates/docsizer` — the counting tool of a cancelled tutorial programme — has been **removed from the
workspace** (its dangling member entry was breaking every `cargo` invocation). Do not reinvent it.

## Status (all numbers re-measured on 2026-10-26)

| Gate | Command | Result |
| --- | --- | --- |
| Tests | `cargo test --all-targets` | **66 passed / 0 failed / 0 ignored** |
| Rule gate | `cargo run -p sasslint --bin sasslint -- lint crates/sasspile/src` | **`count=0`** |
| Rule gate as CI | `cargo test -p sasslint` (`gate_sasspile_is_clean`) | **passes**; no longer `#[ignore]`d |
| sass-spec `.hrx` snapshot | `cargo test -p sasspile --test hrx` | `total=11,533 pass=643` (**5.6 %**), of which **`pass_substantive=478`** and `pass_empty_expectation=165` |
| Clippy | `cargo clippy --all-targets -- -D warnings` | **clean (exit 0)** |
| Plan | `openspec validate redesign-sasspile-rxrust-shared-stream --strict` | valid |

**P0′ (the framework-native rewrite) is closed**: phases are kernels (0′.1/.2), order comes from the
stream (0′.4), the `Err` channel is real with `UndefinedVariable` fatal (0′.3), the four iron rules are
machine-gated (0′.6) and selectors/property names are structured values (0′.7). Only 0′.5 (shared state
as `C::RcMut`/`C::RcCell`) is open, and it has nothing to change until P4/P5 introduce shared state.
The next front is **P1 (value foundation) + P7 (diagnostic messages)**.

The 5.5 % pass rate is **not** a regression signal: the compiler is deliberately incomplete while the
architecture is rebuilt phase by phase (P0–P8). What a round must never do is lose previously passing
cases — that is decided by `ledger diff <base> <run>` requiring **`regress = 0`**, not by the raw rate.
`tests/hrx.rs` asserts nothing case-by-case for the same reason.

**Read `pass_substantive`, not `pass`.** 165 of the 643 passes are against fixtures whose expected CSS is
empty — legitimately correct (Sass really emits nothing there), but also what a compiler that produces
nothing scores, so they prove nothing about a feature. The audit that found this: `directives/{if,for,while}`
showed 39 “passing” cases, **all 39** with empty expectations, i.e. `@if`/`@each`/`@for` were
implemented nowhere despite `tasks.md` claiming otherwise (now corrected, and `@for` over literal integer
ranges has since been built and moves 8 real cases). The harness reports both numbers on every run.

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

One cold chain of six rings, written in `compile` and nowhere else — no stage function takes or returns
an observable type, no `box_it()` seam, no `Stage`/`fail`/`drain` orchestration layer:

```
chars ─▶ Lexer ─▶ Structurize ─▶ Exec ─▶ [global passes] ─▶ Render ─▶ reduce_initial ─▶ into_future()
             (each ▶ is a kernel: struct{source} + Observer holding this phase's state,
              emitting 0..N items with observer.next(..); Err stays generic)
```

The chain it replaced (`scan_map → Vec<Tok> → flat_map(from_iter)`, then `scan(State)` +
`filter_map` + an `order` key + `collect`/`sort_by_key`) is **deleted**, and re-introducing any part of
it fails `sasslint`. Nesting is not a tree: `Exec` carries an owned `scopes` vector and settles a rule
at two moments (a nested block opening flushes the parent's pending declarations; a block closing
flushes the rest), which is why the output needs no sort and one block can emit several rules.

Cross-cutting passes that genuinely need the whole rule set (`@extend` fixpoint, media bubbling) are
expressed as `collect` + a **pure forward fold**, then pushed back into the stream via `flat_map` —
never a mutable object graph visited by a walker.

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
7. **A name position is not a variable reference.** Three shapes were measured, all the same mistake of
   looking a `$name` up where Sass means a *label*: a block head that binds (`@for $i from …`), an
   argument label (`color.hwb($hue: 180, …)` — 1,611 cases), and a binding's declaration flag
   (`$a: b !default`). A head resolves only `#{ … }`; a run that passes named arguments is not evaluated
   at all; `!default`/`!global` are peeled from a binding value only.
8. **A classifier can never be finer than the lexer's lexemes.** Deciding "is this a vendor-prefixed CSS
   call?" or "is this `&` the parent reference?" failed while `-a-calc(` was three lexemes and `&` was
   glued into a word — the fixes were lexical (`-` deferred one character, `&` is a `STRUCTURAL` lexeme,
   a quoted string tracks `$`/`\`). Two more lexeme-level truths: a `//` comment's newline is `Space`,
   not `Word(" ")`; a `&` inside a `#{&}` is text substitution, not the parent position.
9. **The oracle must judge the thing the spec states.** An `error` case passes only when the compiler's
   message equals the fixture's `Error:` header — counting "any error" reported a fake **+385** where
   the true gain was **+1**. Likewise: case keys must be unique (`<basename>.hrx::<case>` collided 1,135
   times, 33 with both a PASS and a FAIL), and the harness's scanned directory list must equal the dirs
   the plan cites — `operators`/`parser`/`variables` had 79 cases nobody was measuring.

**Verification is not self-evident either.** `ledger` stores only failures, so re-importing a run name
must clear that run's old `run_fail` rows (it does now, with tests) — otherwise a fixed case reports a
phantom REGRESS. Before trusting `regress=0`, redo the A/B on an empty `--db`, and for behaviour-neutral
refactors prove it by a byte-identical snapshot (md5/`diff`), not by a matching pass count. Two more
traps this repo actually fell into: (a) a statistic that reads **0** is a bug until a known-positive case
proves the measuring code works (an off-by-one `substr` swallowed a quote and reported "0 rows differ
only by whitespace/case", nearly producing a false conclusion); (b) **a FAIL that becomes another FAIL is
invisible in the pass count**, so also count invalid errors (e.g. `grep` the snapshot for
`unexpected error`) when touching the error path. The A/B record is a **named run in `reports/hrx.db`**
(import the harness output every increment) — never loose snapshot files, which is the sprawl `ledger`
exists to remove.

## Where the rules and knowledge live

| Document | Authority |
| --- | --- |
| `.qoder/rules/rust.md` | hard project rules (always-on): new-session pre-flight, rxrust as the engine, tracing spans, ≤500-line files, tests in `tests/`, no `println!`, no Python/scripts, SSH push, and the measurement red lines |
| `docs/rxrust-source-analysis/` (01–09 + README) | **authoritative** rxrust rc.6 source reading; `09-…-ordering.md` holds the cancellation / terminal-ordering invariants and the list of places where the library's own doc comments contradict its implementation |
| `.qoder/skills/rxrust-reactive/` | the do/don't doctrine applied when writing pipelines (`SKILL.md` + `reference.md`) |
| `openspec/changes/redesign-sasspile-rxrust-shared-stream/` | **the active plan**: architecture (`design.md`), phase tasks (`tasks.md`, incl. every measured number behind a claim) and the requirement deltas being implemented (P0′, P0–P8) |
| `docs/rxrust-ai-manual.md` | **historical** (written against rc.5); superseded by the two above — verify against source before trusting a detail |
| `docs/sasspile-framework-design-report.md` | a dated snapshot of the design report, with its "现状校正" section carrying the corrections |
| `openspec/changes/add-rxrust-line-by-line-tutorial/`, `docs/rxrust-from-scratch/` | **CANCELLED** tutorial programme, kept only for history: do not apply, continue or cite it as a source of behaviour |

`reports/hrx.db` (gitignored, local) is the A/B ledger; `ledger runs` lists the recorded runs and
`ledger diff A B` re-computes any claim made in the documents above.

## Ground rules

- Rust edition 2024 (toolchain 1.99); rxrust pinned to **`=1.0.0-rc.6`** (a bare
  `cargo add rxrust` still resolves the incompatible 0.15 line).
- Ownership over shared mutability: state travels as a value carried by the stream.
- `Shared`/threads only where work is genuinely independent; the fan-out ordering caveat
  (a queued inner observable **loses** emissions — `sort` cannot recover them) is documented
  in `docs/rxrust-source-analysis/09-…`.
- No `println!` / `eprintln!` — `tracing` macros, with spans around cross-function stages.
- No inline tests: everything under `tests/`. ≤ 500 lines per source or test file.
- Code queries go through the **`codegraph_explore` MCP tool first** (see `.qoder/rules/codegraph.md`);
  the `codegraph` CLI (`explore`/`node`/`callers`/`impact`) is the same-semantics fallback when no MCP is
  available. Correctness is still decided by the compiler and tests.
- Push over SSH: `git push github main`.

## Known gaps (recorded, not hidden)

- Language coverage is early-phase: value/type semantics (`*`, `/` and the slash-vs-list ambiguity,
  `Calculation`, units, colours), selectors/`@extend`, module `@use`/`@forward`/`@import`, callables and
  CSS at-rules are each specified in the OpenSpec change but only partially implemented.
- Most of the remaining `core_functions/color` failures need **real messages and real function
  behaviour** (P1.5/P1.7 + P7 diagnostics), not error plumbing: the corpus was bucketed case by case to
  establish that, and the numbers are in `tasks.md`.
- No CLI binary yet — the crate is library-only by design (`compile` is the sole exit).
- The harness runs `.scss` only (no `--indented`), skips multi-file and `options.yml` cases, and compares
  only the `Error:` header (no span/`Warning:`/deprecation) — tasks 0.2/0.3/0.4.

## License

MIT — see [LICENSE](LICENSE). The vendored submodules (`sass-spec`, `element-plus`) keep
their own licenses.
