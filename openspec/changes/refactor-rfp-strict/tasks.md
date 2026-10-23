# Tasks

## 1. M0 门禁与基线（先立闸，后动刀）

- [x] 1.1 `tests/style_gate.rs` 行级扫描器：剔除注释/字符串后统计 `src/` 的 `for`/`while`/`loop`、`else if`、外层 `let mut` 计数，生成 `tests/golden/style-baseline.txt`（当前基线快照，含逐文件分解）
- [x] 1.2 crate 顶挂 `#![deny(clippy::needless_range_loop, clippy::explicit_iter_loop)]`，修掉既有触发项
- [x] 1.3 基线行为快照：全量 hrx 一份 tsv + e2e/unit/wasm 全绿记录（重构对拍的固定前态）
- [x] 1.4 豁免清单文件 `tests/golden/style-exemptions.md` 建档（初始为空，模板含文件/构造/理由/复审条件四字段）

## 2. M1 S1 叶层纯函数清洗

- [x] 2.1 `css.rs`：`tidy_condition`/`is_bogus_combinator`/`split_top_commas`/`strip_loud_comments`/`normalize_selector` 及 render 族全链式化（`from_iter + scan_map + collect`，D2 终端规则），本 change 内未提交的命令式残留一并收编
- [x] 2.2 `selector_unify_complex.rs` + `selector_unify.rs` 循环/条件链消除
- [x] 2.3 `builtins/selector.rs`（18 处）与 `plain_css.rs`（11 处）清洗
- [x] 2.4 S1 其余 ≤5 循环小文件清零（`value_number.rs`、`css` 邻族等，按 style_gate 报表逐项勾销）
- [x] 2.5 S1 验收：style_gate 对 S1 文件命中归零；e2e → 全量 A/B 0 回归 → unit/clippy/wasm 绿

## 3. M2 S2 eval 参数/内置层清洗

- [x] 3.1 `eval_bind.rs`/`eval_meta.rs` 残余循环（bind_ns_vars、config 视图、mixin-exists 族）链式化
- [x] 3.2 `builtins/color_*.rs` + `eval_color*.rs` 循环消除（校验矩阵已多为链式，清 for 收集残余）
- [x] 3.3 `eval_control.rs`（12 处）清洗
- [ ] 3.4 style_gate S2 文件命中归零 + 全套 0 回归验收

## 4. M3 S3 extend 引擎清洗

- [x] 4.1 `eval_extend.rs`（18 处）：占位符组收集/代换双层循环 → `scan_map` + `flat_map`
- [ ] 4.2 `selector_extend*.rs`（22 处）与 extend 计划族清洗
- [ ] 4.3 style_gate S3 命中归零 + 0 回归验收（extend 金样组重点复核 escaping/round-trip 钉）

## 5. M4 S4 parser/lexer 游标组合子化

- [ ] 5.1 `lexer_detail.rs` 残余 `while` 扫描 → `scan_map`/`take_while` 组合子（词干分派沿用 LexPunct 派宏）
- [ ] 5.2 `parser_expr.rs`/`parser_groups.rs`/`parser_supports.rs`（16 mut 热点）游标消除
- [ ] 5.3 `parser_at.rs`/`parser_decl.rs`/`parser.rs` 残余清洗；实测不可行者逐个登记豁免清单（sealed 纯函数形态，四字段齐全）
- [ ] 5.4 style_gate 最终基线冻结为「仅豁免清单命中」+ 0 回归验收

## 6. M5 收口

- [ ] 6.1 style_gate 全库报表终态（迭代循环 = 0 ± 豁免）贴入 `reports/rfp-refactor-closeout.md`，与 spec R2/R3 逐条对账
- [ ] 6.2 `openspec/config.yaml` 的 context 增补「RFP 契约生效」一句话约束（防未来 change 不知情违约）
- [ ] 6.3 全量验证梯（hrx + e2e + unit + clippy + wasm32 目标）终绿，归档建议 `/opsx:archive`

## 7. R3 收紧批（用户裁决：守卫 return + 评论署名）

- [x] 7.1 src 全库 `dart` 评论署名清洗为 golden-only（102→0，cf5240c，ledger 验证 0/0）
- [x] 7.2 守卫 return 消除试点：`eval_color_space.rs`（函数级 20 return → 单出口分类 match）
- [ ] 7.3 `eval_color4.rs`/`builtins/color_transform.rs`/`eval_callable.rs` 等 return top 文件按 ledger 报表批次降量
- [x] 7.4 style_gate 新增 return-guard 计数维度（函数体内 ≥2 语句级提前 return 文件级计数），基线只降不升
