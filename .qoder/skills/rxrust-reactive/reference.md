# rxrust 1.0.0-rc.6 参考（源码核验）

支撑 [SKILL.md](SKILL.md) 的深入 API 笔记。以下每条签名均从 `rxrust-1.0.0-rc.6` 源码
（observable.rs / context.rs / factory.rs / observer.rs / ops/* /
observable/boxed.rs / rc.rs / subject.rs）读出。完整逐文件走读与 do/don't 教义见
`docs/rxrust-source-analysis/`。若 rxrust 版本变动，须对源码重新核验。

**rc.5 → rc.6 差异（对源码核验）：** 核心模型不变 —— `Item<'a>` 仍是 GAT，`and_then`
仍缺位（用 `flat_map`/`concat_map`），`subscribe(closure)` 仍吞掉 `error`/`complete`，
非计时算子仍在 `subscribe` 时就地运行。**新增/改名**的是 `src/rc.rs`：共享可变原语被抽象到
`MutRc`/`MutArc`/`CellRc`/`CellArc` 之上的 `RcDeref`/`RcDerefMut`/`SharedCell` trait
（建立在 `rclite::{Rc, Arc}` crate 上，不是 `std`），`Subject`/`behavior_subject` 的载荷
现走这一层（见 §9 与新增的 §13）。新增 `collect_into(initial)`（§5.2）。

## 1. 架构：三层 trait + Context 包装

```text
ObservableType          仅类型：  type Item<'a> where Self:'a;  type Err;
  ▲
CoreObservable<O>       subscribe 内核：type Unsub: Subscription;
                        fn subscribe(self, observer: O) -> Self::Unsub;  （消耗链）
  ▲
Context                 执行包装：type Inner; type Scheduler: Clone+Default;
                        type With<T>;  lift / transform / swap / into_inner / scheduler
                        type BoxedCoreObservable<'a, Item, Err>;  （GAT 别名）
  ▲
Observable              用户 API —— blanket impl<T: Context where T::Inner: ObservableType>
                        map/filter/scan_map/flat_map/… 全挂在这里
```

- **`Item` 是 GAT**（`type Item<'a>`），不是关联类型。值流：`Item<'a> = T`；可变引用流：
  `Item<'a> = &'a mut T`。后果：**泛型函数不能约束 `O: Observable<Item = T>`**（E0107）。
  要么在一个函数内就地组装链，要么用 `.box_it()` 擦除（§7）。
- `Local<T> = LocalCtx<T, LocalScheduler>`、`Shared<T> = SharedCtx<T, SharedScheduler>`
  （context.rs L358/361）。`T` 是 Inner 核心 observable，**不是** Item。
- 用户链的类型 = `LocalCtx<深层嵌套核心类型>` —— 不可名状；跨函数边界必须 `.box_it()`。
- `Observable::subscribe(f)`（`f: for<'a> FnMut(Self::Item<'a>)`，observable.rs L141）
  会提升为 `FnMutObserver` —— **error/complete 是 no-op，被静默丢弃。**

## 2. Local 与 Shared 决策表

| 场景 | 选 | 约束 |
|---|---|---|
| 单线程、同步管线 | `Local` | 无需 Send/Sync |
| 真跨线程发射/订阅 | `Shared` | Item/Err/闭包 `Send(+Sync)` |
| 切换上下文 | `.into_shared()` / `.into_local()`（context.rs L450/462） | 只换包装；执行模型不变 |

**本编译器**的决策：同步管线默认 `Local`；`Shared` + 线程只用于真跨线程的工作
（依 OpenSpec 设计，不是框架规则）。没有并发工作处不要强上 `Shared` —— 它只增加
`Send + Sync + 'static` 约束。

## 3. 执行模型（源码语义）

- `create/of/from_iter/empty/never/throw_err` 及所有非计时算子**在 `subscribe` 时完全就地
  运行**。所以 `.subscribe()` 之后的 `rx.recv()` 总有值；无需 runtime。
- 只有计时源（`timer/interval`）与计时算子（`delay/buffer_time/debounce/sample/throttle`）
  调用 Scheduler（Local→`spawn_local`，需要 tokio）。
- `flat_map` 内部并发默认 `usize::MAX`（同步内层 == 串行）。严格保序 → `concat_map`
  （同实现，concurrent=1）。新到即弃旧内层 → `switch_map`。

## 4. 工厂（`ObservableFactory`，在 `Local`/`Shared` 上 blanket 提供）

```text
Local::create<Item, Err, F, U>(f)   F: FnOnce(&mut dyn Emitter<Item,Err>) -> U, U: Subscription
Local::of(v)                        单值 + complete
Local::empty()                      立即 complete
Local::never()                      永不发射（用 take/drop 控制）
Local::throw_err(e)                 立即 error
Local::from_iter(iter)              I: IntoIterator，同步发射每项
Local::from_fn(f)                   每次订阅调 f 产一个 Item
Local::defer(f)                     每次订阅重建链：F: Fn() -> O(Context)
Local::subject::<Item,Err>()        多播（§9）
Local::subject_mut_ref::<Item,Err>() 多播、可变引用分发
Local::behavior_subject(initial)    缓存最新值的多播（Item: Clone）
Local::from_future(fut)             异步单值源（需要 runtime）
Local::from_future_result(fut)      Future<Output=Result<Item,Err>> → 值/错误分流
Local::from_stream(st)              Stream → 多值异步源
Local::timer(d)/timer_at(i)         计时单次发射（需要 runtime）
Local::interval(period)             计时多次发射（需要 runtime）
Local::merge_observables(vec)       并许多同型流（并发）
Local::concat_observables(vec)      顺序订阅多个（保序）
```

`create` 闭包参数是 **trait object** `&mut dyn Emitter<Item, Err>`
（`rxrust::observer::Emitter`，**不在 prelude** —— 显式导入）。`Emitter` 有
`next(&mut self, Item)`、`error(&mut self, Err)`、`complete(&mut self)` —— 全是
`&mut self`（不同于 `Observer`，其 error/complete 消耗 `self`）。返回 `U: Subscription`
—— 同步源可返回 `()`。推断定不住 Item/Err 时用 turbofish `::<Item, Err, _, _>`
（如 empty 或仅错误的流）。需要清理时返回 `ClosureSubscription(move || { /*teardown*/ })`。

## 5. 算子目录（签名已核验 —— 通篇 HRTB `for<'a> FnMut`）

### 5.1 变换
```text
map(F)          F: for<'a> FnMut(Item<'a>) -> Out                 值进、值出（移动）
map_to(v)       v: Clone                                          每次发射恒定
map_err(F)      映射错误通道（不吞）
filter(F)       F: for<'a> FnMut(&Item<'a>) -> bool               ★ 借 &Item
filter_map(F)   F: for<'a> FnMut(Item<'a>) -> Option<Out>         ★ 值进、Option 出
tap(F)          F: for<'a> FnMut(&Item<'a>)                       ★ 只读副作用
```

### 5.2 有状态折叠（对所有权友好 —— 编译器的核心武器）
```text
scan(seed, F)            Output：Clone 必需；F: FnMut(Output, Item) -> Output（每项都发射）
scan_map(seed, F)        F: for<'a> FnMut(&mut Acc, Item<'a>) -> Output   ★ Acc 无需 Clone；逐项发射 Output
reduce_initial(Out, F)   F: for<'a> FnMut(Out, Item<'a>) -> Out           ★ complete 时发最终 Out 一次
reduce(F)                F: FnMut(Item<'a>, Item<'a>) -> Item<'a>         无 seed；空流不发射
collect::<C>()           C: Default + Extend<Item>；complete 后发一个 C  ★ 单一终点项
collect_into(col)        rc.6：把各项折进一个预置的 owned 集合（无需 C: Default）★ 发 col
last()                   只发最终值
pairwise()               发 (prev, cur) 元组
```
规则：需要**逐项中间状态** → `scan_map`；需要**一个最终值** → `reduce_initial`/`collect`；
Acc 非 Clone → 必须用 `scan_map`/`reduce_initial`。

### 5.3 阶段连接（高阶 —— 编译管线如何衔接）
```text
flat_map(F)      F: for<'a> FnMut(Item<'a>) -> Inner;  Inner: Context<Inner: ObservableType<Err = Self::Err>>
                 上游每次 next → F 产一条新流、被订阅，其 next 流向下游
concat_map(F)    同 flat_map 但保序（内部并发 = 1）
switch_map(F)    F: for<'a> FnMut(Item<'a>) -> Out; Out: Context<Inner: ObservableType<Err=Self::Err> + 'static>; 取消旧内层
merge_all(n)     压平流中之流（上游发 Observables）
```
`flat_map` 的内层 `Err` 必须等于外层 `Err` —— **两个分支（正常阶段 / 错误短路）必须
同 Item 同 Err。** 这正是阶段输出统一为 `Result<T, E>` 数据 Item、`Err = Infallible`
的原因（§6、§8）。**`and_then` 在 rc.5 被删** —— 等价物是 `flat_map`/`concat_map`
（对同步源语义相同）。

### 5.4 截断 / 生命周期
```text
take(n)/take_last(n)/skip(n)/skip_while(F)/skip_until(N)/skip_last(n)
take_while(F)             F: FnMut(&Item)->bool；   take_while_inclusive 保留触发项
take_until(N)             N: Observable<Err = Self::Err>   N 上首个 next/error 截断
default_if_empty(v)       流空时兜底
finalize(F)               F: FnOnce()   任何终点路径都跑（complete/error/cancel）★ 适合 span 收尾
on_error(F)               F: FnOnce(Self::Err)   ★ 错误旁路，流不变
on_complete(F)            F: FnOnce()
retry(P)                  P: RetryPolicy<Err>: Clone { fn should_retry(&Err, attempt)->Option<Duration> }
```

### 5.5 组合
```text
zip(B)                 B: Observable<Err=Self::Err>；发 (A,B) 对
with_latest_from(S2)   S2: Observable<Err=Self::Err>；A 是时钟，取 B 的最新值
combine_latest(S2, F)  F: for<'a> FnMut(A::Item<'a>, B::Item<'a>) -> Out
merge(S2)              两条流；S2 的 Item<'a> 必须与 Self 精确匹配
```

### 5.6 聚合 / 缓冲 / 去重
```text
buffer_count(n)        每 n 项发一个 Vec
average(Item: Averageable), count, max, min    数值聚合
distinct / distinct_until_changed
group_by(...)          分组多播
contains(v) / start_with(Vec)
```

## 6. 终点 —— 框架自己的形状

```text
(a) subscribe(closure)      只 next。值通道已编码 Result 错误时没问题。
(b) subscribe_with(Observer) 实现 Observer（next=&mut self；error=self；complete=self；is_closed），
                            或把闭包包成 FnMutObserver。必须看到 error/complete 时用。
(c) into_future()           Output = Result<Result<T,E>, IntoFutureError>。rc.6 核验：
                            恰一个值 -> Ok(Ok(v))；error -> Ok(Err(e))；
                            零值 -> Err(Empty)；多于一个值 -> Err(MultipleValues)。
                            构造即订阅，所以非计时链立刻 Ready
                            （任何 block_on 都行；无需 LocalSet）。它 DROP 订阅句柄
                            （into_future.rs:259）=> 不可取消。
(d) await 句柄             from_stream / from_future / timer / observe_on / subscribe_on 返回
                            TaskHandle，其本身是 Future（scheduler.rs:429）：
                            `let h = obs.subscribe(f); h.await;` 即框架的「等终点」。
(e) into_stream()           Stream<Item = Result<Item, Err>>；每次轮询一个值。
                            只在 Drop 时取消（into_stream.rs:123-129）—— drop 它即提前停止。
```

**同步终点（crate 自带测试的做法）：** 非计时算子在 `subscribe` 时完全就地运行，所以
`subscribe` 一返回结果即就绪 —— 经闭包写入本地共享 cell，无需通道：

```rust
use std::{cell::RefCell, rc::Rc};
let out = Rc::new(RefCell::new(Vec::<String>::new()));
let sink = out.clone();
stream
    .map(final_extract)                            // Item<'static> = String
    .collect::<Vec<String>>()                       // 只在 complete 时发一次
    .subscribe(move |css| { *sink.borrow_mut() = css; });   // 就地：返回即完成
```

`std::sync::mpsc` **不是**框架终点：它是套在响应式核心外的 std 壳。只用它桥接到必须跨
线程递 `Sender` 的 API，绝不作编译器自身的收口。

错误协议：**编译器错误绝不进入 observable 的 `Err` 通道**（它是 `Infallible`）。它们作为
in-stream `Result` 数据流向终点，绕开 `subscribe(closure)` 吞错误的问题（§1）。想*额外*
在旁路观察错误，用 `.on_error(move |e| …)`（注意：`on_error` **消耗**错误并把流的 `Err`
变成 `Infallible` —— lifecycle.rs:31-40,56）。

**Shared / 异步入口：`into_future().await`，或 `await` 订阅句柄。** 当源是
`Shared::from_stream(..)`（它会调度一个任务，所以 `subscribe` 返回一个可 await 的句柄、
不是就地运行），用框架自己的 `into_future()` 由调用方 runtime 驱动收口。
`reduce` = `FnMut(Item,Item)->Item`（无 seed，空源不发射）；要折进不同累加器类型用
`reduce_initial(seed, FnMut(Acc,Item)->Acc)`（空源发射 seed）。

```rust
// Shared + from_stream，终点用 rxrust 自己的 into_future（无 std channel）：
Shared::from_stream(futures::stream::iter(items))
    .reduce_initial(String::new(), |mut acc, chunk| { acc.push_str(&chunk); acc })
    .into_future()
    .await
```

**排序陷阱（已核验）：** `Shared::from_stream` 保序（一个任务顺序轮询流），但
`flat_map(|item| Shared::from_stream(..))` 会**每项派生一个独立任务** → 发射交错、文档序
丢失。要逐项扇出工作又保序，把逐步状态（env / 一个 `@if` 链旗标）作为 owned `scan_map`
累加器穿过**一条**有序源，再 `flat_map(Shared::from_iter(..))`（`from_iter` 就地且有序）
压平。跨兄弟状态是*移动累加器值*，不是逐项独立的。

`subscribe_with` 自定义 sink（当错误真的走 Err 通道时，如 throw_err）—— 下面的 sink 只是
*桥接*情形（调用方已持有通道）；响应式核心内部优先 `collect`/`reduce_initial` +
`into_future()`（§6）：
```rust
use rxrust::observer::Observer;
struct Sink<T, E>(std::sync::mpsc::Sender<Result<T, E>>);
impl<T, E> Observer<T, E> for Sink<T, E> {
    fn next(&mut self, v: T) { self.0.send(Ok(v)).ok(); }
    fn error(self, e: E)     { self.0.send(Err(e)).ok(); }
    fn complete(self) {}
    fn is_closed(&self) -> bool { false }
}
```

## 7. 类型擦除：`box_it` 家族（observable/boxed.rs）

跨函数边界传「一条流」的唯一合法方式：

```text
.box_it()          → LocalBoxedObservable<'a, Item, Err = ()>
                       = Local<Box<dyn DynCoreObservable<
                           LocalCtx<BoxedObserver<'a, Item, Err>, LocalScheduler>> + 'a>>
.box_it_clone()    → LocalBoxedObservableClone<…>  （可重复订阅的擦除流）
.box_it_mut_ref()  → Item = &'a mut T 分发
Shared 侧：         SharedBoxedObservable<…>  （要求 Send）
```

别名需 `feature="scheduler"`（默认开），且在 prelude 再导出。`LocalBoxedObservable`
自带其 `Local` 上下文，所以可在其上续算子链、作 `flat_map` 内层、或以 `subscribe`
收尾。Item/Err 都 owned 时选 `'static`；显式写 `Err = Infallible`（`()` 默认值与
以 `Infallible` 为型的 `create` 源不匹配）。

反模式：`fn stage<O: Observable<Item = T, Err = E>>(…)` → E0107（GAT）。box_it 是出口。

## 8. 直接用算子组合 —— 不要框架层

不要引入 `type Stage<T>` / `fn fail<T>` / `fn load_stage` / `fn drain(...)`。那是自造
框架层（`rust.md`：禁自造框架层 `Stage<T>`/`fail`/`drain`/`drain_one`），把 rxrust
掰成跑*你的*控制流抽象而非使用它的算子。直接在源上组合**一条**冷算子链，以单个终点
`subscribe` 收尾。逐项阶段*函数*（取 owned 值、返回 observable）没问题，fn 边界上的
`box_it()` 也没问题 —— 禁令针对 `Stage<T>`/`fail`/`drain` 框架*类型*与任何
手写状态机（`from_fn` + `VecDeque`，或*驱动*遍历的共享可变 `Acc`）。

```rust
use rxrust::prelude::*;

// 一条穿过 owned 值的冷链；无 Stage/fail/drain 层、无共享可变驱动器。
// 循环（`@each`）即 `flat_map`；分支（`@if`）即 `filter`；单一终点收口。
// 本项目的入口是 async-only，所以 `into_future()` 是自然的收尾：
pub async fn compile(source: String) -> String {
    lexer::token_stream(source)                       // chars -> owned Tok，下游移动
        .flat_map(|toks| parse_eval_render(toks))     // owned 值上的算子
        .collect::<Vec<String>>()                     // complete 时发一次容器
        .reduce_initial(String::new(), |mut acc, chunk| { acc.push_str(&chunk); acc })
        .into_future()
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

// 循环字面就是 `flat_map`，就地写 —— 无包装、无「先缓冲再重放」：
//   Shared::from_iter(items).flat_map(|item| body_stream(item)).box_it()
```

用 `tracing::span!` 为每个阶段计时，在执行闭包内记录 elapsed（§11）。

## 9. Subject 多播（subject.rs）

```rust
let subject = Local::subject::<i32, Infallible>();
subject.clone().next(1);                       // Subject 自身实现 Emitter
subject.clone().subscribe(|v| /* … */);        // 每个订阅者共享内部 Rc
subject.complete();
```

- `Subject` 是**框架内部的多播管道**，不是业务状态容器。只要一个产者的输出需同时喂
  多个消者就用它 —— 如模块编译一次、扇出到每个 `@use`，或 `@mixin` 定义被多个
  `@include` 消费。rc.6 中 subject 载荷是 `rc.rs` 共享句柄上的 `SubjectPtr<…>`：
  `Local::subject::<Item,Err>()` 用 `MutRc`（`Rc<RefCell<…>>`），`Shared` 形态用
  `MutArc`（`Arc<Mutex<…>>`）；`behavior_subject` 要求 `Item: Clone`。这是框架自己的
  共享 —— **不是**允许把业务数据（tokens/AST/累加器）包进 `Rc<RefCell>`/`Mutex`；那些
  保持为经 `scan_map`/`reduce_initial` 移动的 owned 值（§11）。
- `behavior_subject(initial)` 缓存最新值（Item: Clone）。
- `Connectable` / `ref_count` / `fork`（connectable.rs）用于延迟多播接线；需要时再读。

## 10. RxJS / rxrust 0.15 差异（防旧习惯）

| 旧习惯 | rc.6 现实 |
|---|---|
| `pipe(op1, op2)` | 无 pipe；直接链式调方法 |
| `and_then` 算子 | **已删。** 等价：`flat_map` / `concat_map`（同步源相同） |
| 0.15 `Arc<Mutex<Option<T>>>` + `try_unwrap` 排空 | mpsc 通道或 `into_future` |
| 0.15 `Observable<Item = T>` 关联类型 | GAT `Item<'a>`；泛型约束语法完全不同 |
| `share()` / `publish()` | `Subject` / `ref_count()` / `Connectable` |
| 0.15 三闭包 `Observer` 构造器 | `Observer` trait（next=&mut、error/complete=self）；便利经 `subscribe_with(FnMutObserver(..))` 或自行实现 |
| 自由 `zip(a, b)` | 实例 `a.zip(b)` / 工厂 `merge_observables(Vec)` / `concat_observables(Vec)` |
| JS `switchMap` 取消语义 | 同名 `switch_map` ✓ 存在 |

## 11. 所有权红线（项目规则 —— 违反 = 任务失败）

1. **不**对业务数据（token 流、AST、源码、累加器）用 `Arc<Mutex<_>>`。按值移动：
   `scan_map(&mut Acc, item)` / `reduce_initial(owned_acc, …)` / `collect` 后移走整包。
2. **不**用 `println!`/`eprintln!`；跨阶段用 `tracing::span!`（stage/module/elapsed_ms），
   计时记在**执行闭包内**（§8），或用 `finalize` 收尾。
3. 闭包捕获 owned 值 + `move`；一次性消费用 `Option<T>::take()` 满足 `FnMut`。
4. `src/` 内不写内联测试；测试全在 `tests/*.rs`；单文件 ≤ 500 行。
5. 错误协议：编译器错误（SassError）**永远作为 in-stream `Result` 数据流动**；`Err` 通道
   保持 `Infallible`；`on_error` 只用于旁路监听。

## 12. 常见编译错误 → 处置

| 症状 | 根因 | 修法 |
|---|---|---|
| E0107 on `Observable<Item = T>` | Item 是 GAT | 在一个 fn 内就地组装，或返回 `LocalBoxedObservable` 别名 |
| `tx.send` 处 `expected String, found &str` | 通道类型被 map 闭包遮蔽 | 标注 `mpsc::channel::<Result<T, E>>()` |
| flat_map 分支类型不匹配 | 内层流必须 Item/Err 一致 | 两分支都 `.box_it()` 到同一擦除流类型 |
| map 闭包中 `move out of captured value` | map 是 `FnMut`、被调多次 | `Option::take()` 一次取出 owned 值 |
| create 闭包中找不到 `Emitter` | 不在 prelude | `use rxrust::observer::Emitter;` |
| subscribe 后 `rx.recv()` 无值 | 上游用了计时算子/异步源 | 弃用计时算子或进入 runtime；同步源永不落空 |
| `into_shared` 的 Send 不满足 | Shared 要求整条链 Send+Sync+'static | 除非真需要跨线程执行，否则用 `Local` |
| 想在 `filter` 里改值 | filter 收 `&Item` | 值语义用 `map`/`filter_map` |
| `subscribe` 吞掉错误 | `FnMutObserver` 丢弃 error/complete | in-stream `Result` 协议 / `subscribe_with` / `on_error` |

## 13. 共享状态原语（`rc.rs`，rc.6 新增）

rxrust 把自己的共享可变记账抽象在三个 trait 之后（全建在 `rclite::{Rc, Arc}` 上，
不是 `std::rc`/`std::sync`）：

```text
RcDeref: Clone      { type Target; type Ref<'a>: Deref; fn rc_deref(&self) -> Ref<'_>; }
RcDerefMut: RcDeref { type MutRef<'a>: DerefMut; fn rc_deref_mut(&self);
                      fn try_rc_deref_mut(&self) -> Option<MutRef<'_>>; }  // 重入时 None
SharedCell<T: Copy+Eq>: Clone+From<T> { get; set; compare_exchange; }

MutRc<T>   = Rc<RefCell<T>>    Local 共享可变状态
MutArc<T>  = Arc<Mutex<T>>     Shared（Send）共享可变状态
CellRc<T>/CellArc<T> = Rc<Cell<T>>/Arc<AtomicCell<T>>   Copy 标志/计数器（比 RefCell 便宜）
```

`try_rc_deref_mut` 返回 `None` 是框架检测同步重入、经 scheduler 延后而不是 panic/死锁的
方式。**这些是框架内部件**（subject 扇出、`Connectable`、内部计数器）。编译器的业务数据
必须保持只可移动：如果你伸手拿 `MutRc`/`MutArc`/`Subject` 去装 token 向量或累加器，
那就是 R2/§11 禁止的 GC/共享状态味 —— 改以 `scan_map`/`reduce_initial` 的 owned 累加器携带。

## Subscription 与 RAII

`Subscription::unsubscribe(self)` 消耗自身；`()` 实现 `Subscription`（永远 closed）——
这就是 `create` 能返回 `()` 的原因。`unsubscribe_when_dropped()` 返回一个
`#[must_use] SubscriptionGuard`，于作用域退出时退订 —— 不绑定守卫即立即退订。
