# 从零手写 rxrust —— 逐行实战教程（零基础 → 完整 rc.6）

> 目标：读者**亲手**一行一行敲，最终写出与 `rxrust = 1.0.0-rc.6` 能力等价的库；
> 每一行源码都要讲清"它做什么 / 为什么必须在这 / 不这样写会怎样"。
> 本目录的验收由 `crates/docsizer` 机器核对，规划与契约见
> `openspec/changes/add-rxrust-line-by-line-tutorial/`（proposal / design / tasks / 3 份 spec）。

## 现状（实测，不靠感觉）

| 指标 | 值 | 来源 |
|---|---|---|
| 参照源码 | 88 文件 / 29,755 行 | `docsizer inventory` |
| 可教行（分母） | **14,116**（排除 test 模块 11,644、空行 1,677、纯 `use` 239、纯括号 2,079） | 同上，分类可对账 |
| 课次 | **58 课 / 103 段**（`--target 300 --max 600`） | `docsizer plan` |
| 行级**预分配** | 100%（`COVERAGE.tsv` 由 `plan` 生成，每行都有归属课号）；**是否已写成课文**另由 `VERIFICATION.tsv` + `words` 跟踪 | `docsizer coverage` |
| 非代码字数 | 51,913 / 目标 ≥ 1,000,000（含待重写的旧草稿） | `docsizer words` |
| **可读性闸** | ✅ 已发布的 5 课：**0 先用后定义 / 0 超预算**<br>⚠ 全目录仍报 214 处——全部来自 `advanced-drafts/` 与旧草稿，即重写清单 | `docsizer terms --strict` |
| 交付进度 | 前置卷（按“没写过代码”重写）**R.1–R.5 已交付**：程序/编译 → 变量/可变 → 函数/返回值 → 列表/循环 → 所有权/搬走 | `tasks.md` 的 V0R 节 |

## 目录结构

```
docs/rxrust-from-scratch/
  README.md            ← 本文件（进度表 + 怎么用）
  GLOSSARY.md          ← 术语表：可读性闸的数据源（一行=一个概念，标注首现课）
  MAP.tsv              ← 88 个源文件 → 7 卷（`plan` 的输入）
  COVERAGE.tsv         ← 行级账本：源文件 / 行区间 / 课号 / 主·复习
  VERIFICATION.tsv     ← 每课代码的编译/真跑结论
  v0-foundations/      ← 零基础前置卷（真正写给没写过代码的人，已交付 5 课）
    00-01-what-a-program-does.md      程序、编译
    00-02-variables-and-mutability.md 变量、可变
    00-03-functions-and-return-values.md 函数、返回值
    00-04-lists-and-loops.md          列表、循环
    00-05-ownership-and-moving.md     所有权、搬走
  advanced-drafts/     ← ❗旧“五课 V0”：写给已会 Rust 的人，读者错位，
                        将被 R.7–R.16 逐课吸收后删除（terms 报的 214 处全在这里）
  v1-observer-source/  v2-subscription/  v3-context-rc-factory/
  v4-three-layer-traits/  v5-higher-order-multicast/  v6-scheduler-async-final/
```

**课号与"哪一课管哪几行"以 `COVERAGE.tsv` 为唯一事实源**（由 `docsizer plan` 生成）——
不在本文重复列举，避免课次一改两处就打架。该账本目前是**预分配**：随着课文写出，
`VERIFICATION.tsv` 逐课从 `PENDING` 转为实测结果。

## 七卷主线

| 卷 | 主题 | 你会写出 |
|---|---|---|
| V0 | ✅ 已完成：前置地基（所有权/借用、关联类型→GAT、闭包三能力、共享可变、宏最小知识） | 能读懂 V1 起每一行签名 |
| V1 | 消费者与源头：`Observer`/`Emitter`/`DynObserver`/`FnMutObserver` + `from_iter`/`of`/`empty`/`never`/`throw_err` | 第一个能跑的 push 流 |
| V2 | 订阅与取消：`Unsubscribable`/`Subscription`/`SingleAssignment`/`DynamicSubscriptions` | 外部 `unsubscribe()` 真让源头停 |
| V3 | 线程模型：`Context`/`Local`/`Shared`/`MutRc`/`MutArc` + `factory` | 一份算子代码两种线程模型 |
| V4 | 三层 trait（`ObservableType` GAT / `CoreObservable` / `Observable`）+ 一阶与状态算子 | 手写 `map`/`filter`/`take`/`scan`/`reduce`/`collect` |
| V5 | 高阶与多播：`merge_all`/`flat_map`/`switch_map`/组合算子 + `Subject` 家族/`Connectable`/`publish`/`ref_count` | 流中流、一 producer 喂 N consumer |
| V6 | 调度与异步互操作：`Task`/`TaskHandle`/`Scheduler`、时间算子、`box_it`、`from_future`/`from_stream`/`into_future`/`into_stream`、`prelude` 组装 | 你自己的 `minirx::prelude::*` |

## 每一课的固定八段（不可省）

① 本课要解决的问题（不解决会怎样）→ ② 你先试 → ③ 最小实现完整代码（可编译）→
④ **逐行**讲自己的代码 → ⑤ 贴 rc.6 源码并逐行对照（给 `文件:行`）→ ⑥ 差异与设计取舍 →
⑦ 你会撞到的编译错（≥3，原文 + 复现步骤）→ ⑧ 练习 + 参考答案。

## 怎么跑一课的验证（工装 MUST 在 workspace 之外）

```bash
cargo new --lib /tmp/minirx-check                       # 只此一次
C=/Users/panglijun/rust/qrass/docs/rxrust-from-scratch/v0-foundations/code
cp $C/00-04-shared-mutable.rs /tmp/minirx-check/src/lib.rs
mkdir -p /tmp/minirx-check/tests
cp $C/00-04-shared-mutable.tests.rs /tmp/minirx-check/tests/lesson.rs
cp $C/00-04-shared-mutable.answers.rs /tmp/minirx-check/tests/answers.rs
cd /tmp/minirx-check && cargo test && cargo clippy --all-targets -- -D warnings
```

> 为什么在仓库外：实测在 `target/` 下 `cargo new` 会让 cargo 改写根 `Cargo.toml` 的
> `workspace.members`（本项目遇到过一次并已复原）。

## 谁该从哪里开始

完全没写过代码 → 从 `v0-foundations/00-01-what-a-program-does.md` 开始，顺着 00-02…00-05 往下。
每课最多两个新词，不先用未定义的行话（这一条由 `docsizer terms --strict` 强制）。
已经有 Rust 基础 → 可直接进 V1，卡住时回来查前置卷。

## 三句自检（任何一步卡住先背）

- **推 not 拉**：源头决定何时发，你不回头遍历。
- **状态是值**：需要沿流被读的状态，就是 `scan` 发射的那个 item；藏进对象就是 GC 思维。
- **终点只有一个**：中途别把流压回 `Vec` 再加工；收口用 `subscribe`（同步）或 `into_future().await`（异步）。
