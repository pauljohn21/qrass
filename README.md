# qrass

A reactive **SCSS → CSS** compiler in Rust. Every pipeline stage is an rxrust
Observable that is built and drained in place, passing *owned* tokens, AST nodes
and values from one stage to the next — ownership-first, no garbage-collector
style `Arc<Mutex<_>>` anywhere in the business path.

## Status

| Gate | Result |
| --- | --- |
| `cargo test -p qrass` | **286 passed / 0 failed** |
| element-plus theme-chalk golden | **4 / 4** |
| sass-spec `.hrx` runner (full modern subset) | **6037 PASS / 10961 attempted = 55.08%**, floor `55.0` |
| `expressions/if/error` annotation blocks, compared verbatim | **63 / 63** |
| Clippy on the files under active development | clean |

Conformance is measured **only** against the `.hrx` goldens in the local
`sass-spec` submodule and the committed element-plus CSS baseline. Those files
are the specification; nothing else is treated as an authority.

## Quick start

```bash
git submodule update --init element-plus sass-spec

cargo run -p qrass -- crates/qrass/tests/fixtures/passthrough.scss
# a {
#   color: red;
# }
# .b > .c {
#   margin: 0 auto;
# }
```

CLI flags: input path, `-o` / `--output <path>`, `--style expanded`,
`--load-path <dir>` (repeatable, consulted for `@use` / `@forward` resolution).
`expanded` is the only output style implemented so far; anything else is
rejected before compiling.

Diagnostics are emitted through `tracing` on **stderr**, so stdout stays pure
CSS:

```bash
RUST_LOG=info cargo run -p qrass -- input.scss   # `trace` adds per-stage spans
```

## Library

```rust
use std::path::Path;
use qrass::error::SassError;
use qrass::{Options, compile};

fn build(entry: &str) -> Result<String, SassError> {
    let opts = Options {
        load_paths: vec!["vendor".into()],
        ..Options::default()
    };
    compile(Path::new(entry), &opts)
}
```

## Pipeline

Five stages, each a self-contained stream drained through an `mpsc` channel and
handing owned data to the next (wrapped in a `tracing` span with `stage`,
`module`, `elapsed_ms`):

```
load → lex → parse → eval → render
```

| Area | Files | Role |
| --- | --- | --- |
| Loading | `loader.rs`, `span.rs` | Source reading, URL canonicalization, position mapping |
| Lexing | `lexer.rs`, `lexer_detail.rs`, `token.rs` | `scan_map` char transducer producing owned tokens; punct dispatch derived by `qrass-macros` |
| Parsing | `parser*.rs`, `ast*.rs` | Owned token runs folded into statement / expression AST |
| Eval | `eval*.rs`, `env.rs`, `module.rs`, `value.rs`, `builtins/`, `color4*.rs`, `named_colors.rs` | Scope arena, module instantiation, builtins, CSS Color 4 math, `@extend`, three-valued `if(…)` execution |
| Output | `css.rs`, `color_css.rs`, `parser_text.rs`, `plain_css.rs` | Expanded CSS serialization; plain-CSS source enforcement |
| Diagnostics | `error.rs`, `diag.rs` | Golden-layout error snippets (marker runs, labels, location line) |

Layout: `crates/qrass` (lib + CLI, 41 source files, ~9.6k lines),
`crates/qrass-macros` (derive macros), `docs/rxrust-ai-manual.md` (the rxrust
1.0 notes this codebase follows), `openspec/changes/add-qrass-scss-compiler/`
(proposal, design, specs, tasks), `reports/` (assemlist snapshots),
`.qoder/rules/rust.md` (hard project rules).

## Convergence workflow

The `.hrx` runner walks the modern spec subset, materializes each case under a
virtual spec root and reports one row per case when asked:

```bash
# narrow to one area and export a machine-readable report
QRASS_HRX_DIR=expressions/if QRASS_HRX_OUT=/tmp/hrx.txt cargo test -p qrass --test hrx_spec
awk -F'\t' '$1=="PASS"{print $2}' /tmp/hrx.txt | sort > /tmp/pass.txt

# a round is only accepted if the previous pass list is a subset of the new one
comm -23 /tmp/pass_before.txt /tmp/pass_after.txt   # must print nothing
```

`crates/qrass/tests/golden/sass-spec/min_pass_rate.txt` is a ratchet: a full run
fails below the recorded floor, and each round raises it with a comment stating
what moved. `tests/if_error.rs` additionally compares whole error annotation
blocks verbatim, so a mis-placed caret cannot slip through the substring matcher
used by the shared runner.

Known gaps, recorded rather than hidden: `with`-config propagation,
`expressions/if/css.hrx#0` (lexer), and the legacy three-argument `if()`
argument-count blocks, which need a builtin declaration snippet plus an
`if-function` deprecation warning.

## Ground rules

- Rust edition 2024 (toolchain 1.99), rxrust pinned to `=1.0.0-rc.5`.
- Ownership over shared mutability; state moves *by value* through operators.
- No `println!` / `eprintln!` — `tracing` macros only, with spans around
  cross-function pipelines.
- No inline tests in `src/`; everything lives under `tests/`.
- ≤ 500 lines per source or test file — split rather than overflow.
- CodeGraph CLI (`callers`, `impact`, `explore`, `node`) for code queries.

## License

MIT — see [LICENSE](LICENSE). The vendored submodules (`sass-spec`,
`element-plus`) keep their own licenses.
