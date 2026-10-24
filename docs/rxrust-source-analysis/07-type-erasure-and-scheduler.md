# 07 · 类型擦除（`box_it`）与调度器（`scheduler`）

## 一、类型擦除：把不透明算子链收成一个可命名类型

### 1. 为什么需要

每个算子返回 `Self::With<Op<Self::Inner, F>>`，链一长类型就不可书写、也不可跨函数返回、
更无法把"不同来源的流"塞进同一个 `Vec`。`box_it()` 用 `dyn` 把核擦掉。

### 2. `DynCoreObservable`（对象安全的核，`observable/boxed.rs:28-61`）

```rust
pub trait DynCoreObservable<Ctx>: where Ctx: Context {
  fn dyn_subscribe(self: Box<Self>, ctx: Ctx) -> Ctx::BoxedSubscription;
}
// 空布：任何核，只要其 Unsub 能装箱，就自动可 dyn
impl<S, Ctx> DynCoreObservable<Ctx> for S
where Ctx: Context, S: CoreObservable<Ctx, Unsub: IntoBoxedSubscription<Ctx::BoxedSubscription>> {
  fn dyn_subscribe(self: Box<Self>, ctx: Ctx) -> Ctx::BoxedSubscription {
    (*self).subscribe(ctx).into_boxed()
  }
}
```

注意 `Ctx` 是**整个 Context**（含 observer + scheduler），不是裸 observer——所以擦除时
**scheduler 被保留**（`boxed.rs:24-27`）。

### 3. 装箱别名（`observable/boxed.rs:86-116`）

```rust
pub type BoxedCoreObservable<'a,Item,Err,S> =
  Box<dyn DynCoreObservable<LocalCtx<BoxedObserver<'a,Item,Err>, S>> + 'a>;
pub type BoxedCoreObservableSend<'a,Item,Err,S> = … + Send + 'a;   // Shared
pub type BoxedCoreObservableMutRef<…> = … BoxedObserverMutRef …;    // 广播 &mut Item
```

`BoxedCoreObservableClone`（+ `DynCoreObservableClone::clone_box`，`:41-76`）是**可再次订阅**的
擦除类型（链本身 `Clone` 时才有）。

### 4. 用户侧 API（`ops/box_it.rs` 测试；`observable.rs:2002-2120`）

- `Local::of(42).box_it()` → `LocalBoxedObservable<'_, i32, Infallible>`（`test_box_it_with_type_alias`）。
- `into_inner().into_boxed()` 直接拿裸 `BoxedCoreObservable`，再 `Local::new(boxed)` 包回（`test_local_box_observable`）。
- `box_it_clone()`：产 `LocalBoxedObservableClone`，可 clone 后分别 `subscribe`（`test_box_it_clone_observable_method`）。
- 不同源链（`of` / `from_iter.map` / `from_iter.filter`）擦后同型 → 放进 `Vec<LocalBoxedObservable<…>>`
  统一订阅（`test_heterogeneous_collection` / `test_box_it_heterogeneous_with_type_alias`）。

> 技能手册的"跨函数边界传流用 `box_it()` → `LocalBoxedObservable`"正是对应这里。项目编译器管线
> 里每个阶段函数返回 `Stage<T> = LocalBoxedObservable<'static, Result<T,E>, Infallible>` 就是它。

### 5. 相关 observer/subscription 也各有一套装箱

observer 侧 `DynObserver`/`BoxedObserver*`（`03 §一.5`）、subscription 侧
`BoxedSubscription*`（`03 §二.2`）——三者对齐，才能在擦除边界把"下游 observer""上游核"
"取消句柄"都 dyn 化，同时保住 `Item<'a>`/`Err`/`S`。

## 二、调度器（`scheduler.rs`，feature `scheduler`）

只有**时间/调度类算子**（`05 §G`：delay/timer/interval/buffer_time/debounce/sample/throttle/
observe_on/subscribe_on）才用调度器；其余全同步（`01 §5`）。

### 1. 核心类型

| 类型 | 角色 | 出处 |
|------|------|------|
| `Task<S>` | 数据 + 执行逻辑的**状态机**；`Task::new(state, step_fn)` | `scheduler.rs:170` |
| `TaskState` | `Pending`/`Finished`/…：step 函数返回它决定是否续跑 | `:138` |
| `TaskHandle` | **同时是 `Subscription`（取消）与 `Future`（await）**的统一句柄 | `:375` |
| `Scheduler<S>: Clone` | `schedule(&self, source: S, delay: Option<Duration>) -> TaskHandle` | `:567-578` |
| `Schedulable<Sch>` | 把 `S` 变 `Future`（`into_future`）；`Task<..>` 靠它可排 | `:231` |
| `SleepProvider` | 提供 `sleep(Duration)->Future`，让内部 `Task` 可延时 | `:255` |
| `TaskFuture<Sch,S>` | 把 Task 适配成真 `Future` 供运行时 poll | `:269` |

`TaskHandle` 一身二职是关键：时间算子的取消（`Subscription`）与"等它完成"（`Future`）用同一句柄，
`03 §二.2` 的 `EitherSubscription`（DelaySubscription：TaskHandle→SourceSubscription）即配合它。

### 2. `LocalScheduler` / `SharedScheduler`（`:633` / `:706`）

- `LocalScheduler`（`impl_scheduler!(LocalScheduler, spawn_local)`，`:669`）：
  - Native：`tokio::task::spawn_local`（需 `LocalSet`，非 Send）。
  - WASM：`wasm_bindgen_futures::spawn_local`（`:591-608`）。
- `SharedScheduler`（`:706`，`impl_scheduler!(SharedScheduler, …)`）：Native 用 `tokio::spawn`
  （要求任务 `Send`）；WASM 因单线程**回落 `spawn_local`**（`:681`）。
- **Panic 安全**：所有被排 future 用 `CatchUnwind`（`catch_unwind(AssertUnwindSafe(...))`，`:456-519`）
  包裹——**panic 被转成任务完成/错误，不会击穿运行时**。

### 3. "调度器是句柄，不是运行时"（自定义时的铁律，`:64-72`）

`Local::default()`/`Shared::default()` 会被算子**频繁再造**；调度器应指向**全局 / thread-local /
共享 `Arc`** 的同一运行时，**绝不在 `schedule` 里新建线程池**（否则每次 `Default::default()` 泄漏）。
`context.rs` 测试里那个 `InstanceScheduler(7)` 就是在验证"自定义 scheduler 实例会被逐层保留、
不被 `Default` 顶替"（`context.rs:662-731`）。

自定义最小实现（照 `context.rs:199-243` / `scheduler.rs:77-104`）：
```rust
#[derive(Clone, Copy, Default)]
struct MyScheduler;
impl SleepProvider for MyScheduler {
  type SleepFuture = std::future::Ready<()>;
  fn sleep(&self, _: Duration) -> Self::SleepFuture { std::future::ready(()) }
}
impl<S: 'static + Schedulable<MyScheduler>> Scheduler<S> for MyScheduler {
  fn schedule(&self, source: S, _delay: Option<Duration>) -> TaskHandle {
    // 必须 spawn 到“共享运行时”，别新建
    TaskHandle::finished()
  }
}
type MyLocal<T> = LocalCtx<T, MyScheduler>;   // 之后 MyLocal::of(..).map(..) 全可用
```

## 三、和 wasm32 / 项目目标的契合

- 非时间管线：`Local` 全程同步、`subscribe` 后同步取结果——wasm32-unknown-unknown 无需任何
  tokio runtime（`01 §5`、`04 §四`）。
- 需要时间算子：`Local` + `LocalSet`/`spawn_local`；别在 wasm 上强用 `SharedScheduler` 配带
  `Rc` 态的算子（`take` 等）→ `Send` 不满足（`factory.rs:595-599`）。
- 关掉 default feature `scheduler` 可彻底去掉 tokio/计时依赖，自带极简 `Scheduler`（`scheduler.rs:57-60`）。

---
下一篇 `08-recipes-and-pitfalls.md`：编译器式管线配方 + 常见坑 + Do/Don't。
