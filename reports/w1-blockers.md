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
