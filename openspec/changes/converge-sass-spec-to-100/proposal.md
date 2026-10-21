# Proposal

## Why

qrass has converged the sass-spec modern suite from 75.11% to **81.30%** (8911/10961,
rounds 75–94, every round gated by a zero-regression full hrx A/B). 2050 FAIL cases
remain and the project's OpenSpec task 11.3 defines the finish line as full
conformance. Triage across the last twenty rounds has produced a complete,
data-backed map of the residue; what is missing is a single change that drives every
cluster to closure with explicit methods and stop-loss rules, instead of the
round-by-round opportunistic cuts that have plateaued at +5…+10 per round.

## What Changes

One coordinated convergence program that attacks all 2050 residual failures through
five mechanism-level workstreams plus a sweep loop:

- **W1 Numeric-tail solver** (≈390 cases: `to_space` 326, cross-space round trips):
  recover the exact conversion constants used by the golden outputs by
  least-squares/integer-relation fitting over golden input→output pairs (a Rust
  `#[test]`-harness solver, dart internals never consulted), then re-verify every
  color cluster against the fitted constant set.
- **W2 Missing-channel & powerlessness serialization** (≈450 cases: `no_alpha`
  families 72, `is_powerless` 50, `special_functions` 27+36, `invert` 38, `hwb/rgb`
  residue): model `none` propagation and powerless hue/channel rendering per space
  for every serialization surface (legacy rgb/hsl/hwb forms, Color 4 bare/wrapper
  forms, `channels:` inspect form).
- **W3 mix + to-gamut algorithms** (≈126 cases): implement the spec'd `mix` method
  pipeline (percentage-form golden serialization included, `lastwards` operand
  precedence) and `to-gamut(in gamut)` minde clipping.
- **W4 Module member visibility** (≈94 cases: `forward/member` 40, `use/error` 44,
  `meta/load_css` 26): per-module member views replacing the shared bare-name
  registry lookup for existence/visibility checks, and consumer-side deferred
  diamond-conflict reporting.
- **W5 Grammar & selector residues** (≈60 cases: `selector/combinator` 27,
  `supports/comment` 12, adjacent-compounds spacing, bogus-combinator omission).
- **W6 Sweep loop**: the proven selection method (got-success piles ⇒ shared
  validation matrix; golden evidence pairs only) applied until the residue list is
  empty, one hrx-A/B-gated round per cluster.

## Capabilities

### New Capabilities
- `sass-spec-conformance`: the full behavioral contract qrass must satisfy — the
  modern sass-spec hrx suite as the authoritative oracle, the golden evidence-pair
  derivation rule, per-cluster completion requirements, and the zero-regression
  A/B acceptance gate.

### Modified Capabilities
<!-- none: openspec/specs/ is empty; main specs will be synced from this delta on archive -->

## Impact

- **Code**: `color4*` (convert/constant tables), `builtins/color*` (serialization,
  mix, gamut, transform), `value.rs` (legacy forms), `eval_*` module visibility,
  `parser_supports`/`selector*`, new `tools/`-style solver test harness under
  `crates/qrass/tests/`.
- **Baselines**: `min_pass_rate.txt` ratchets 81.3 → 100; `reports/hrx-roundNN.tsv`
  A/B chain remains the hard gate for every step.
- **Risk honesty**: W1 was previously declared "no local hook" (round-87 stop-loss);
  this change re-opens it *with* a new method (solver fitting). W5/W6 residues are
  individually small but numerous. Each workstream carries its own stop-loss
  criterion in design.md; the 100% target is the program's completion definition —
  any cluster proven unreachable must be escalated as an explicit blocker report,
  not silently dropped.
