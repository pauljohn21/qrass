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
| builtins/selector.rs `simple_selectors` (6× cursor `while`, L435-467) | sigil-driven char cursor | parser-cursor family — scheduled for S4 (task 5.3) combinatorization or formal sealed-pure-function registration | resolved at S4 close |

Baseline snapshot (task 1.3): `reports/hrx-m5k.tsv` — 9188/10961 PASS, e2e 4/4,
unit 515/0, clippy -D warnings GREEN, at commit `8501a84` + this change's M0
work. Every S1–S4 batch must re-verify 0 regression against a fresh snapshot
taken on that batch's parent commit (gate R1/D6).
