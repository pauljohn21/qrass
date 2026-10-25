# 01 · 设计哲学与总体架构

本篇讲"rxrust 为什么长这样"。所有论断都对应到源码位置。

## 1. 它是 push-based（观察者拉不动，源往里推）

一个 Observable 不是"被迭代"的惰性集合，而是"一旦被 `subscribe`，源就主动把 item 推给
observer"。看最朴素的源 `FromIter`（`observable/from_iter.rs:54-76`）：

```rust
impl<C, Iter> CoreObservable<C> for FromIter<Iter>
where C: Context, C::Inner: Observer<Iter::Item, Infallible>, Iter: IntoIterator {
  type Unsub = ();
  fn subscribe(self, context: C) -> Self::Unsub {
    let mut observer = context.into_inner();      // 解包出下游 observer
    for item in self.iter {
      if observer.is_closed() { break; }          // 下游关了就早停（背压/取消）
      observer.next(item);
    }
    if !observer.is_closed() { observer.complete(); }
  }
}
```

要点：
- **`subscribe` 里同步跑完整个 `for`**：非时间类算子（`from_iter`/`map`/`filter`/`scan_map`…）
  在 `subscribe` 调用栈内一次性跑完（见 §5 执行模型）。所以 `subscribe` 之后再读一个普通
  `Vec`/channel 一定能拿到结果，不需要 async runtime。
- **取消靠 `is_closed()`**：`take(5)` 会把 observer 置为 closed，源在下一次 `next` 前 `break`。
  这是 pull 式背压的等价物，但由 push + `is_closed` 协作实现（`from_iter.rs` 测试
  `test_from_iter_with_take` 用无限 `0..` + `take(5)` 证明有效）。
- **`error`/`complete` 之后不再推**：它们是终结（下节）。

## 2. 三层契约：`ObservableType` → `CoreObservable` → `Observable`

架构注释在 `observable.rs:5-8`，实现分三块：

### (a) `ObservableType`（纯类型暴露，`observable.rs:103-108`）

```rust
pub trait ObservableType {
  type Item<'a> where Self: 'a;   // GAT：item 可依赖生命周期
  type Err;
}
```

它只回答"这条流推什么 Item、什么 Err"。GAT 是关键设计：`Item<'a>` 让 item 可以是
**借来的引用**（如 `&'a mut T`），支撑零拷贝与"把可变引用往下游传"的高级用法。多数拥有型源
让 `Item<'a>` 与 `'a` 无关（`FromIter::Item<'a> = Iter::Item`，`create.rs:23-29`）。

### (b) `CoreObservable<O>`（纯逻辑核，泛型于 observer 上下文，`observable.rs:115-124`）

```rust
pub trait CoreObservable<O>: ObservableType {
  type Unsub: Subscription;
  fn subscribe(self, observer: O) -> Self::Unsub;
}
```

这里的 `O` 是"包着 observer 的 Context"。同一个核逻辑对 `Local`/`Shared` 都成立，因为它对
`O` 泛型。源与算子都只实现这个 trait。

### (c) `Observable: Context`（面向用户 API，`observable.rs:130-2526`）

所有算子方法（`map`/`filter`/`flat_map`/…）定义在它上面，返回 `Self::With<Op<…>>`。它与
`CoreObservable` 通过一段**空布实现**缝合（`observable.rs:2541-2555`）：

```rust
impl<T> Observable for T
where T: Context, T::Inner: ObservableType {
  type Item<'a> = <T::Inner as ObservableType>::Item<'a> where T: 'a;
  type Err = <T::Inner as ObservableType>::Err;
}
```

即：**任何 Inner 实现了 `ObservableType` 的 Context 自动就是 Observable**，Item/Err 从内核推出。
用户永远只跟 `Observable`（`Local`/`Shared` 值）打交道，`CoreObservable`/`ObservableType` 是它的
机器面。这就是为什么文档/错误信息里看到的是干净的 `Item`/`Err`。

## 3. 订阅 = 把下游 observer 逐层"包"起来（算子的核心手法）

`map`（`ops/map.rs`）是范本。它有两个结构：

- `Map<S, F>{ source, func }`：这是**核数据**（实现 `ObservableType` + `CoreObservable`）。
- `MapObserver<O, F>{ observer, func }`：这是**observer 包装**（`ops/map.rs:44-62`）：

```rust
impl<O, F, Item, Out, Err> Observer<Item, Err> for MapObserver<O, F>
where O: Observer<Out, Err>, F: FnMut(Item) -> Out {
  fn next(&mut self, v: Item) { self.observer.next((self.func)(v)); }   // 变换后转发
  fn error(self, e) { self.observer.error(e); }                          // 其余全权委托
  fn complete(self) { self.observer.complete(); }
  fn is_closed(&self) -> bool { self.observer.is_closed(); }
}
```

`CoreObservable::subscribe`（`ops/map.rs:110-124`）做"包装 + 下传"：

```rust
fn subscribe(self, context: C) -> Self::Unsub {
  let Map { source, func } = self;
  let wrapped = context.transform(|observer| MapObserver { observer, func });
  source.subscribe(wrapped)         // 把"包了一层"的 context 交给上游 source
}
```

于是链 `src.map(f).filter(g).subscribe(h)` 的 `subscribe` 展开成：
`Src.subscribe( Ctx{ FilterObserver{ MapObserver{ FnMutObserver(h) } } } )`
——**observer 洋葱从下往上套，subscribe 从上往下钻到源**。`transform` 在换 observer 的同时
自动把 scheduler 带着走（见 `02`）。`filter`（`ops/filter.rs:80-85`）用同样的 `transform+下传`。

推论（很重要，用于避开项目"再造 stream 引擎"的坑）：**加算子 = 加一对
`Xxx{source,..}` + `XxxObserver{observer,..}`，绝不是在 rxrust 之上再写遍历/驱动。**

## 4. item 按值还是按引用：因算子而异（写代码前必查）

从签名一眼可辨（`observable.rs` 目录）：

| 算子族 | 闭包拿 item 的方式 | 出处 |
|--------|-------------------|------|
| `map` / `filter_map` / `flat_map` / `scan_map` / `reduce` / `tap`(mut) | **按值** `Item`（move 进来） | `map.rs:53` `F: FnMut(Item)->Out`；`scan_map.rs:63` `FnMut(&mut Acc, Item)` |
| `filter` | 谓词拿 `&Item`，observer 内部仍收 owned `Item` | `filter.rs:57` `F: FnMut(&Item)->bool`，`next(&mut self, v: Item)` |
| `distinct*` / `take_while` 等谓词 | 多拿 `&Item` | `observable.rs:229,283` |

`FnMutObserver`（用户 closure）签名是 `for<'a> FnMut(Self::Item<'a>)`——item 按 `Item<'a>`
交付，若该流的 `Item<'a>` 是借用型就能零拷贝。

## 5. 执行模型：哪些同步跑完、哪些要调度器

- **同步（`subscribe` 内跑完）**：`from_iter`/`of`/`create`/`empty`/`defer`/`from_fn` 这些非时间源，
  以及 `map`/`filter`/`scan_map`/`reduce_initial`/`collect`/`flat_map`/`merge`/`zip`/`tap`… 全部
  非时间组合子。依据：它们的 `CoreObservable::subscribe` 直接递归下传、无 `schedule` 调用
  （如 `from_iter.rs`、`map.rs`、`merge_all.rs` 的事件都同步穿过）。
- **异步（走 `Scheduler`）**：只有时间/调度类——`delay`/`timer`/`interval`/`buffer_time`/
  `debounce`/`sample`/`throttle`/`observe_on`/`subscribe_on`。它们用 `Self::Scheduler`
  （`scheduler.rs`，见 `07`）。

结论：纯同步管线**不需要 tokio runtime**；`subscribe` 后立刻 `channel.recv()` 就能取结果。
只有在用时间算子时才需要 runtime（`Local`→`spawn_local`，`Shared`→多线程）。

## 6. 冷流 vs 热流

- **冷（默认）**：每次 `subscribe` 都从头驱动一次源。`from_iter`/`of`/`create` 都冷（`subscribe`
  里 `for`/调闭包）。`defer` 每订阅现造子流（`factory.rs:494`）。
- **热**：`Subject`（`subject/subject_core.rs`）先于订阅存在，`next` 推给当前订阅者列表；
  `BehaviorSubject` 额外持当前值。把冷流转热共享：`publish()`→`ConnectableObservable`，
  `connect()`/`ref_count()`（`observable/connectable.rs`，见 `06`）。

## 7. 错误协议：`Err` 是类型参数，`error(self)` 是终结

`Observer::error(self, err)` 消耗 self（`observer.rs:27`），`Observable::subscribe(closure)` 只
接 `Err = Infallible`（`observable.rs:144-151` + `FnMutObserver` 忽略 error，`observer.rs:266`）。
可失败流的正规处理：
- 装一个自定义 `Observer`（`subscribe_with`，`observable.rs:157-163`），或
- 先 `.on_error(closure)` 消费错误把它变 `Infallible`（`ops/lifecycle.rs`），再 `subscribe`，或
- `.into_future()`（`ops/into_future.rs`）把"完成时的最后一个值 / 首个错误"变 `Future`。

项目里的做法（in-stream `Result` + `Err = Infallible`）就是把错误**当数据**放进 `Item`，
终点的 mpsc 收口再判——见 `08`。

## 8. 类型擦除：`box_it()`

链式算子产生巨长不可名状类型。`box_it()` 把核擦成
`LocalBoxedObservable<'a, Item, Err>`（底层 `BoxedCoreObservable =
Box<dyn DynCoreObservable<LocalCtx<BoxedObserver<..>, S>>>`，`observable/boxed.rs:86-87`）。
`S`（scheduler）在擦除中被保留（`boxed.rs:24-27`）。要能多次订阅用 `box_it_clone()`。跨函数
边界传流一律 `box_it`（见 `07`、`08`）。

---
下一篇 `02-context-scope-rc.md` 拆 `Local`/`Shared` 与所有权底座。
