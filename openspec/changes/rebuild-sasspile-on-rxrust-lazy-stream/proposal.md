# Proposal: 以 rxrust 惰性流为内核重建 sasspile

## Why

`crates/sasspile` 的历次返工都源自同一个根因：**没有从底层把 rxrust 当引擎用，而是自顶向下先设计一棵结构（`Vec<Rule>` AST / 一堆 `Node` / 手写的 `Engine{work-queue,step}`），再把 rxrust 算子"套"到这套命令式结构上去遍历**。心智模型实际上抄的是 dart-sass——物化 AST / 值堆、遍历、急切求值拼串、再回收——那是 **GC 语言的思维**，与 Rust 的**所有权 + 线性消费**相反。结果：`while` 换成 `from_fn`、递归换成一棵先建后走的树，都仍是"算子外衣下的状态机"。

本变更把这条认知固化为**规范契约**，并以它为准，把 sasspile 主链重建成**一条惰性 owned 值流**：值只在终点 `subscribe` 合并时求值输出，循环交给 `flat_map`、一份产出喂多消费者交给**多播**，全程不物化中间结构、绝不写命令式循环或共享可变对象。

## What Changes

- 确立**架构契约 capability**：惰性求值（终点 `subscribe` 才合并输出 CSS）、消费自身（值被 move、消费一次）、一到多用多播（`Subject`/`share`/`behavior_subject`+`peek`）、禁止物化树/手搓 loop/`&mut self` 共享可变、dart-sass 仅作行为 oracle。
- 以契约重建 sasspile 主链 capability：parsing 产出惰性 owned 值流（非物化 AST）；values 建模 `Expr`/`Value` 惰性求值；control-flow 用 `flat_map`/`filter` 表达 `@each`/`@for`/`@if`；css-output 只在终点合并生成。
- **conformance/audit capability**：`ledger` A/B 每步守 `regress=0`；"隔离件+金样单测→白名单守门→接线"的生长纪律；`sasslint` 持续审核，且新增"算子外衣下的状态机"检测（`from_fn`+可变 `VecDeque`/`Vec<Frame>`、万能 `Acc`）。

## Impact

- Affected specs: `rxrust-lazy-stream-contract`（新）、`sasspile-core`（新）、`conformance-and-audit`（新）。
- Affected code: `crates/sasspile/src/*`（`lexer`/`values`/`control flow`/`render`/`lib` 主链），`crates/sasslint`（增规则），`crates/ledger`（A/B，已存在）。
- 行为基线：以 `sass-spec/*.hrx` + `element-plus` 为准；每个 PR `regress=0`。
- 风险：主链重建可能中途回归 → 以 `regress=0` 为提交闸，达不到即回退到绿色基线。
