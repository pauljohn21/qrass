# W1 fitted constants (task 2.2) — evidence-only solver output

Source: `cargo test --test constant_solver dump -- --ignored` over
`reports/w1-golden-pairs.tsv` (1154 clean rows, tasks 1.2). All digits below
are recovered from golden input→output pairs; the reference implementation's
internals were not consulted.

## xyz(D65) → lab effective matrix C (x_n = C·X_d65, normalized by the D50 white)

```
C = [1.0867307799454928, 0.023796508854945487, -0.052050701373721815,
     0.029627808770381393, 0.9904344267533352, -0.01707379906341556,
     -0.011202265286917965, 0.018246403482108393, 0.9112472274903048]
max relative residual over the fitted set: 6.6e-15   (f64 machine precision)
```

Row 1 normalizes x/Xn, row 2 gives Y/Yn=1 exactly ⇒ this single matrix fully
encodes the D65→D50 adaptation *and* the white-point division for the lab
segment. Independent rows: 5 lab sections ⇒ 15 equations vs 9 unknowns.

## xyz(D65) → oklab effective matrix (for cross-validation)

```
C_oklab = [0.39424908435086314, 0.601148321223212, 0.12526150216582183,
           0.39323346859678854, 0.6002370633503553, 0.12701515921834206,
           0.39723958918030655, 0.591749168518813, 0.13125641672685165]
max relative residual: 1.4e-15
```

`C_oklab` reproduces the currently compiled oklab pipeline to 1e-15 ⇒ the oklab
segment is already exact (its FAILs are serialization-surface cases, W2), and
the fit method itself is validated on an independent space pair.

## Conclusion

- The `to_space` tail delta lives entirely in the **lab/lch effective matrix**:
  production must replace `BRADFORD_D65_TO_D50 → white-division` composite with
  this fitted C (forward) and C⁻¹ (reverse). Task 2.3.
- No other segment needs refitting first: gamut→XYZ rows already pass the
  string check (they feed through the same `mul`).

## Final production form (task 2.3, applied)

A standalone-C swap broke 4 D50 round-trip cases (the Bradford pair must stay
algebraically self-closing). The shipped form instead keeps the two-step
structure and fits the *pair*:

```
white_d50 = [0.9642956764295194, 1.0, 0.8251046025099973]   (4 golden rows agree at 1e-12)
BRADFORD_D65_TO_D50 := diag(w)·C_fit =
  [1.0479297925443183, 0.022946870602940705, -0.05019226628980399,
   0.029627808770338507, 0.9904344267519015, -0.01707379906339085,
   -0.009243040646773988, 0.015055191492342076, 0.7518742814267251]
```

`bradford_d50_to_d65 = inv()` keeps every D50 round trip exact by construction.
Full hrx: 8911 → 9032 (+121). Four `to_space` cases moved from passing-on-luck
to failing for a *different* reason (color-mix `far` form ×2 — task 2.5 — and
`none` serialization ×2 — W2); none regressed to a wrong ordinary value.
