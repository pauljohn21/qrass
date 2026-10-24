# rxrust conversion backlog — NO exemptions granted

OpenSpec `refactor-rfp-strict` Requirement R4 (revised to **zero-exemption** by the
user's ruling): the rxrust Observable stream is the *first-principles substrate* of
all control flow, not a preferred option. The only forms that are non-imperative by
definition — (a) a single-expression `match` over an enum/`Result`/`Option` for value
selection, and (b) a non-iterative `Option`/`Result` combinator chain on a *single*
value — need no entry anywhere. Everything else (`for`/`while`/`loop`, std-iterator
chains standing in for a dataflow, `if`/`else if` chains, guard-`return` stacks) is a
**conversion item**, never a waiver. `style_gate.rs` ratchets each file's counts down
toward zero; nothing is exempt.

If a construct provably cannot be expressed as an equivalent Observable chain, that is
a **blocking finding to escalate** (with a minimal repro and an A/B regression note) —
not something to register here and wave through, and never something to win by
sacrificing behaviour (R1) or by retreating to a std chain.

## Open conversion backlog (imperative constructs still present, to reach 0)

Priority: pure-function folds first, then the char/token cursor state machines. Every
step gated by a full hrx A/B `regress=0`.

| # | File · construct | target rxrust form |
|---|---|---|
| C1 | `main.rs` `parse_args` `while let … args.next()` (CLI lookahead) | fold the owned `Vec<String>` with `Local::from_iter … .scan_map` over a `(pending, Cli)` accumulator, `reduce_initial` to the result |
| C2 | `builtins/selector.rs` `simple_selectors` 6× cursor `while` over `&[char]` | `Local::from_iter(chars) .scan_map(CursorState, …)` stream-migrator (the `lexer.rs` `scan_map` + `LexPunct` precedent) |
| C3 | `lexer_detail.rs` `lex_string` / `lex_interp_raw` `loop`/`while` | char-cursor `scan_map` migrator with owned quote/escape/interp state; recursion re-expressed as nested `flat_map`/owned buffers |
| C4 | `parser_if.rs` `modern_if` / `clause_value` / `if_cond` token-cursor `loop` | `Local::from_iter(tokens) .scan_map(Parser-state, …)` + `flat_map` over the recursive-descent stages |
| ✅ C5 (done) | `builtins/color_transform.rs` channel-edit `for` → **done**: folded to `Local::from_iter(keys).reduce_initial((c4, latch-err), apply_channel_edit).map(Ok).box_it()` drained via `drain_one`; `named`/`base_css` moved into the `'static` closure, `apply_channel_edit` is a pure step fn, the Scale unit ladder is a tuple `match` (no else-if). loops 1→0, ledger A/B `regress=0` | — |
| C6 | `&mut self`-mutating eval walks currently on std `try_fold`/`for_each` (loader resolution, eval_* statement walks, `eval_color_space_symbolic` slot fold …) | thread the mutated state as the stream's **owned accumulator** (`scan_map`/`reduce_initial`) instead of `&mut self` — `'static` closures then hold |

C6 is tracked continuously: `style_gate.rs` counts `for`/`while`/`loop` per file, so any
new raw loop regresses the ratchet and fails the gate; std chains are a contract
violation per R2 even where the loop count is already 0.

## Reference

Gate tool: `tests/style_gate.rs` (`style_gate_counts_only_decrease` +
`line_budget_only_shrinks`), baseline `tests/golden/style-baseline.txt`
(file\tloops\telseif\tletmut\treturns), line budget `tests/golden/line-budget.txt`.
Re-bless with `QRASS_STYLE_GATE_BLESS=1 cargo test -p qrass --test style_gate`.

Baseline snapshot (task 1.3): ledger run `m5k` (reports/hrx.db). Every batch re-verifies
0 regression against a fresh snapshot on its parent commit (gate R1/D6).
