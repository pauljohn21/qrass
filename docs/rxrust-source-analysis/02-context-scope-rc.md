# 02 · Context / Scope / rc —— Local 与 Shared 的所有权底座

rxrust 把"单线程 vs 多线程"做成**纯类型层**的一件事：同一套源与算子代码，只换 `Local`/`Shared`
两个 marker，底层用 `Rc<RefCell>` 还是 `Arc<Mutex>` 就由 `Scope` 关联类型决定。业务代码里**没有
`if cfg(thread)` 分支**，也没有共享可变挡箭牌。

## 1. `Scope`：类型级 marker（`context.rs:21-49`）

```rust
pub trait Scope {
  type RcMut<T>: From<T> + Clone + RcDerefMut<Target = T>;
  type RcCell<T: Copy + Eq>: SharedCell<T>;
  type BoxedSubscription: Subscription;
  type BoxedObserver<'a, Item, Err>;
  type BoxedObserverMutRef<'a, Item: 'a, Err>;
}

pub struct LocalScope;   // Rc/RefCell
pub struct SharedScope;  // Arc/Mutex

impl Scope for LocalScope {
  type RcMut<T> = MutRc<T>;                 // Rc<RefCell<T>>
  type RcCell<T> = CellRc<T>;               // Rc<Cell<T>>
  type BoxedSubscription = BoxedSubscription;
  type BoxedObserver<'a,Item,Err> = BoxedObserver<'a,Item,Err>;
  ...
}
impl Scope for SharedScope {
  type RcMut<T> = MutArc<T>;                // Arc<Mutex<T>>
  type RcCell<T> = CellArc<T>;              // Arc<AtomicCell<T>>
  type BoxedSubscription = BoxedSubscriptionSend;
  type BoxedObserver<'a,Item,Err> = BoxedObserverSend<'a,Item,Err>;  // + Send
  ...
}
```

`Local` 的一切"可变共享"是 `Rc<RefCell>`，`Shared` 是 `Arc<Mutex>`。Send 与否在类型里钉死，
编译期决定，不是运行时开关。

## 2. `Context`：携带 Inner + Scheduler 的依赖注入容器（`context.rs:72-162`）

```rust
pub trait Context: Sized {
  type Scope: Scope;
  type Inner;
  type Scheduler: Clone + Default;
  // 由 Scope 派生的别名：RcMut/RcCell/BoxedObserver/BoxedSubscription…
  type With<T>: Context<Inner=T, Scope=Self::Scope, Scheduler=Self::Scheduler>;

  fn from_parts(inner, scheduler) -> Self;   // 全控构造
  fn new(inner) -> Self;                      // 默认 scheduler
  fn lift<U>(inner: U) -> Self::With<U>;      // 造同族、新类型、默认 scheduler
  fn transform<U, F>(self, f: FnOnce(Inner)->U) -> Self::With<U>;  // 换 inner，保 scheduler
  fn wrap<U>(&self, inner: U) -> Self::With<U>;                    // 借 self 的 scheduler
  fn swap<U>(self, inner: U) -> (Inner, Self::With<U>);            // 换出旧 inner
  fn into_inner(self) -> Inner;               // 消费取 inner
  fn into_parts(self) -> (Inner, Scheduler);
  fn scheduler(&self) -> &Scheduler;
  fn inner(&self) -> &Inner;  fn inner_mut(&mut self) -> &mut Inner;
}
```

`LocalCtx<T,S>` / `SharedCtx<T,S>` 就是 `{ inner: T, scheduler: S }` 两个字段（`context.rs:169-184`），
`impl_context_for_container!` 宏给它们实现 `Context`（`context.rs:248-336`）。三件最常被算子用的：

- `transform`：**算子包装 observer 用它**（`map.rs:121` `context.transform(|obs| MapObserver{obs,func})`）。
  它把 scheduler 原样带过去，所以类型擦除/链式都不会丢调度器（`context.rs:293-298`）。
- `lift` / `new`：**工厂造源用它**（`factory.rs` 每个方法都 `Self::lift(Source)`）。`lift` 用默认
  scheduler（`context.rs:282`）。
- `into_inner`：**源解包 observer 用它**（`from_iter.rs:63` `context.into_inner()`）。
- `swap`：`subscribe`/`subscribe_with` 用它把用户 observer 换进 context（`observable.rs:149,161`）。

`LocalCtx`/`SharedCtx` 自身还实现了 `Observer`（委托 inner）和 `Subscription`（委托 inner）
（`context.rs:406-437`）——这让"包着 observer 的 context"能直接当 observer 往下传。

## 3. `Local` / `Shared` 别名与相互转换（`context.rs:357-463`）

```rust
pub type Local<T>  = LocalCtx<T,  LocalScheduler>;
pub type Shared<T> = SharedCtx<T, SharedScheduler>;
```

- `LocalCtx<T,S>::into_shared(self) -> Shared<T>`，要求 `T: Send + 'static`（`context.rs:439-451`）。
- `SharedCtx<T,S>::into_local(self) -> Local<T>`（`context.rs:453-463`）。
- **转换只换 wrapper、不改执行模型**：非时间源仍 `subscribe` 时同步跑（与技能笔记一致）。
  `into_shared` 的 `Send` 要求会逼你把 `!Send` 的业务值塞进 `Arc`——这正是项目禁止的"共享可变"，
  所以默认走 `Local`（wasm32 目标、非 Send）。

## 4. 自定义 scheduler 不需重写整 trait（`context.rs:186-244`）

只需 `type MyLocal<T> = LocalCtx<T, MyScheduler>;`（`MyScheduler: Clone+Default` 且实现
`Scheduler`）。文档示例给了 `SleepProvider` + `Scheduler<Schedulable>` 的最小实现骨架。

## 5. `rc.rs`：共享可变/原子的四件套 + 统一解引用（`rc.rs`）

| 类型 | 内部 | 用途 | 定义 |
|------|------|------|------|
| `MutRc<T>` | `Rc<RefCell<T>>` | Local 复杂可变状态 | `rc.rs:69` |
| `MutArc<T>` | `Arc<Mutex<T>>` | Shared 复杂可变状态 | `rc.rs:143` |
| `CellRc<T>` | `Rc<Cell<T>>` | Local 简单 Copy 标志/计数 | `rc.rs:108` |
| `CellArc<T>` | `Arc<AtomicCell<T>>` | Shared 无锁原子 Copy | `rc.rs:182` |

统一抽象两个 trait（让上层代码对 Local/Shared 通用）：

```rust
pub trait RcDeref: Clone { type Target; fn rc_deref(&self) -> Self::Ref<'_>; }        // 读
pub trait RcDerefMut: RcDeref {
  fn rc_deref_mut(&self) -> Self::MutRef<'_>;          // 写（RefCell 借用 / Mutex 锁）
  fn try_rc_deref_mut(&self) -> Option<Self::MutRef>;  // 不 panic/不死锁地探测
}
pub trait SharedCell<T: Copy+Eq>: Clone+From<T> {
  fn get(&self)->T; fn set(&self,T); fn compare_exchange(&self,T,T)->Result<T,T>;
}
```

- `Rc`/`Arc` 来自 `rclite`（`rc.rs:23`），`AtomicCell` 来自 `crossbeam_utils`（`rc.rs:22`）。
- **`try_rc_deref_mut` 是关键**：Subject 用它检测"同步再入"——拿不到写借用就 panic（见 `06`），
  `merge_all` 用它把取消/清理放在锁外做（`merge_all.rs:114-131`）。
- `SharedCell`（`CellRc`/`CellArc`）给"一个 bool/usize 标志"这种场景，避开 `RefCell`/`Mutex`
  运行时开销——`ref_count` 的 terminated 标志、`SubscriptionState` 都用它（`context.rs` 关联类型）。

## 6. 为什么这套设计契合 Rust 所有权（呼应项目铁律）

- `RcDerefMut` 把"共享可变"收敛到框架内部两类地方：**多播列表**（`Subject` 的 subscribers）和
  **跨上游取消协调**（`SingleAssignment`/`MergeAllControl`）。业务 item 始终按值 move 过算子，
  不塞 `Arc<Mutex>`。
- `Scope` 让 Local/Shared 是**编译期**选择，不是运行期 `if`——避免 GC 语言里"到处锁"的直觉。
- 累加状态（`scan_map` 的 `acc`、`reduce` 的 `acc`）是 observer 结构体里的**独占 owned 字段**，
  靠 `&mut self` 改（`scan_map.rs:54-75`、`reduce.rs:104-127`），天然满足所有权、无需引用计数。

---
下一篇 `03-observer-subscription.md` 拆消费与取消的两侧契约。
