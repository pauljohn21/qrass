# W1 residual hand-offs (task 2.4 log)

As of commit 4010229 (floor 83.0), three to_space cases remain failing after
the fitted lab segment + lossy mix form; each is owned by another workstream:

1. `to_space/lab/lch.hrx#10` — lossy-mix edge (form present, digit/edge miss).
2. `to_space/oklch/prophoto_rgb.hrx#11` — mix-form edge on a prophoto target.
3. `to_space/xyz_d50/lch.hrx#5` — `alpha/transparent` (`/ 0`) serialization → **W2** surface matrix.

None is a numeric-tail failure any more; the W1 fitting program is closed.
