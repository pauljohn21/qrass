# Spec Delta — rxrust-tutorial-curriculum（课程结构与顺序契约）

## Purpose

规定这套逐行教程的**读者起点、卷次划分、课次顺序与依赖**：一个零基础的程序读者从第 1 课开始，
不需要预先懂响应式、不需要预先懂 GAT/宏，也能一路亲手写到与 `rxrust-1.0.0-rc.6` 等价的完整库。
本文件只锁"课程必须满足什么"，不锁具体措辞。

## ADDED Requirements

### Requirement: 读者起点为零基础

教程 MUST 假设读者只会 Rust 基础语法（变量/函数/struct/enum/`impl`/`match`/简单 trait 使用），
MUST NOT 假设读者懂：闭包 trait(`Fn/FnMut/FnOnce`)、泛型 trait 参数、关联类型与 GAT、
`Rc/Arc/RefCell/Mutex`、`dyn`/object-safety、`async/await`、宏、协变/生命周期高阶约束。
凡上述任一概念首次出现，MUST 在**同一课内**先给出"它解决什么问题 + 最小可运行示例 + 与本课的关联"
三段式讲解，再在源码中使用它。

参照：`docs/rxrust-source-analysis/`（术语表）；`rxrust-1.0.0-rc.6/src/lib.rs:57-66`（模块清单）。

#### Scenario: 首次使用 GAT 前先教 GAT
- **WHEN** 课程需要在 `ObservableType` 中写 `type Item<'a>`
- **THEN** 该课（或其前置小节）已用可运行例子说明"为什么 `type Item` 不够、`<'a>` 从哪来"，
  且读者在此之前已至少手写过一次不含 GAT 的关联类型

#### Scenario: 零基础读者不被告知"自行查阅"
- **WHEN** 某处出现读者未学过的 Rust 特性
- **THEN** 文中不出现"这部分请自行搜索/参考官方文档"式跳步，而以本课内讲解或前置练习补齐

### Requirement: 七卷结构与依赖顺序

教程 MUST 按以下 7 卷组织，且后卷 MUST 只依赖前卷已写出的代码：

| 卷 | 主题 | 覆盖的 rc.6 源码 |
|---|---|---|
| V0 | 零基础前置：所有权/泛型/trait/闭包 trait/智能指针/内部可变性/宏最小知识 | 无（预备） |
| V1 | 消费者与源头：`Observer`/`Emitter`/`DynObserver`/`FnMutObserver` + `from_iter`/`of`/`empty`/`never`/`throw_err` | `observer.rs`、`observable/{from_iter,trivial}.rs` |
| V2 | 订阅与取消：`Unsubscribable`/`Subscription`/`SingleAssignment`/`DynamicSubscriptions` | `subscription*` |
| V3 | 线程模型抽象：`Context`/`Local`/`Shared`/`MutRc`/`MutArc`/`RcDeref*` + `factory` | `context.rs`、`rc.rs`、`factory.rs` |
| V4 | 三层 trait 与算子工厂：`ObservableType`(GAT)/`CoreObservable`/`Observable`+`transform`/`lift`/`into_inner`；一阶与状态算子 | `observable.rs`(trait 部分)、`ops/{map,filter,take,skip,scan,scan_map,reduce,collect,…}` |
| V5 | 高阶与多播：`merge_all`/`concat_all`/`flat_map`/`switch_map`/组合类算子；`Subject` 家族/`Connectable`/`publish`/`ref_count` | `ops/{flat_map,merge_all,…}`、`subject*`、`observable/connectable.rs` |
| V6 | 调度与异步互操作 + 组装：`Task`/`TaskHandle`/`Schedulable`/`LocalScheduler`/`SharedScheduler`、时间算子、`from_future`/`from_stream`/`into_future`/`into_stream`/`create`/`defer`、`box_it` 类型擦除、`prelude`/feature 门控/测试工装 | `scheduler*`、`observable/{boxed,from_future,from_stream,create,defer,timer,interval}.rs`、`ops/{into_future,into_stream,box_it,…}`、`prelude.rs` |

#### Scenario: 依赖不倒置
- **WHEN** 某课讲解 V3 的 `Context` 抽象
- **THEN** 读者手上的 `minirx` 此时已经能跑（V1/V2 已完成），且该课不引用尚未写出的 V5/V6 代码

#### Scenario: 每卷结束有可运行里程碑
- **WHEN** 一卷的最后一课完成
- **THEN** 读者的 `minirx` 在该卷处可编译、`cargo test` 通过，并有该卷"你现在能做到什么"清单

### Requirement: 课粒度与总量约束

单课 MUST 覆盖 **200–600 行**可教源码；教程 MUST 以约 **45–60 课**覆盖全量可教源码；
同一源码行 MUST 只被一课作为"主讲解"归属（允许后续课复习式引用，但需在覆盖账本登记主归属）。

#### Scenario: 一课不至于无法一次读完
- **WHEN** 某课需要覆盖超过 600 行源码
- **THEN** 该课 MUST 被拆分（如 `04a/04b`），不得堆成超长单页

### Requirement: 终态等价性

学完全部卷后，读者手上的 `minirx` MUST 在**能力面**上等价于 rc.6：9 个模块职责齐备、
三层 trait 结构一致、`Local`/`Shared` 双线程模型、取消可传播、`Subject` 家族多播、
调度与时间算子、异步互操作、类型擦除，且 MUST 逐个实现 rc.6 的 52 个 `ops` 文件所代表的能力
（同义算子可合并讲解，但差异 MUST 被点名）。

#### Scenario: 逐能力核对
- **WHEN** 用 rc.6 的 `ops` 文件清单核对教程覆盖
- **THEN** 每个算子都能指向"它在哪一课被写出、与最小实现差在哪、为什么"
