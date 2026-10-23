# RFP style-gate exemptions

OpenSpec `refactor-rfp-strict` Requirement.4: the ONLY sanctioned imperative
constructs are (a) single-expression `match` over enums/results (value
selection, not control flow) and (b) non-iterative `Option`/`Result` combinator
chains — these need no entry here. Anything beyond them (typically S4 parser
cursor scans that measurably cannot express as an Observable chain) must be
registered below with all four fields, in the sealed-pure-function form
(no side effects, `mut` confined to the function body).

| File | Construct | Reason | Re-review condition |
|---|---|---|---|
| builtins/selector.rs `simple_selectors` (6× cursor `while`) | sigil-driven char cursor scanning a `&[char]` buffer with string/interp/bracket escape state | parser-cursor family (design D3-S4): an Observable form would need owned per-char state and buys no clarity; behavior pinned by selector-parse unit tests + hrx | frozen at S4 (task 5.4) as a sealed pure-function exemption |
| `&mut self`-borrowed walks (eval_config offer/forward folds, loader resolution loop, diag marker build, eval_* statement walks, `eval_color_space_symbolic` slot fold …) + `builtins/color_transform.rs` channel-edit loop (local `c4` mutation, 6 golden-pinned early-exit classes) | std `try_fold`/`for_each` closure chains | rxrust closures require `'static`, so a walk mutating `self` cannot enter an Observable stream; the chain-closure form (no loops, no if/else chains) is the contract-legitimate carrier — single-branch `if` guards allowed, `else if` chains not. The edit loop's former `else if` chain now routes through the pure `classify_non_number` three-class enum + one `match` | revisit if rxrust gains borrowed-context folds |
| `lexer_detail.rs` `lex_string` / `lex_interp_raw` (outer `loop` + nested quote-skip `while`) | char-cursor scanner with recursive interpolation and quote/escape state | both scan a `&mut Lexer` buffer and `lex_string` calls `lex_interp_raw` recursively; an Observable form would need owned per-char state plus `ControlFlow` and buys no clarity — cursor state confined to the scanner body, behavior pinned by lexer unit tests + hrx | frozen at S4 (task 5.4) as a char-cursor exemption |
| `parser_if.rs` `modern_if` / `clause_value` / `if_cond` (recursive-descent `loop` token cursors) | `&mut self` token consumption with one-token lookahead (`pk`/`peek_word`/`bump`/`eat`), delimiter/depth break conditions, `?` and multiple `return Err` error exits | same S4 parser-cursor family as `lexer_detail`; a chain form would need owned parser state plus `ControlFlow` across three mutually-recursive methods — no clarity gain; pinned by `expressions/if` hrx goldens (the pure peek-only `looks_like_modern_if` scan was combinatorized to `scan`+`find_map`, and its `and`/`or` op lookahead to a tuple match) | frozen at S4 (task 5.4) |
| `main.rs` `parse_args` `while let … args.next()` | CLI option parser: consumes each option's value token via a second `args.next()` lookahead while accumulating into several `&mut` locals | the option→value lookahead cannot be expressed as a `try_fold` over a self-consuming iterator without a contrived pending-skip state machine; this is the binary entry, unexercised by hrx/e2e/unit so a rewrite is unverifiable | frozen at S4; revisit only if CLI parsing gains a golden harness |

Baseline snapshot (task 1.3): ledger run `m5k` (reports/hrx.db) — 9188/10961 PASS, e2e 4/4,
unit 515/0, clippy -D warnings GREEN, at commit `8501a84` + this change's M0
work. Every S1–S4 batch must re-verify 0 regression against a fresh snapshot
taken on that batch's parent commit (gate R1/D6).
