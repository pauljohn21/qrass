# sass-spec-conformance Specification

## Purpose
Defines the behavioral contract qrass must satisfy to be considered fully
conformant with the modern sass-spec suite: every hrx case passes under the
project's A/B gate, and all behavioral rules are derived from golden evidence.

## Requirements

### Requirement: Full modern-suite conformance
qrass SHALL pass every test case of the sass-spec modern hrx suite (10961
cases) under the golden comparison rules (CSS compared after canonicalization,
first error line compared with substring equivalence).

#### Scenario: Program completion
- **WHEN** the full hrx suite runs against the final build of this change
- **THEN** the pass count equals the total case count and the pass-rate floor reads 100

#### Scenario: Any intermediate round
- **WHEN** any implementation step of this change is submitted
- **THEN** the full hrx run shows zero regressions relative to the previous accepted baseline

### Requirement: Golden-derived behavior only
Every observable behavior rule adopted by this change SHALL be derived from
local hrx golden input→output/error evidence pairs; the reference
implementation's source code, documentation, or internals SHALL NOT be used as
a behavioral authority.

#### Scenario: Rule adoption
- **WHEN** a new validation or serialization rule is introduced
- **THEN** the accompanying code comment or test cites the concrete golden file(s) and section names whose evidence pair implies the rule

### Requirement: Cross-space numeric agreement
Color conversions between CSS Color 4 spaces SHALL serialize to the same
decimal strings the goldens show, including channel values that currently
differ only in the eighth significant digit.

#### Scenario: To-space golden table
- **WHEN** each `to_space/*.hrx` case converts its input color to the target space
- **THEN** the serialized channels match the golden strings exactly

#### Scenario: Constants are evidence-fitted
- **WHEN** conversion constants are (re)chosen
- **THEN** they are the output of a fitting harness run over golden pairs, and the harness result is recorded in `reports/`

### Requirement: Missing-channel serialization
A color carrying `none` channels SHALL serialize per space on every surface
(value, `inspect`, `channels:`), preserving powerlessness through conversions
exactly where the goldens show `none` retained and computing concrete values
wherever the goldens show one.

#### Scenario: No-alpha families
- **WHEN** `lab/lch/oklab/oklch` no_alpha cases print a color whose lightness or chroma is missing
- **THEN** the output matches the golden spelling (none vs computed value) case for case

#### Scenario: Powerless hue
- **WHEN** a gray (zero chroma or equalized channels) color prints as hsl/hwb/lch/oklch
- **THEN** the hue slot follows the golden (none or a concrete angle), and `color.is-powerless` answers accordingly

### Requirement: mix and to-gamut algorithms
`color.mix()` SHALL honor its `$method` semantics including percentage-form
serialization and operand precedence, and `color.to-gamut($space, $method:
minde)` SHALL clip with the oklab-based minde method the goldens demonstrate.

#### Scenario: Mix golden forms
- **WHEN** `mix` cases serialize legacy results
- **THEN** the rgb/percentage/alpha form chosen matches the golden per the operand evidence

#### Scenario: Minde clipping
- **WHEN** `to-gamut` runs with the minde method on out-of-gamut colors
- **THEN** the returned color matches the golden channels exactly

### Requirement: Module member visibility
Mixin/function/variable existence and `@forward` visibility SHALL be answered
from each module's own exported-member view (post show/hide), never from the
global bare-name registry; duplicate forwards of the same member SHALL report
`Two forwarded modules both define …` at the point a consumer could observe
the clash.

#### Scenario: Hidden forward invisible
- **WHEN** a module forwards a member set filtered by `show`/`hide` and the importer queries a dropped member
- **THEN** the existence check answers false although the defining module loaded

#### Scenario: Diamond conflict deferred
- **WHEN** two forwards of the same member reach a consumer that actually selects it
- **THEN** the compile reports the two-forwarded-modules error, while unused duplicate forwards stay legal

### Requirement: Cluster blocker reporting
If a cluster is proven unreachable by golden-fitted means, the program SHALL
stop that workstream and deliver an explicit blocker report (evidence,
attempted methods, residual case list) instead of silently excluding the
cases from the completion definition.

#### Scenario: Stop-loss fires
- **WHEN** a workstream's documented stop-loss criterion triggers
- **THEN** `reports/` gains a blocker document naming every still-failing case key and the change does not claim completion without one
