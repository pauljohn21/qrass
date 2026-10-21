# Cluster inventory — M0.1 (from reports/hrx-m0-baseline.tsv)

PASS=8911 (81.30%)  FAIL=2050  SKIP=445  TOTAL=10961+445

## FAIL × form
  1028  output mismatch
   559  expected success, got error
   274  expected error, got success
   189  error text mismatch

## FAIL × workstream domain (top 30)
  56 color/to_space/rgb
  37 color/to_space/prophoto_rgb
  31 color/rgb/one_arg
  29 color/to_space/oklch
  28 color/mix/missing
  28 color/lab/special_functions
  28 color/hwb/three_args
  26 forward/member/import
  22 functions/special/prefixed
  20 color/change/error
  19 forward/error/member
  19 color/to_space/srgb_linear
  19 color/to_space/oklab
  19 color/oklch/no_alpha
  18 use/error/member
  18 color/to_space/srgb
  18 color/oklab/no_alpha
  18 color/lch/no_alpha
  17 import/load
  17 color/to_space/xyz_d50
  17 color/lab/no_alpha
  17 color/adjust/hwb
  16 meta/calc_args
  16 color/to_space/lab
  16 color/to_space/a98_rgb
  16 color/hwb/four_args
  15 meta/global_variable_exists
  15 meta/function_exists
  15 function
  15 color/to_space/xyz

## assignment to workstreams
W1: color/to_space (326) + cross-space numeric residue
W2: no_alpha/is_powerless/special_functions/invert/missing-serialization (~450)
W3: color/mix (71) + color/to_gamut (55)
W4: forward/member (40) + use/error (44) + meta/load_css (26) + exists siblings
W5: selector/combinator (27) + supports/comment (12) + grammar bits
W6: everything else, sweep per cluster
