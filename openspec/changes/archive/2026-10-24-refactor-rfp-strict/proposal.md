# Proposal

## Why

qrass 的立项契约是「以 rxrust 为核心的 RFP（reactive-functional programming）风格」，但 sass-spec 收敛的 A/B 驱动迭代模式（每轮追求 PASS 增量最大、回归最小的最小 diff）使叶层代码持续命令式化：当前 `crates/qrass/src` 中 `for`/`while` 循环 323 处 vs 迭代器组合子 376 处——已接近对半开。漂移集中在纯变换叶层（`css.rs`、`selector_*`、`builtins/selector.rs`、`eval_extend.rs`、`parser_supports.rs`），且只要「tsv-only 归因 + 0 回归硬闸」仍是最高纪律，叶层就会继续命令式化——这是结构性问题，越晚开工存量越大。用户裁决：**整体重构，严格使用 rxrust 框架**——一切迭代/状态/数据流形态经 Observable 链表达，消除 `if/else` 链与 `for`/`while`，闭包优先。

## What Changes

- 确立并执行**严格 RFP 风格契约**（三层）：
  - R1 编排层：阶段衔接 `flat_map` 链、错误以 in-stream `Result` 承载、单一 drain 终端（现状已合规，作为锚点）；
  - R2 纯变换/状态层（本次主体）：所有批量变换、累加、字符/状态机走 `Local::from_iter(..).map/filter/scan_map/flat_map..collect/drain`；**不做「同步场景 std 链即可」豁免**（与既有「lexer = scan_map 流迁移器」先例同构，全库只有一种流形态）；
  - R3 例外申请制：仅 ①enum 单表达式 `match` 分派（取值，非控制流）与 ②单值 `Option/Result` 组合子链（`map/and_then/or_else/then_some`，非迭代）保留；`if/else if` 链、守卫堆叠、外层 `let mut` 累加器一律消除。
- 存量清洗按回归风险升序分四批：S1 叶层纯函数（`css.rs`/`selector_unify_*`/tidy 族）→ S2 eval 参数层（`eval_bind`/`eval_meta`/颜色 builtins）→ S3 extend 引擎（`eval_extend`/`selector_extend*`）→ S4 parser/lexer 游标组合子化（风险最高，允许在实测不可行时降级为 R3 例外并记录）。
- 风格门禁固化为机器闸：crate 级 `#![deny(clippy::needless_range_loop, clippy::explicit_iter_loop, ..)]` + 每批 diff 的命令式构造扫描报表（Rust 工具，非脚本）。
- 新代码即刻执行契约：`converge-sass-spec-to-100` 的剩余切片自本 change 批准起按 R1-R3 书写，与本存量清洗穿插推进（每轮只做一边，A/B 归因不混叠）。
- 每批次硬闸：element-plus e2e 绊线（10-15s）→ 全量 hrx（9161+ 例）0 回归 → unit/clippy/wasm 全绿后方可提交；任何批次回归即 `git checkout` 回退并记录，不带伤前进。

## Capabilities

### New Capabilities

- `rfp-style-contract`: 代码库的响应式函数式风格契约——流的表达形态、状态迁移算子、错误传播协议、例外申请边界，以及使该契约可验证、可持续（防再漂移）的门禁要求。

### Modified Capabilities

<!-- 无：本 change 不改变 sass-spec 可观察行为；conformance 行为需求由
     converge-sass-spec-to-100 的 spec 承载，此处仅约束实现风格。 -->

## Impact

- **受影响代码**：`crates/qrass/src` 全目录（40+ 文件，~323 处循环/条件堆叠热点见批次清单）；`pipeline.rs`/`drain.rs`/`lexer.rs` 的既有 rxrust 形态不变，作为模式基准。
- **依赖与工具链**：rxrust `=1.0.0-rc.5`（pin 不动）；新增 clippy deny 列表与扫描报表工具（`crates/` 内 Rust bin 或测试）。
- **风险面**：S4 parser/lexer 组合子化是全库行为最敏感的改动（escaping/round-trip 金样钉），放最后且允许降级；S1-S3 行为面经由 golden 完全钉死，风险受控。
- **主线关系**：与 `converge-sass-spec-to-100`（10/31, 83.8%）穿插执行，本 change 不占用其通过率预算；两 change 的验收闸同为「0 回归」。
