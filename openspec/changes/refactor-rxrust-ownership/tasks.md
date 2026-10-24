# Tasks

## 0. M0 — 立闸与基线（先让欠账可见）

- [ ] 0.1 `style_gate.rs` 加 `std_chain` 第五维（D4 启发式：链尾 `.collect/.fold/.try_fold/.for_each` 且前有 `.iter/.into_iter/.map/.filter/...`；排除单值组合子与 `any/all/find/position`），逐文件计数
- [ ] 0.2 首次 bless 生成 `tests/golden/std-chain-baseline.txt`（当前全量 std 链快照，只降不升），记录基线总数
- [ ] 0.3 全库 hrx 基线快照一份（`QRASS_HRX_RUN` 新 run）+ e2e/unit/clippy/wasm 全绿留档（对拍前态）
- [ ] 0.4 `tests/golden/style-exemptions.md` 改为纯"改造欠账清单 C1–C6"（无豁免语义），与 `refactor-rfp-strict` 5.4/5.5 交叉引用对齐

## 1. M1 — B1 叶层纯 std 链 → owned Observable

- [ ] 1.1 `css.rs`：序列化/切分族里 `.iter().map().collect()` 构集合者改 `Local::from_iter(..).map(..).collect()`
- [ ] 1.2 `value.rs` / `value_number.rs`：render/interp/number 链改流（value.rs 已无 for，重点 std 链维）
- [ ] 1.3 `builtins/*.rs`（selector/string/list/map/math/color_*）叶层 map/filter/collect 链逐文件改流
- [ ] 1.4 `error.rs` / `span.rs`：notes/字符前缀等 std fold/collect 改 `reduce_initial`/`scan_map`
- [ ] 1.5 M1 验收：`std_chain` 维对 B1 文件命中归零 + 全量 hrx A/B `regress=0`、e2e/unit/clippy/wasm 绿

## 2. M2 — B2 有状态 std 折叠 → `reduce_initial`/`scan_map`

- [ ] 2.1 全库扫出 std `try_fold`/`fold`/`for_each` 承载累加器的站点清单（分组、first-wins、去重、校验矩阵）
- [ ] 2.2 逐站改 `reduce_initial`（单值）或 `scan_map`+`collect`（多值），错误经 latched `Option<E>`；每站 hrx A/B `regress=0`
- [ ] 2.3 M2 验收：相关文件 `std_chain`/letmut 命中下降并 bless，全量 A/B 绿

## 3. M3 — B3 游标 → 逐输入状态迁移器（D3）

- [ ] 3.1 `builtins/selector.rs simple_selectors`（6× 字符游标，纯函数、hrx 覆盖）：`Local::from_iter(chars).scan_map(owned Scan, step).reduce_initial/drain` 产 token 列表；先做（金样强、行为可 A/B 证）
- [ ] 3.2 `lexer_detail.rs` `lex_string`/`lex_interp_raw`（引号/插值递归游标）：改 `scan_map` 逐字符迁移器 + owned 状态；递归结构以子流 `flat_map` 或迭代化 owned 栈承载；全量 lexer/语法 hrx A/B `regress=0`
- [ ] 3.3 `parser_if.rs` `modern_if`/`clause_value`/`if_cond`（递归下降 token 游标）：改 `Local::from_iter(tokens).scan_map(ParserState, ..)` + `flat_map` 阶段衔接，消 `loop`
- [ ] 3.4 M3 验收：loops/else-if 命中归零、`std_chain` 下降，全量 A/B + e2e + wasm 绿；遇强行改造会变行为者按 6.x 上报，不留循环

## 4. M4 — B4 `&mut self` 求值/loader 走查 → owned 累加子流

- [ ] 4.1 `loader.rs` 候选分层解析：把被改状态穿成 owned 累加子（`scan_map`/`reduce_initial`），错误走值通道
- [ ] 4.2 `eval_*` 语句/成员走查、`eval_config` offer/forward、`eval_color_space_symbolic` slot 折叠：同法去 `&mut self`/std 链
- [ ] 4.3 M4 验收：相关 std_chain/letmut 下降并 bless，全量 A/B `regress=0`

## 5. M5 — B5 CLI 入口（先建可验证性）

- [ ] 5.1 新增 `crates/qrass/tests/cli_args.rs`：对 `parse_args` 的入参/出参/错误做 `#[test]` 金样（补上此前缺失的验证网）
- [ ] 5.2 `main.rs parse_args` `while let args.next()` → 对 owned `Vec<String>` 的 `scan_map`/`reduce_initial`（pending-flag owned 状态 + latched 错误），5.1 测试逐条等价
- [ ] 5.3 M5 验收：cli 测试全绿 + 全量 A/B `regress=0`

## 6. M6 — 不可表达上报（贯穿 M1–M5）

- [ ] 6.1 任一构造经尝试仍无法等价表达（不改行为 / 不引 else-if / 不留外部游标）：暂停该文件，写 `reports/rxrust-blocking-findings.md`（最小复现 + 卡点 + 已试流形态 + 行为对照），交用户裁决；不登记豁免、不悄悄留循环

## 7. M7 — 冻结与收口

- [ ] 7.1 style_gate 全库终态：loops=0、else-if=0、`std_chain=0`（± 仅单值组合子/谓词合法白名单外者），逐维只降不升 bless
- [ ] 7.2 全库 std 链 + 游标改造对账报表 `reports/rxrust-ownership-closeout.md`，与 spec R1–R9 逐条核对
- [ ] 7.3 全量验证梯终绿（hrx + e2e + unit + clippy + wasm32）；`openspec` 同步/归档建议

## 8. 承接自 `refactor-rfp-strict`（归档时转入；其 5.4/5.5 已由本 change M7.1 / M3 覆盖，不重复）

- [ ] 8.1 (原 rfp 7.3) `eval_color4.rs`/`builtins/color_transform.rs`/`eval_callable.rs` 等 return top 文件按 ledger 报表批次降量（守卫 return → 单出口 match/`?` 错误链；return 维只降不升）
- [ ] 8.2 (原 rfp 6.1) style_gate 全库报表终态（loops/else-if/std-chain = 0 ± 仅单值组合子）贴入 `reports/rfp-refactor-closeout.md`，与 spec R2/R3 逐条对账
- [ ] 8.3 (原 rfp 6.3) 全量验证梯（hrx + e2e + unit + clippy + wasm32）终绿，`/opsx:archive`

## 9. 承接自 `converge-sass-spec-to-100`（归档时转入；sass-spec 现代套件 81.30%→100% 行为收敛，与本 change 的风格轨单轮只做一边）

> 保留原文以不曲解 golden 证据；每项仍以全量 hrx A/B `regress=0` 对拍（W1/M0/M9.1 已在原 change 完成，不在此列）。

- [ ] 9.1 (W2) From the 1.3 matrix, implement (space, none-mask) serialization per surface in `fmt_comp`/legacy `to_css`/inspect paths; one surface group per hrx-gated commit
- [ ] 9.2 (W2) Powerless hue rules for hsl/hwb/lch/oklch incl. `is_powerless` answers; cross-check `change/hsl`, `mix` PASS clusters stay green
- [ ] 9.3 (W2) `special_functions` (calc/var serialization with none channels) completion; `functions/special` sweep
- [ ] 9.4 (W2) Floor ratchet + test pins per surface group in `tests/`
- [ ] 9.5 (W3) mix: hue-interpolation methods over fitted constants; percentage/rgb legacy form per golden evidence
- [ ] 9.6 (W3) mix: `lastwards` operand precedence + weight semantics sweep of remaining `color/mix` mismatch
- [ ] 9.7 (W3) to-gamut `minde` (oklab ΔE clipping) per golden evidence table; `is-in-gamut` cross-check
- [ ] 9.8 (W4) Introduce module-scoped member view (exports post show/hide) consumed by `meta.*-exists` family and `@include`/call error paths; keep registry for resolution only
- [ ] 9.9 (W4) `meta/load_css` residue sweep (user-member access through configured module, apply(load-css) gaps)
- [ ] 9.10 (W4) Full-module-visibility cluster re-run; floor ratchet
- [ ] 9.11 (W5) adjacent-compounds: selector parser split decision for `[c]d` class inputs + whitespace-joined serialization + DEPRECATION plumbing (stderr not compared — css effect only)
- [ ] 9.12 (W5) bogus-combinators invalid-selector rule (drop rule body per golden) and trailing-combinator family
- [ ] 9.13 (W5) `supports/comment` 12 + remaining grammar clusters (sample→table→cut per golden)
- [ ] 9.14 (W6) Iterate: top got-success/mismatch cluster → golden evidence table → validation/serialization cut → hrx A/B → floor ratchet (repeat until residue = 0 or blocker reports)
- [ ] 9.15 (W6) exists-family siblings (function-exists/global-variable-exists $module ladder) — reuse round-93/94 patterns
- [ ] 9.16 (W6) `.sass` indented-syntax and `@import`-domain cases present in the modern subset: enumerate exact list from residue tsv, cut per cluster or escalate
- [ ] 9.17 (M9) Implement clamp-at-display + unitless-ratio literal reading on top of the audited scale; land the change/edit lossy-mix form (evidence in reports/w2-surface-matrix.tsv)
- [ ] 9.18 (完成) Final full-suite run: 10961/10961 or every remaining case named in an accepted blocker report
- [ ] 9.19 (完成) Floor to `100`, README/task-11.3 status update, openspec sync/archive preparation
- [ ] 9.20 (完成) Memory + docs consolidation (fitted constants table, surface matrix, visibility architecture)
