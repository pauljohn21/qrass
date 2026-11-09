---
name: rxrust-source-analysis
description: 编写、审查或调试使用 rxrust crate（1.0.0-rc.6）的 Rust 代码、且必须确认某个原语的真实行为时使用 —— 算子签名与发射时机、Local 与 Shared 上下文、Observer/Subscription 契约、错误传播、Subject 多播与 ref_count、box_it 类型擦除、scheduler 要求，或为何 subscribe(closure) 吞掉错误。
---

# rxrust 源码分析

## 概览

`rxrust = 1.0.0-rc.6` 的源码级参考，来自对 crate `src/` 的逐行阅读
（实测：**88 个 `.rs` 文件 / 29,755 行**）。**核心原则：绝不凭记忆陈述 rxrust 行为
—— 本文每条论断都带 `file:line` 出处，所以引用出处而非猜测。**

## 何时使用

- 必须核查某算子的签名边界、item 按值还是按引用、发射时机或 error/complete 行为。
- 某 `subscribe(|v| ...)` 闭包始终看不到 `error`/`complete`。
- `and_then` 缺位；各阶段必须链式衔接。
- 在 `Local` 与 `Shared` 之间抉择，或 `into_shared` / `into_local` / wasm32 上的 `Send` 摩擦。
- 一份输出必须喂给多个消费者（`Subject`、`behavior_subject`、`publish`、`ref_count`）。
- 跨函数边界出现不可名状的链类型（`box_it`）。
- 审查带 GC 味的 rxrust 代码：`Arc<Mutex<业务状态>>`、手搓 `VecDeque` + `from_fn`
  驱动器、物化的 `Vec<Rule>` 被递归遍历。

不适用：rxrust 0.15（API 不同）或非 Rust 的 ReactiveX。

## 速查 —— 问题 → 章节

| 要确认什么 | 读 |
|---|---|
| push 模型、`ObservableType`/`CoreObservable`/`Observable` 契约、subscribe = 嵌套 Observer 包装、冷与热、同步执行、错误协议 | [01-philosophy-and-architecture.md](references/01-philosophy-and-architecture.md) |
| `Scope`/`Context`/`LocalCtx`/`SharedCtx`、`Local`/`Shared`、`MutRc`/`MutArc`/`CellRc`/`SharedCell` | [02-context-scope-rc.md](references/02-context-scope-rc.md) |
| `Observer`/`Emitter`/`FnMutObserver`、`DynObserver`、`Subscription` 组合子、`SingleAssignment` 安装与取消竞态 | [03-observer-subscription.md](references/03-observer-subscription.md) |
| `factory.rs` 各源（`of`/`from_iter`/`create`/`defer`/`from_*`/`timer`/`interval`/`empty`/`never`/`throw_err`）；wasm32 注记 | [04-observable-trait-and-sources.md](references/04-observable-trait-and-sources.md) |
| **51** 个 `ops.rs` 模块中的任何一个（分 A–J 组）+ 选算子表 | [05-operators.md](references/05-operators.md) |
| `Subject<P>` 重入、`Subscribers`、`BehaviorSubject` + `peek`、`fork`/`connect`/`ref_count` | [06-subject-multicast-connectable.md](references/06-subject-multicast-connectable.md) |
| `box_it`/`BoxedCoreObservable`/`DynCoreObservable`；`Task`/`TaskHandle`/`Scheduler`/`SleepProvider`、`LocalScheduler`/`SharedScheduler` | [07-type-erasure-and-scheduler.md](references/07-type-erasure-and-scheduler.md) |
| 编译器管线配方、循环/状态/树/多播惯用法、陷阱表、Do/Don't | [08-recipes-and-pitfalls.md](references/08-recipes-and-pitfalls.md) |
| 取消 vs 终止：先占槽后订阅、恰好终止一次、仅在提前终止时取消、锁纪律、Subject 重入、文档与实现对不上的清单 | [09-cancellation-and-terminal-ordering.md](references/09-cancellation-and-terminal-ordering.md) |

章节目录与阅读顺序：[references/README.md](references/README.md)。

## 版本钉（关键）

```toml
rxrust = "=1.0.0-rc.6"   # 裸 `cargo add rxrust` 会解析到旧的 0.15 线
```

默认 feature `scheduler` 门控 `Local`/`Shared` 与 boxed 别名；`nightly` 会改变
`map` 闭包签名与借用 item 输出。

## 查证契约

1. 给问题分类：源、算子、observer/subscription、上下文、多播、擦除，还是 scheduler。
2. 只翻开映射到的那一章。
3. 引用该章给出的 `file:line`；不要重新推导。
4. 若该章沉默，就明说并去读 crate 源码 —— 不要外推。

**危险信号 —— 停下查证：** 「rxrust 有 `and_then`」、「`subscribe` 会报告错误」、
「这需要 `Arc<Mutex>`」、「我要把 `VecDeque` 包进 `from_fn`」、
「我要把输出 `collect` 起来再喂给每个消费者」（那正是多播的用途，`06 §8`）。

## 备注

- 对行为异常的流，先收集 tracing 证据
  （**必备背景：** superpowers:systematic-debugging）。
- 第 08 章 §5 是源项目的 `sasspile`/`sasslint` 验收环
  —— 一个带完整示例的样例，不是别处的要求。第 01–07 章与项目无关。
- `rxrust` 本身就是引擎（`flat_map`、owned 累加器、冷链惰性）；
  不要在其上重建一个流引擎。
