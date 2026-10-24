# 06 · 多播：`Subject` / `BehaviorSubject` / `ConnectableObservable` / `ref_count`

"一份产出喂多个下游"是 rxrust 的**内置原语**（呼应项目铁律：一到多用多播，不自攒缓存再 `for`）。
本篇拆四种：冷 Subject 广播、值 vs 可变引用两种 observer、BehaviorSubject 带当前值、
Connectable 把冷流转热 + ref_count 自动连断。

## 1. `Subject<P>` 的指针化设计（`subject/subject_core.rs:172-339`）

```rust
pub struct Subject<P> { pub observers: P }   // P 指向 Subscribers<box observer>
```

`P` 由工厂按 Scope 选定（`subject.rs:44-59`）：
- Local 值广播：`SubjectPtr<'a,C,Item,Err> = C::RcMut<Subscribers<C::BoxedObserver<'a,Item,Err>>>`
  （`MutRc<Subscribers<Box<dyn DynObserver<..>>>>`）。
- Shared 值广播：`…BoxedObserverSend…`（`MutArc`，`+Send`）。
- 另有 `subject_mut_ref` / `behavior_subject_mut_ref` 用 `BoxedObserverMutRef`（广播 `&mut Item`）。

同一个 `Subject` 结构靠换 `P` 覆盖 Local/Shared + 值/可变引用四种组合（`subject.rs:20-27`），
零代码重复。

## 2. `Subscribers`：多播引擎（`subject/subscribers.rs`）

容器 `Subscribers{ inner: DynamicSubscriptions<Ob> }`（`subscribers.rs:19-21`），复用 `03` 的 id
化增删（≤2 免堆分配）。四种广播：

- `broadcast_value`（`:63-77`）：**n 个订阅者 → n-1 次 clone + 1 次 move**（最后一个直接拿
  owned 值）。故值广播要求 `Item: Clone`。
- `broadcast_mut_ref`（`:96-103`）：把同一个 `&mut T` 用 Rust **再借用**依次传给每个 observer，
  前一个的修改对后可见——**无需 `Clone`**，靠 HRTB `Ob: for<'a> Observer<&'a mut T, Err>`。
- `broadcast_error`（`:118-132`）：drain 全部、clone-til-last 发 `error`；之后列表清空。
- `broadcast_complete`（`:145-152`）：drain 全部逐个 `complete`；之后列表清空。

## 3. `Subject` 作为 `Observer`（往里推）（`subject_core.rs:411-505`）

值版（`Item: Clone, Err: Clone`）：

```rust
fn next(&mut self, value: Item) {
  if let Some(mut guard) = self.observers.try_rc_deref_mut() {   // 抢写借用
    guard.broadcast_value(value);
    return;
  }
  panic!("re-entrant Subject emissions are not supported (next/error/complete). \
          Use an explicit async boundary (e.g. delay(0)) if you need feedback loops.");
}
fn is_closed(&self) -> bool { self.observers.rc_deref().inner.is_empty() }
```

**再入策略（重要）**：
- `next/error/complete` **不可重入**——在自己回调里对同一 Subject 再发会走到"抢不到写借用"分支
  → **panic**。要反馈环就 `delay(0)` 造异步边界（下个调度 tick 再发，借用已释放）。
  （`subject.rs:62-104`、`subject_core.rs:419-450`）
- `is_closed` = "订阅者列表为空"（注意：这是 Subject 语义，与普通 observer 的"终结"不同）。

## 4. `Subject` 作为 `Observable`（`subject_core.rs:511-575`）

`subscribe(context)`：把下游 observer `into_inner().into_boxed()` 装箱，`guard.add(boxed)` 入列表，
返回 `SubjectSubscription{observers_ptr, state: RcCell<SubscriptionState>, scheduler}`；退订时按 id
`remove`。若此刻列表正被广播占用（`try_rc_deref_mut` 失败=在回调里订阅），则**不自作主张插**，
而是排一个 `Task<AddState>` 到 scheduler 延迟注册（`SubscriptionState{Pending,Ready(id),Cancelled}`）——
所以"广播中订阅"被安全推迟而非 panic。

## 5. `BehaviorSubject`（`subject/behavior_subject.rs`）

```rust
pub struct BehaviorSubject<ValuePtr, P> { pub subject: Subject<P>, pub value: ValuePtr }
```

= `Subject` + 一个共享当前值 `value`（`RcDerefMut<Target=Item>`）。`next` 先写 `value` 再转发
（`:85-89`）；新订阅者**立刻收到当前值**（`:117-120` 的 subscribe 先发 value）。适合表示"状态/
最新值"。附带 **`peek()`**（`:170,188,204`）：直接读当前值（`self.value.rc_deref().clone()`），
Context 包装后 `.inner().peek()` 可取。项目里"缓存某模块的最新产出并让多个 parent 立即拿到"
即用它 + `peek`。

## 6. `ConnectableObservable`（`observable/connectable.rs`）

冷流转热共享的桥：

```rust
pub struct ConnectableObservable<S, P> { source: S, subject: Subject<P> }
```

- 它**不是普通 Observable**：`subscribe`（`:92-100`）只把 observer 加进内部 subject，**不启动 source**。
- `fork()`（`:102-103`）→ `Subject<P>` 克隆，给每个下游一个可订阅句柄（共享同一连接）。
- `connect()`（`:105-110`）→ `source.subscribe(lift(subject))`：这才把源接到 subject、开始发光。
- 用法（`connectable.rs:16-32`）：先 `fork` 出 obs1/obs2 各自 subscribe，再 `connect()` 一次驱动，
  两个下游共享同一次上游执行。

## 7. `ref_count`（`ops/ref_count.rs`）＝自动 connect/disconnect

```rust
pub struct RefCount<S, P, C> { connectable, connection: C /* RcMut<slot> */ }
```

`RefCountSubscription`（`:122-142`）在退订时检查 subject 是否已空、空则断连。核心语义：
- **首个订阅者**到达即 `connect`（`connection` 槽从空变占用，`SourceConnection` 先占位再订阅，
  避免并发首订阅竞态；`:46`）。
- **末个退订**自动 disconnect。
- 源回调与取消**都在连接锁外**执行（`ref_count.rs:1-2`）。
- 便捷式：`publish().ref_count()`（`observable.rs:1927` 的 `publish()` = `multicast(Subject::default())`；
  `Connectable::ref_count` 见 `connectable.rs:144-150`）。

三者的取舍：
| 需要 | 用 |
|------|----|
| 自己掌控何时启动、手动 connect | `publish()` + `fork()` + `connect()` |
| 首订阅自动连、末退订自动断 | `publish().ref_count()` |
| 一个能随时 `next` 的热总线 | `Local::subject()` / `Shared::subject()` |
| 带"最新值"且新订阅者立即拿到 | `Local::behavior_subject(init)` + `peek()` |
| 避免并发首订阅竞态 | `ref_count` 已内置（`SourceConnection` 占位） |

## 8. 与项目铁律的衔接

- **一到多 = 上面这些，绝不是**"把产出 `collect` 进 `Vec`/缓存，再 `for_each` 挨个喂"。后者正是
  会话里反复出现的 GC 反模式；多播把它交给 `Subject`/`Connectable`。
- Subject 内部用 `Rc<RefCell>`/`Arc<Mutex>` 是**框架自身的多播机制**，不等于你可以把业务可变状态
  塞 `Arc<Mutex>`——业务 item 依旧按值 move 过算子（`02 §6`）。
- 值多播要求 `Item: Clone`；不想 clone 大结构就用 `*_mut_ref` 变体广播 `&mut Item`（`subscribers.rs:96`）。

---
下一篇 `07-type-erasure-and-scheduler.md`。
