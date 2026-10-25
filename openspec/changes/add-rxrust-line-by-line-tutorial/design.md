# Design — 从零手写 rxrust 教程（逐行 · 零基础 · ≥100 万字）

## 0. 一句话

把 rc.6 的 **29,755 行源码**（扣除 test 模块与占位行后 **可教 14,116 行**，`docsizer` 实测）按
"读者亲手建构的顺序"切开，每行都给讲解；字数不是目标而是覆盖率的结果。

## 1. 体量与字数推导（**实测分母**，`docsizer inventory` 产出；这是 100 万字的来源）

| 项 | 数 | 依据 |
|---|---|---|
| `src` 总行 | 29,755（88 个文件） | `docsizer inventory` |
| — `#[cfg(test)]` 模块 | 11,644（占 39%，所以必须排除，不能当可教内容） | `excl_test_module` |
| — 空行 / 纯 `use` / 纯括号行 | 1,677 / 239 / 2,079 | `excl_blank` / `excl_pure_use` / `excl_brace_only` |
| **可教行（分母）** | **14,116** | 29,755 − 11,644 − 1,677 − 239 − 2,079 = 14,116（逐类可对账） |
| 课次（`--target 300 --max 600`） | **58 课 / 103 段**，落在课程契约的 45–60 带内 | `docsizer plan` + `coverage 100.00% unassigned=0 duplicates=0` |
| 每行平均真讲解 | **≥ 70 字** | "作用 + 存在理由 + 不这样写的后果"三问的最小长度 |
| 逐行讲解合计 | ≈ 98.8 万字 | 14,116 × 70 |
| V0 前置卷（零基础） | ≈ 12 万字 | 所有权/闭包 trait/泛型/Rc-Arc/内部可变性/object-safety/GAT/宏 |
| 练习 + 参考答案 + 编译错解读 | ≈ 25 万字 | 每课 ≥3 个真实编译错 + ≥4 练习 |
| **预期总量** | **≈ 135 万字（非代码口径）** | 达标线 100 万，余量 35% |

反灌水：任何"复述源码原文、跨卷复制、把代码块算进字数"都在 `docsizer words` 的**非代码口径**下无收益
（实测现存草稿：2 文件 / 总字符 12,458 / 非代码 7,613）。

## 2. 文件布局

```
docs/rxrust-from-scratch/
  README.md                 # 总目录 + 怎么用 + 卷内进度表
  GLOSSARY.md               # 术语表（中文释义 + 英文原词 + 首现课号）
  MAP.tsv                   # 88 个源文件 → 7 卷归属（plan 的输入）
  COVERAGE.tsv              # 行级覆盖账本：源文件\t行区间\t课号\t主/复习（plan 的输出）
  VERIFICATION.tsv          # 每课编译验证结论：课号\tbuild\ttest\tclippy\t关键报错
  v0-foundations/            lesson 00-04 …（零基础前置，不覆盖源码）
  v1-observer-source/        lesson 01-06   observer.rs / from_iter / trivial
  v2-subscription/           lesson 07-10   subscription*
  v3-context-rc-factory/     lesson 11-16   context.rs / rc.rs / factory.rs
  v4-three-layer-traits/     lesson 17-24   observable.rs(trait) + 一阶/状态 ops
  v5-higher-order-multicast/ lesson 25-34   flat_map / merge_all / subject / connectable
  v6-scheduler-async-final/  lesson 35-48   scheduler / boxed / future-stream / prelude / 组装
```

单课文件命名 `NN-kebab-title.md`，一课 MUST 自包含可读（含其前置的 5 行回顾）。

## 3. 课的固定八段（与 lesson-format spec 一致）

① 问题（不解决会怎样）→ ② 你先试 → ③ 最小实现完整代码 → ④ 我的代码逐行 →
⑤ rc.6 源码逐行对照（`文件:行`）→ ⑥ 差异与取舍 → ⑦ 你会撞到的编译错（≥3）→ ⑧ 练习+答案。

## 4. 覆盖账本格式（工具可解析）

```
observable/from_iter.rs     29-75    02    主
observer.rs                 19-43    01    主
observer.rs                 255-277  05    主
ops/map.rs                  1-120    17    主
```
- 一行 MUST 只有一条"主"；"复习"允许多条；
- `docsizer coverage` 以 inventory 为准算 `unassigned/duplicates`。

## 5. 验证工装（不入库，避免污染产品码）

- 临时 crate：MUST 建在 workspace **之外**（实测坑：在仓库目录树内 `cargo new` 会被改写根 `Cargo.toml`
  的 `workspace.members`）。例：`/tmp/minirx-check/`；每课累计追加模块 → `cargo build/test/clippy -D warnings`。
- 记录进 `VERIFICATION.tsv`：`课号\tbuild\ttest\tclippy\t关键报错(可空)`。
- 失败时按调试协议：先在报错处加最小复现片段（`#[test]` 放临时 crate 的 `tests/`），
  取证后再改课文；MUST NOT 为了让课文通过而删内容或降要求。

## 6. 写作流程（每课一个循环）

1. 从 `COVERAGE.tsv` 取该课区间 → 读源码原文（不凭记忆）；
2. 先写"读者会看到的最小可编译版本"，在临时 crate 里跑绿；
3. 写逐行讲解；每行至少回答"做什么/为什么在这/去掉会怎样"；
4. 造 3 个真实编译错并记录修法；
5. 写练习与答案；更新 `COVERAGE.tsv`/`VERIFICATION.tsv`/`GLOSSARY.md`；
6. `docsizer` 三命令复核 → 提交（一课一提交，diff 可 review）。

## 7. 与"以 rxrust 为核心"的关系

教程不是旁观：它要求把每个设计选择讲成"框架给了什么，所以我不用发明什么"。
本项目反复返工的根因（先凭经验写、再回头查框架；把状态藏进自造对象；在算子上面再包壳）
被逐条转成 V1–V6 的"反例"章节，并引用已在仓库里落地的实测断言：
`crates/sasspile/tests/pairing.rs`（`with_latest_from` 冷源配到终值 `[(1,300),(2,300),(3,300)]`）、
`crates/sasspile/tests/multicast.rs`（`ref_count` 也不改变这一事实；`Shared::subject` 的正确喂法）、
`ops/scan.rs` vs `ops/scan_map.rs` 的 `Output` 语义差（状态该不该是 item）。

## 8. 阶段划分（详见 tasks.md）

V0 → V1 → V2 → V3 → V4 → V5 → V6，每卷末尾 MUST 通过：逐行锚点抽查 + `docsizer coverage/words`
+ 累计编译验证。卷内允许并行按课拆分，但**不得跳卷**（后卷依赖前卷写出的代码）。

## 9. Risks

| 风险 | 缓解 |
|---|---|
| 退化成"挑重点讲概念" | `docsizer coverage` 的 `unassigned=0` 硬闸 + 行级锚点抽查 |
| 为零基础讲 GAT/宏导致篇幅失控 | V0 独立成卷，V1 起只引用 V0 已讲特性，不重复展开 |
| 教程代码与 rc.6 语义偏离（教了错的） | 每课第 ⑤ 段强制贴源码 `文件:行` 对照；偏离 MUST 写进"差异与取舍" |
| 字数压力导致灌水 | 非代码字数口径 + 反例条款；字数只在卷末做**下限**核对，不做逐课指标 |
| 与 sasspile 主线争抢会话预算 | 本变更纯文档 + 独立 bin，`regress=0` 与 sasslint 闸不受影响；按卷推进可中断 |

## 10. Open Questions（定稿时确认）

1. `minirx` 参考实现是否入库（默认不入库，仅 `target/` 验证）。
2. V0 前置卷是否接受"只讲本课用得上的 Rust 子集"（默认是；否则篇幅翻倍）。
3. 是否配 `minirx` 的测试卷（每课练习的自动判分）——默认只做人工对照答案。
