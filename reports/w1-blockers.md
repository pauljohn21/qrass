# W1 residual hand-offs (task 2.4 log)

As of commit 4010229 (floor 83.0), three to_space cases remain failing after
the fitted lab segment + lossy mix form; each is owned by another workstream:

1. `to_space/lab/lch.hrx#10` — lossy-mix edge (form present, digit/edge miss).
2. `to_space/oklch/prophoto_rgb.hrx#11` — mix-form edge on a prophoto target.
3. `to_space/xyz_d50/lch.hrx#5` — `alpha/transparent` (`/ 0`) serialization → **W2** surface matrix.

None is a numeric-tail failure any more; the W1 fitting program is closed.

## M4 wildcard-conflict (use/error/member family, ~9-18 cases) — BLOCKED

Naive cross-module same-name detection (`wildcard_conflicts` ledgers keyed by
Rc module identity or `defined` bare name) regressed **element-plus e2e** both
times: the project's `@use X as *; @use Y as *` where Y `@forward`s X (mixed
local/forwarded exports of one member) must NOT conflict, while two *local*
definitions of the same name MUST (use/error/member goldens). Correct modeling
= per-member **upstream canonical identity** through forward chains: needs
Module to carry `member → origin-url` (forwarded entries currently lose the
provenance — `eval_module.rs` builds `forward_buf` merges without upstream ids).
Next attempt preconditions: (1) origin recorded at the @forward collection site;
(2) element_plus_e2e run BEFORE full hrx — the 4-case e2e is the cheap tripwire
for this whole cross-module-identity family.

## M3 mix algorithm (color/mix, ~60 cases) — OPENED, probe reverted

First-run evidence (`missing.hrx` matrix read this session, probe reverted to
9147 baseline):
* per-channel interpolation in the $method space; BOTH-missing → `none` output;
* ONE-side missing: with `$method: rgb` the result shows the OTHER side's value
  (e.g. `rgb(0 100 200) x rgb(200 none 0)` → g prints 100) — but this is NOT
  yet proven to be "fill with other value": the rec2020 twin of the same pair
  prints g = 37.57%, consistent with the CSS spec rule "missing component
  converts as ZERO in the source space THEN interpolates in method space"
  (0-side value + weight algebra can land on 100 for the identity-weight rgb
  case). The two readings only diverge under non-trivial matrices — build the
  decision table from `both_weights/` + `explicit_weight/` rows (weight 0/100%
  edges) BEFORE coding;
* output form: legacy-first-operand display (method=rgb all-concrete → hex;
  rec2020 → `rgb(p%, p%, p%)` legacy percent commas; hsl a → `hsl(h, s%, l%)`);
  both modern → method-space `color(...)`;
* `lookup()` space names are `srgb` etc. — normalize $method rgb→srgb and keep
  the *display* decision (hex) keyed off the original method spelling.
Next session: dump the full `color/mix/*.hrx` evidence table via the solver's
pair extractor (extend `collect_pairs` to mix() calls), decide fill-vs-zero,
then one A/B-gated implementation pass (color_mix.rs has room; color_ops.rs is
at 533 lines — the dispatch edit must land there or split it first).

### M3 mix session-2 probes (numeric, throwaway rustc harness)
CONFIRMED: * `mix(transparent, #0144bf, 70%)` = alpha-weighted premul with the
  alpha=0 operand contributing ZERO rgb (exact on r/g/b + out_a 0.3) — weight
  is the FIRST color's (dart's `$weight` = w1, w2 = 1-w1);
  out_a = w1a1 + w2a2 (plain, verified on the 42% case = 0.49 ✓).
UNRESOLVED: the 42% case rgb channels match NEITHER encoding-domain premul
  (10.07%) NOR linear-light premul (24.47%); back-solving gives an implausible
  W1≈0.194 ⇒ the golden's operands are NOT what the literal spelling shows —
  the legacy `rgba(#hex, a)` carrier's channel domain (0..255 vs % vs the
  hex's ORIGINAL c4 payload) must enter differently. NEXT: extend the solver
  `collect_pairs` to mix() (4 args incl. weight) and batch-evaluate algebraic
  candidates (encode-domain × {a-weighted, plain-w} × display paths) against
  ALL ~200 concrete mix goldens; let residuals pick the formula — no more
  hand-solving.
