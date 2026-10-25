# 从零手写 rxrust —— 一步一步把响应式库写出来

> 目标：你**亲手**把 rxrust 的核心从 0 写到能用，并且每一行都知道它为什么在那儿、它想解决什么、
> 不这么写会怎样。每课结束都有一个**可编译可跑**的小里程碑；每课都对照
> `rxrust-1.0.0-rc.6` 真实源码（给出 `文件:行`）核对，凡是与源码不一致的地方，以源码为准。
>
> 读源码的姿势：源码不是用来"验证我想法对不对"的材料，它是**设计输入**。写任何一步之前，
> 先去问"这套体系里已经有哪个动词干这件事"。这套教程就是把这个问题一路问到底。

## 参照与环境

| 项 | 值 |
|---|---|
| 对照版本 | `rxrust = "=1.0.0-rc.6"`（`ls ~/.cargo/registry/src/*/rxrust-*` 自查路径） |
| 源码位置 | `~/.cargo/registry/src/<mirror>/rxrust-1.0.0-rc.6/src`（本教程里所有 `文件:行` 都指这里） |
| 你写的 crate | `minirx`（`edition 2021+`，零依赖起步，L08 起才允许 `futures`，L09 起才允许 `tokio`） |
| 已有资料 | `docs/rxrust-source-analysis/`（逐文件精读，回答"它是什么"）；本套教程回答"它为什么长这样、怎么自己写出来" |

rxrust 顶层只有 9 个模块（`lib.rs:57-66`），这也是本教程的目录：

```rust
pub mod context;      // Local / Shared 两种"容器与共享指针"的抽象
pub mod factory;      // Local:: / Shared:: 这些构造入口
pub mod observable;   // ObservableType / CoreObservable / Observable 三层 + 各源头
pub mod observer;     // Observer / Emitter / DynObserver / FnMutObserver
pub mod ops;          // ~50 个操作符，一个文件一个
pub mod rc;           // MutRc / MutArc：同一套代码在单线程/多线程下都能跑的关键
pub mod scheduler;    // Task / TaskHandle / LocalScheduler / SharedScheduler
pub mod subject;      // 多播总线：一个生产者喂多个消费者
pub mod subscription; // 取消（Unsubscribable / Subscription / SingleAssignment）
```

## 课程表

| 课 | 你写出什么 | 里程碑（可跑） | 主要对照源码 |
|---|---|---|---|
| [01](01-the-observer-and-the-first-source.md) | `Observer` 四方法 + 一个 `from_iter` 源头 | `source.subscribe(closure)` 打印出 1,2,3 | `observer.rs:19-43`、`observable/from_iter.rs:29-75` |
| 02 | `Subscription` / `Unsubscribable` / 取消；`FnMutObserver` | 能 `unsubscribe()` 并让源头**提前停** | `subscription.rs`、`subscription/single_assignment.rs`、`observer.rs:255-277` |
| 03 | `Context`：`Local`(Rc/RefCell) vs `Shared`(Arc/Mutex)；`MutRc`/`MutArc` | 同一个 `map` 编译出两种线程模型 | `context.rs`、`rc.rs` |
| 04 | 三层 trait：`ObservableType`(GAT) / `CoreObservable<C>` / `Observable: Context`；`lift`/`transform`/`into_inner` | 手写 `map` 只需 20 行且对 Local/Shared 通吃 | `observable.rs`（trait 骨架）、`observable.rs` 的 `transform`/`lift` |
| 05 | 一阶算子：`filter` `take` `take_while` `skip` `map_to` `tap` `distinct_until_changed` | 链式 `from_iter(1..10).filter(..).take(2)` 只发 2 个 | `ops/*.rs` 一阶组 |
| 06 | 状态类算子：`scan`（发射累加值）/ `scan_map`（私藏累加子）/ `reduce` / `reduce_initial` / `collect` | 亲手测出"状态必须随流走"的唯一写法 | `ops/scan.rs`、`ops/scan_map.rs`、`ops/reduce.rs`、`ops/collect.rs` |
| 07 | 高阶：`map` 返回 observable + `merge_all(concurrent)` ⇒ `flat_map`；`concat_all`；`SingleAssignment` 与取消传播 | 流中流；顺序 vs 并发两种合并 | `ops/flat_map.rs`、`ops/merge_all.rs` |
| 08 | 类型擦除：`box_it` → `LocalBoxedObservable` / `SharedBoxedObservable`；`DynObserver`/`Emitter` | 把链存进 `Vec`、跨函数返回 | `observable/boxed.rs`、`observer.rs:95-244` |
| 09 | 多播：`Subject`、`BehaviorSubject`+`peek`、`ConnectableObservable`、`publish()`/`ref_count()`/`share()` | 一个生产者喂 N 个消费者；冷→热 | `subject.rs`、`subject/subject_core.rs`、`observable/connectable.rs`、`ops/ref_count.rs` |
| 10 | 调度：`Task`/`TaskHandle`/`Schedulable`/`SleepProvider`；`LocalScheduler`(spawn_local) vs `SharedScheduler`(tokio::spawn)；`delay`/`timer`/`interval` | 只有时间算子需要 runtime；测出哪些算子是内联同步 | `scheduler.rs`、`observable/timer.rs`、`observable/interval.rs` |
| 11 | 异步互操作：`from_future`、`from_stream`、`into_future`、`into_stream`；`create` 与 `&mut dyn Emitter` | 用 `into_future().await` 收口，不再手搓 `mpsc` | `observable/from_future.rs`、`from_stream.rs`、`create.rs`、`ops/into_future.rs`、`ops/into_stream.rs` |
| 12 | 组装：`prelude`、`factory`(`Local::`/`Shared::`)、feature 门控、`test_scheduler` 与虚拟时间测试 | 你自己的 `minirx::prelude::*` | `prelude.rs`、`factory.rs`、`Cargo.toml [features]`、`scheduler/test_scheduler.rs` |

## 怎么用这套教程

1. **一定要手敲。** 每课代码都不长，但要求你打进去并跑绿；复制粘贴会跳过唯一有效的学习环节（编译器的报错就是源码在跟你对话）。
2. 每课的固定结构：`目标 → 先自己试 → 逐行讲解 → 对照真实源码 → 设计取舍（不这么写会怎样）→ 练习`。
3. 每课末尾的**「反例」**不要跳过：那些是我们项目里真金白银踩过的坑（把状态塞进自造对象、用配对算子带状态、`.ok()` 吞错、`collect` 后再排序……）。
4. 三句话自检（任何一步卡住先背）：
   - **推 not 拉**：源头决定何时发，你不回头遍历。
   - **状态是值**：需要沿流被读的状态，就是 `scan` 发射的那个 item；藏在对象里就是 GC 思维。
   - **终点只有一个**：中间别把流压回 `Vec` 再加工；收口用 `subscribe`（同步）或 `into_future().await`（异步）。

## 学完之后你手上会有什么

一个约 1500 行的 `minirx`：具备 rxrust 的全部骨架（三层 trait、Context 抽象、取消、多播、调度、
异步互操作、约 25 个算子），而且**你知道每个零件的来处**——那时你再回去读 rxrust 任何一行，
看到的都是"我写过它，所以我懂它为什么必须这样"。
