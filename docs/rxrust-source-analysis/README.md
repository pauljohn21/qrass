# rxrust 源代码解析（可复用参考）

> 对象：`rxrust = 1.0.0-rc.6`。本套文档基于对 crate `src/` 逐文件通读（~29700 行 / 90 文件）整理，
> 目标是让**任何使用 rxrust 的项目**都能直接照它写管线：理解模型、选对原语、避开坑。
> 叙述中文，代码 / 类型 / 文件名 / 标识符保持 English。所有结论均标注源码出处（`文件:行`）。

## 目录

| 文件 | 内容 |
|------|------|
| `01-philosophy-and-architecture.md` | 设计哲学与总体架构：三层契约、push 模型、订阅即"组装嵌套 Observer"、GAT `Item<'a>`、冷/热流、执行模型 |
| `02-context-scope-rc.md` | `Scope`/`Context`/`LocalCtx`/`SharedCtx`、`Local`/`Shared`、`into_shared`/`into_local`；`rc.rs` 的 `MutRc`/`MutArc`/`CellRc`/`CellArc`/`RcDeref(Mut)`/`SharedCell` |
| `03-observer-subscription.md` | `Observer`/`Emitter`/`DynObserver`/`IntoBoxedObserver`/`FnMutObserver`；`Subscription`/`ClosureSubscription`/`SubscriptionGuard`/`TupleSubscription`/`DynamicSubscriptions`/`SingleAssignment`/`SourceWithHandle`/`BoxedSubscription` |
| `04-observable-trait-and-sources.md` | `ObservableType`/`CoreObservable`/`Observable` + 空布实现；`factory.rs` 全部构造源（`of`/`from_iter`/`create`/`defer`/`from_fn`/`from_future*`/`from_stream*`/`timer`/`interval`/`empty`/`never`/`throw_err`/`subject`/`merge_observables`） |
| `05-operators.md` | 完整算子目录（约 70 个），按类分组：签名约束、item 传递语义、Clone 要求、错误/完成传播、并发挥度 |
| `06-subject-multicast-connectable.md` | `Subject<P>` 多播（value / mut_ref 两种 observer）、再入策略、`Subscribers` SmallVec、订阅延迟；`BehaviorSubject` + `peek`；`ConnectableObservable` 的 `fork`/`connect`/`ref_count` |
| `07-type-erasure-and-scheduler.md` | `box_it`/`BoxedCoreObservable`/`DynCoreObservable`；`scheduler.rs` 的 `Task`/`TaskHandle`/`Scheduler`/`Schedulable`/`SleepProvider`、`LocalScheduler`/`SharedScheduler`、自定义调度器要点 |
| `08-recipes-and-pitfalls.md` | 落地配方：直接组合算子的 `flat_map` 管线（无 `Stage`/`fail`/`drain` 框架层；同步 `Local`→`mpsc`、`Shared`/`from_stream`→`into_future().await` 两种收口）、Local vs Shared 选择、in-stream `Result` 错误协议、常见编译错误与 `from_stream` 乱序坑、Do/Don't |

## 三句话记住 rxrust

1. **一切皆 push**：源在 `subscribe` 时同步把 item 推给下游 observer；算子只是"再包一层 observer"。
   没有 `and_then`，跨阶段串联一律 `flat_map`（`map(f).merge_all()`）。
2. **状态放在 observer 里被 move，不放 `Arc<Mutex<_>>`**：`scan_map`/`reduce`/`collect` 把累加子
   封在 `XxxObserver` 内部，靠 `&mut self` 变更（`observer.rs`/`ops/*`）。`Arc<Mutex>` 只出现在
   框架内部的多播（`Subject`）与跨上游取消协调里。
3. **错误/完成是终结且消费 self**：`Observer::error(self)`/`complete(self)` 消耗 observer；
   `subscribe(closure)` 用的 `FnMutObserver` 会**静默丢弃** `error`/`complete`
   （`observer.rs:258-277`）——要处理错误用 `on_error`/`subscribe_with`/`into_future`。

## 版本与 feature（务必注意）

```toml
rxrust = "=1.0.0-rc.6"   # 裸 `cargo add rxrust` 会解析到旧的 0.15 系列，API 完全不同
```

- 默认 feature `scheduler` 打开 → 提供 `Local`/`Shared` 别名与 `LocalScheduler`/`SharedScheduler`
  （`lib.rs:41-44`、`context.rs:357-361`）。关掉它需要自带调度器类型。
- `nightly` feature 影响 `map` 的闭包签名（`FnMut(Item)->Out` vs `FnMut<(Item,)>`），
  决定能否产出**借用型** item（`ops/map.rs:49-107`）。stable 默认即可满足绝大多数用法。

## 阅读顺序建议

- 想快速写管线：直接看 `08-recipes-and-pitfalls.md`，再回查 `05-operators.md`。
- 想理解"为什么 subscribe closure 收不到错误"等机制：读 `01` → `03` → `04`。
- 要用多播 / 共享一份产出：读 `06`（并回顾项目铁律：一到多用 `Subject`/`share`，不共享可变挡箭牌）。
