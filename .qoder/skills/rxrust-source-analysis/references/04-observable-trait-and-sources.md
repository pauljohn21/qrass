# 04 · Observable 三 trait 与所有构造源（factory）

## 一、`Observable` 上的订阅入口（`observable.rs:139-163`）

```rust
fn subscribe<F, U>(self, f: F) -> U
where F: for<'a> FnMut(Self::Item<'a>),
      Self::Inner: CoreObservable<Self::With<FnMutObserver<F>>, Unsub = U>
{ let (source, wrapped) = self.swap(FnMutObserver(f)); source.subscribe(wrapped) }

fn subscribe_with<O>(self, observer: O) -> <Self::Inner as CoreObservable<Self::With<O>>>::Unsub
where Self::Inner: CoreObservable<Self::With<O>>
{ let (core, wrapped) = self.swap(observer); core.subscribe(wrapped) }
```

- `subscribe(closure)`：仅当 `Item<'a>` 能被 `FnMut` 吃掉；因 `FnMutObserver` 只实现
  `Err = Infallible`（`03 §一.3`），**可失败链不能直接 `subscribe` closure**。
- `subscribe_with(observer)`：给自定义 `Observer`（含 `Subject`）用，能收 `error`/`complete`。
- 二者都靠 `swap`：把"当前 context 里的核 observable"和"要下传的 observer"对调，然后让核
  `subscribe` 那个装了 observer 的新 context。

`Self::With<Op<…>>` 里的 `With` = `Local`/`Shared` 同族换 inner（`context.rs:88`），故算子返回
仍是 `Observable`，链式得以继续。

## 二、`factory.rs`：`ObservableFactory`（`factory.rs:121-1064`）

设计：`trait ObservableFactory: Context<Inner = ()>` + 空布 `impl<C: Context<Inner=()>>
ObservableFactory for C {}`（`factory.rs:1068`）。所以 **`Local`/`Shared`（Inner=`()` 的 marker）
当工厂**，`Local::of(..)`、`Shared::from_iter(..)` 由此而来；每个工厂方法 `Self::lift(Source)`
产出 `Self::With<Source>`（即 `Local<Source>`）。`lift` 注入默认 scheduler（`context.rs:282`）。

### 逐个源（含约束与语义）

| 工厂方法 | 产出的核 | Item / Err | 关键约束与语义 | 出处 |
|----------|----------|-----------|----------------|------|
| `of(v)` | `Of<V>` | `Item=V`, `Err=Infallible` | 发一个值即 complete | `factory.rs:165`；`observable/of.rs` |
| `from_iter(iter)` | `FromIter<I>` | `Item=I::Item`, `Err=Infallible` | `IntoIterator`，订阅时同步逐个发；`is_closed` 早停 | `factory.rs:394`；`from_iter.rs` |
| `from_fn(f)` | `FromFn<F>` | 发一个**函数生成的值** | `F: Fn()->Item`，订阅时求值 | `factory.rs:441`；`observable/from_fn.rs` |
| `defer(f)` | `Defer<F,O>` | 转发内层 `O` | `F: FnOnce()->Self::With<O>`，**每订阅现造一条子流**（冷、可动态） | `factory.rs:494`；`observable/defer.rs` |
| `create(f)` | `Create<F,Item,Err>` | 用户指定 | `F: FnOnce(&mut dyn Emitter<Item,Err>)->U`，`U: Subscription`；自定义发射+teardown，零堆装箱 | `factory.rs:147`；`create.rs` |
| `empty()` | `Empty` | 立即 complete 不发光 | 空/完成信号/条件链 base case | `factory.rs:196`；`observable/trivial.rs` |
| `never()` | `Never` | 永不发光永不 complete | 需 `take`/手动取消，否则挂住 | `factory.rs:228` |
| `throw_err(e)` | `ThrowErr<E>` | 立即 error | 失败条件建模 | `factory.rs:267` |
| `timer(d)` / `timer_at` / `_with` | `Timer<S>` | `Item=()`，延迟后发一次 | **时间源**，走 scheduler | `factory.rs:524-563`；`observable/timer.rs` |
| `interval(p)` / `_with` | `Interval<S>` | `Item=usize` 递增 | **时间源**，周期发直到取消；`Shared` 才能配 `take`（Rc 态非 Send） | `factory.rs:584-617`；`observable/interval.rs` |
| `from_future(fut)` / `_result` / `_with` | `FromFuture`/`FromFutureResult` | `Item=fut::Output`；result 版 `Ok→next` `Err→error` | 异步→流；走 scheduler | `factory.rs:650-773`；`observable/from_future.rs` |
| `from_stream(st)` / `_result` / `_with` | `FromStream`/`FromStreamResult` | `Item=st::Item`；result 版 `Err→error` 终止 | `futures_core::Stream` 逐 poll；`Ready(None)`→complete | `factory.rs:807-929`；`observable/from_stream.rs` |
| `subject()` / `subject_mut_ref()` | `Subject<SubjectPtr>` | 多播（热） | 见 `06` | `factory.rs:283-303` |
| `behavior_subject(init)` / `_mut_ref` | `BehaviorSubject<RcMut<Item>,…>` | 带当前值的多播 | 见 `06` | `factory.rs:324-355` |
| `merge_observables(it)` | `MergeAll<FromIter<…>>{concurrent:MAX}` | 并发订阅全部、按时序交错 | 完成=全部完成 | `factory.rs:984` |
| `concat_observables(it)` | `MergeAll<FromIter<…>>{concurrent:1}` | 逐个顺序订阅、保源序 | | `factory.rs:1051` |

### `create` 深读（自定义源的正解）

`create` 是"把任意产生逻辑接成 Observable"的正规入口（`create.rs:60-73`）：

```rust
impl<C,F,Item,Err,U> CoreObservable<C> for Create<F,Item,Err>
where C: Context, C::Inner: Observer<Item,Err>,
      F: FnOnce(&mut dyn Emitter<Item,Err>) -> U, U: Subscription {
  type Unsub = U;
  fn subscribe(self, context: C) -> U {
    let observer = context.into_inner();
    let mut emitter = CreateEmitter(Some(observer));
    (self.f)(&mut emitter)     // 闭包负责 next/error/complete + 返回 teardown
  }
}
```

用法要点：
- 闭包收到的 `&mut dyn Emitter` 会往下驱动**整条已包好的 observer 洋葱**，你无需知道下游类型。
- 返回 `Subscription`（teardown）：`()`（无清理）、`ClosureSubscription(closure)`、或某个
  handle。`test_create_teardown`（`create.rs:112-126`）演示返回 `ClosureSubscription` 后由外层
  `unsubscribe()` 触发。
- `Local::create::<Item, Err, _, _>(…)` 常需**显式标注 `Item/Err`**（闭包参数类型推断不全），
  这与技能手册"最小源形状"一致。

## 三、源 vs 算子的分界（读代码时抓主线）

- **源**（`observable/*` + `factory.rs`）：实现 `CoreObservable` 的 `subscribe` 是"**主动生产并
  推给 observer**"（`from_iter` 的 `for`、`create` 的闭包、`timer`/`interval` 排 `Task`）。
- **算子**（`ops/*`）：实现 `CoreObservable` 的 `subscribe` 是"**用 `transform` 把 observer 包一层
  再下传给 source**"（`map.rs:118`、`filter.rs:80`、`scan_map.rs:85`、`reduce.rs:96`…）；算子
  自己不生产，只改写穿过的事件。
- 两者都实现 `ObservableType` 来**声明 Item/Err 如何随类型流动**（GAT `Item<'a>`）。

## 四、wasm32 / !Send 相关提醒（项目当前目标）

- `Local` 一切皆 `Rc`-based、无 `Send` 约束，契合 wasm32；`interval_with(SharedScheduler)` 配
  `Local` 再 `take` 会编译失败（`factory.rs:595-599`），因为 `take` 的 Rc 态不 `Send`。写 wasm
  管线就统一 `Local` + `LocalScheduler`，别混。
- 真要并发（模块编排）才上 `Shared`，且业务 `Item/Err`/闭包全需 `Send + Sync + 'static`
  （见 `08` 的取舍；但注意项目铁律——共享的应是冻结的 `Arc<不可变>`，不是 `Arc<Mutex<业务态>>`）。

---
下一篇 `05-operators.md`：约 70 个算子按类逐一拆（约束/传递/错误传播/并发）。
