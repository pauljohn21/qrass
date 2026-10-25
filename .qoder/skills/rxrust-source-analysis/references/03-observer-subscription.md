# 03 · Observer / Emitter 与 Subscription —— 消费侧与取消侧契约

一条流有两端：**消费端 `Observer`**（收 next/error/complete）和**取消端 `Subscription`**
（撤订）。本篇把两侧契约讲透——它们决定了"错误为什么会丢""取消怎么传播""状态放哪"。

## 一、Observer（`observer.rs`）

### 1. 主 trait（`observer.rs:19-43`）

```rust
pub trait Observer<Item, Err> {
  fn next(&mut self, value: Item);   // 收值：可变借用 self
  fn error(self, err: Err);          // 终结：消耗 self
  fn complete(self);                 // 终结：消耗 self
  fn is_closed(&self) -> bool;       // 供源判断是否早停
}
```

- **`next` 借 `&mut self`、`error`/`complete` 吃 `self`**：一个 observer 至多终结一次，终结后
  不可能再有 next——把"契约"编码进类型。这是 Rust 所有权对 RX 协议的表达。
- **`is_closed`**：`take`/`first` 等会把它置 closed；源在循环里查它 `break`（`from_iter.rs:66`）。
  取消/背压全靠这一位。

### 2. `Emitter`（`observer.rs:95-99`）——`create` 专用

```rust
pub trait Emitter<Item, Err> {
  fn next(&mut self, value: Item);
  fn error(&mut self, err: Err);      // 注意：&mut self，不消耗
  fn complete(&mut self);
}
```

与 `Observer` 的区别：`error/complete` 也收 `&mut self`。为什么？两个理由（源码注释
`observer.rs:53-94`）：
1. **零成本类型擦除**：`create` 闭包收到 `&mut dyn Emitter`，不需要 `Box<dyn Observer>` 堆分配。
2. **不泄漏抽象**：`create` 不该知道下游是 `MapObserver`/`FilterObserver`/`Subject`/用户 closure，
   统一以 `dyn Emitter` 交互。

`create.rs:32-58` 的 `CreateEmitter(Option<O>)` 就是这座桥：`next` 转发；`error/complete`
用 `self.0.take()` 把内部 observer 取出来再调用其消耗式 `error/complete`——用 `Option::take`
在"借用式 Emitter"和"消耗式 Observer"之间转换。

### 3. `FnMutObserver`（`observer.rs:255-277`）——`subscribe(closure)` 的真相 ⚠️

```rust
pub struct FnMutObserver<F>(pub F);
impl<F, Item> Observer<Item, Infallible> for FnMutObserver<F> where F: FnMut(Item) {
  fn next(&mut self, v) { (self.0)(v); }
  fn error(self, _err: Infallible) {}   // 忽略
  fn complete(self) {}                   // 忽略
  fn is_closed(&self) -> false           // 永不 closed
}
```

- 只对 `Err = Infallible` 实现。**这就是 `subscribe(|v| …)` 会静默丢弃 error/complete 的根因**，
  也是为什么可失败流必须用 `on_error` / `subscribe_with` / `into_future`。
- `is_closed` 恒 `false`：纯 closure 订阅永不主动取消。

### 4. `Option<O>` / `MutRc<Option<O>>` / `MutArc<Option<O>>` 作 observer（`observer.rs:284-335`）

- `None` 吞掉所有事件；`Some` 委托。
- 引用计数包 `Option<O>` 的 observer：`error/complete` 用 `rc_deref_mut().take()` 取走内部 observer
  终结——这是 Subject / MergeAll 里"下游 observer 被多方共享、终结一次"的标准写法。

### 5. 对象安全与装箱：`DynObserver` / `IntoBoxedObserver`（`observer.rs:110-227`）

主 `Observer` 因 `error(self)` 消耗 self 而**非对象安全**。rxrust 用镜像 trait `DynObserver`
（方法名 `box_next/box_error/…`，`self: Box<Self>`）把它变成 vtable 友好；`IntoBoxedObserver`
按 Context 的 Scope 选择装箱目标（Local：`Box<dyn DynObserver<..>+ ‘a>`；Shared：`+ Send`）。
`BoxedObserver` 系列别名见 `observer.rs:234-244`，`Subject` 的订阅者列表就存这个（见 `06`）。

## 二、Subscription（`subscription/`）

### 1. 主 trait（`subscription.rs:102-122`）

```rust
pub trait Subscription {
  fn unsubscribe(self);                 // 消耗 self（终结，对齐 error/complete）
  fn is_closed(&self) -> bool;
  fn unsubscribe_when_dropped(self) -> SubscriptionGuard<Self> { … }  // RAII
}
```

- `impl Subscription for ()`：`is_closed` 恒 `true`，`unsubscribe` 空（`subscription.rs:125-129`）——
  同步跑完即完成的源（如 `FromIter::Unsub = ()`）用它。
- `ClosureSubscription(F: FnOnce())`：把闭包当 teardown（`subscription.rs:58-67`），`create` 里
  返回清理逻辑常用。
- `SubscriptionGuard<T>`：`#[must_use]`，`Drop` 时 `unsubscribe`（`subscription.rs:79-94`）。
  **注意**：不绑到变量会立即取消（注释警告）。

### 2. 组合子（多个上游的取消协调）

| 类型 | 定义 | 语义 |
|------|------|------|
| `TupleSubscription<(A,B,…)>` | `subscription/tuple.rs` | 固定元组，`unsubscribe` 逐个撤；编译期已知数量的多路取消 |
| `EitherSubscription<A,B>{Left,Right,Idle}` | `subscription/either.rs:10-15` | 在两种订阅态间切换（`DelaySubscription`：TaskHandle→SourceSubscription） |
| `DynamicSubscriptions<U>{ next_id, SmallVec<[(id,U);2]> }` | `subscription/dynamic.rs` | 运行期动态增删、按 id 移除、`unsubscribe_all`；≤2 免堆分配；`reserve_id()+insert()` 支持"先要 id 再造 observer"的环依赖 |
| `SourceWithHandle<U,H>{source,handle}` | `subscription/source_with_handle.rs:26-48` | 源订阅 + 可取消 handle 合体，`unsubscribe` 先撤 handle 再撤源；Debounce/Throttle/Delay 撤订时取消已排任务 |
| `BoxedSubscription = Box<dyn BoxedSubscriptionInner + ‘static>`（Shared 版 `+Send`） | `subscription/boxed.rs:7-40` | 类型擦除、可异构存集合 |

`BoxedSubscription` 为何强制 `'static`（`boxed.rs:25-36`）：订阅句柄是**控制句柄不是数据视图**，
必须自持有、可在任意稍后时刻被存进 `MergeAllState`/`Subject` 并调用——不能挂着借用生命周期。

### 3. `SingleAssignment<P>`（`subscription/single_assignment.rs`）——安装 vs 取消的竞态协调

一条"单赋值通道"，状态 `State{Pending(wakers), Assigned(U), Cancelled}`：

```rust
let (install, [handle1, handle2]) = SingleAssignment::<MutRc<_>>::channel();
// handle 可先 unsubscribe（Cancelled）；之后 install(upstream) 会立即取消该 upstream
install(upstream);
```

关键性质（测试逐条钉死，`single_assignment.rs:244-418`）：
- **取消先于安装会被记住**：late install 立刻 `unsubscribe`（`:314-325`）——保证"取消不会漏掉一个
  还在 subscribe 途中的上游"。`MergeAll` 正是给每个上游订阅前先建槽（`merge_all.rs:10-13`）。
- **清理在锁外跑**：`set`/`unsubscribe` 先改状态 `drop(state)` 再 `upstream.unsubscribe()`
  （`:120-159`）——避免回调里再入同一状态死锁/panic。
- **drop 句柄不取消**（`:385-392`）：取消只经显式 `unsubscribe`。
- 它还是 `Future`（`:177-199`）：Assigned→委托 poll，Cancelled→`Ready(())`，Pending→登记 waker。

## 三、两侧如何协作（一张图）

```
用户 subscribe(closure)
  → swap(FnMutObserver(closure))                     observable.rs:149
  → 逐算子 transform(...) 把 observer 洋葱套好        map.rs:121 / filter.rs:83
  → 源 into_inner() 取出洋葱、for item { if is_closed break; next(item) }  from_iter.rs:62-75
  → 结束 complete()；若 take 触发则 is_closed 提前 break（不再 complete）
返回的 Unsub：同步源=()；异步/多路=TupleSubscription/DynamicSubscriptions/SingleAssignment/…
```

**给使用者的判据**（呼应项目"状态放哪"的铁律）：
- 要一个跨 item 的可变状态 → 用 `scan_map`/`reduce`/`collect`，状态封在 observer 的 owned 字段里，
  绝不 `Arc<Mutex<业务态>>`。
- 要"一份产出喂多下游" → `Subject`/`publish().ref_count()`（见 `06`），不是自己缓存再广播。
- 要"处理错误" → 别指望 `subscribe(closure)`；用 `on_error`/`subscribe_with`/`into_future`，或把
  `Err` 变 `Infallible` 让错误随 `Item`（`Result`）走。

---
下一篇 `04-observable-trait-and-sources.md`：`factory.rs` 全部构造源逐个拆。
