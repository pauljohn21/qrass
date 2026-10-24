# Design

## Context

Baseline `reports/hrx-round94a.tsv`: 8911/10961 (81.30%). The 2050 residue was
fully triaged over rounds 84–94; cluster sizes below are measured from that tsv.
Established facts this design builds on:

- The `to_space` residue is a ~1e-8 **relative rounding** delta in the shared
  conversion constants, not a model gap (round-87 evidence matrix); operation
  order (pre-multiplication) was disproven bit-for-bit (round-85 probe).
- `none` propagation between tristimulus spaces and missing-as-0 computation
  already match the goldens; the `no_alpha`/`is_powerless`/`special_functions`
  families diverge on **serialization surface** choices, not conversion math.
- The global bare-name mixin/function registry is shared across modules, which
  is why per-module visibility questions (`forward/member`, `use/error`,
  existence predicates) fail: lookups see members the importer must not see.
- Every accepted step so far was gated by a full-run hrx A/B with zero
  regressions; the floor file ratchets monotonically.

## Goals / Non-Goals

**Goals:**
- Order the 2050 cases into six workstreams with a concrete, testable method each.
- Define per-workstream stop-loss and a program-wide completion definition
  (100% or an explicit blocker report per the spec).
- Keep every step reversible and A/B-gated; the baseline chain never weakens.

**Non-Goals:**
- No refactor beyond what a workstream needs; no touching the Local-observable
  pipeline architecture; no deviating from the golden-only derivation rule.
- Not a deadline plan: sequencing is by dependency, budget is per-milestone.

## Decisions

### D1 — W1 constant fitting via golden pairs (solver), not dart reading
Build a `#[test]`-harness solver (`crates/qrass/tests/constant_solver.rs`):
extract ≥40 high-precision golden pairs per space pair from `to_space/*.hrx`
(12-significant-digit outputs ⇒ per-row linear solves are over-determined),
least-squares fit each 3×3 matrix and white point in f64, quantize candidates to
the shortest decimal literals that reproduce **all** golden strings in the
family, then swap the fitted tables into `color4.rs` behind the existing
`mul()` path. Alternative considered and rejected: brute-force digit mutation
of current constants (search space too large without evidence constraint).
Fallback order: fit gamut→XYZ rows first (they also feed `to_xyz_d65`), then
XYZ→lab/oklab segments. If a sub-family stays unreachable after fitting, emit
its case list into `reports/w1-blockers.md` (spec-mandated) and continue.

### D2 — W2 serialization-surface matrix before any algorithm change
`no_alpha` (72) and friends are one table: (space, none-mask, surface) → golden
text. Extract the full table mechanically (awk over the 19+6 hrx files, same
method as the round-87 matrix), then decide per cell whether the fix is
`fmt_comp`, legacy `to_css`, `inspect` form, or powerless hue; only after the
table is complete does code change. This inverts the failed guess-and-check
pattern and reuses the proven got-success method.

### D3 — W3 algorithms kept in own modules
`mix` method pipeline (hue interpolation longer/shorter/increasing/decreasing,
premultiplied weight, golden percentage serialization form) lands as
`builtins/color_mix.rs` expansion; minde clipping as `color_gamut.rs`
extension. Both consume W1's fitted constants — sequenced after W1 to avoid
fitting twice.

### D4 — W4 per-module member views
Add a module-scoped member index (the `Module` already carries exported
`mixins/functions/vars` post filter_forwarded): existence predicates and
consumer lookups consult (current module exports + wildcard imports + namespaced
bindings) instead of the flat registries. Diamond-forward conflict detection
moves to first consumer access (`Two forwarded modules…`). Larger blast radius:
mitigated by the registry staying as a *resolution* path while *visibility*
answers come from views — resolution order unchanged where legal.

### D5 — W5/W6 remain cluster-per-round
Selector combinators (adjacent-compounds whitespace insertion, bogus-combinator
rule omission) and the ~500-case sweep residue keep the established loop: one
golden-evidenced cluster per hrx-gated round. The `special_functions` (49) and
`supports/comment` (12) shapes were already sampled; their tables join D2's matrix.

### D6 — Sequencing
W1 → W2 → W3 (W3 depends on W1 numerics) runs parallel-safe with W4/W5, which
touch disjoint files. W6 sweeps fill the remainder after each milestone. Each
milestone ends with: full hrx A/B, floor ratchet, `codegraph sync`, commit; push
only on user confirmation as always.

## Risks / Trade-offs

- **W1 may not fully converge**: fitting reproduces sampled pairs but unseen
  space pairs could still round differently; risk capped by blocker-report clause
  (completion definition stays honest). Mitigation: fit against *all* 257 hrx
  files at once, not a sample.
- **W4 visibility rework** is the only architectural change; fallback is a
  narrower per-API (existence predicates first, resolution later) split.
- **Floor-file/shell hazards**: destructive rewrites of baselines are banned in
  favor of in-place edits (round-92 incident lesson is a standing rule).
- Volume risk: 2050 ⇒ even at +50/round this is dozens of rounds; the program
  structure trades speed for zero-regression safety, which the project has
  never relaxed.
