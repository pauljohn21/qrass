# rxrust 1.0 AI 编码手册（官方 Book + 源码核实版）

> 依据：rxRust 官方 Book（`https://rxrust.github.io/rxRust/latest/`，源码在 crate 的
> `guide/` 目录）逐章精读 + `rxrust-1.0.0-rc.5` 源码逐文件核实（`context.rs` /
> `factory.rs` / `observable.rs` / `observer.rs` / `ops/*` / `observable/boxed.rs` /
> `subject/*` / `subscription*`）。所有签名从源码取证，非凭记忆。
> 供 AI 在本项目（qrass）编写/审查响应式代码时直接遵循。
> 版本钉死：`rxrust = "=1.0.0-rc.5"`（裸 `cargo add` 会解析到旧 0.15 线）。
> 保留 default features（`scheduler` feature 提供 `LocalScheduler`/`SharedScheduler`
> 与 `LocalBoxedObservable` 等别名，且已 re-export 进 prelude）。

---

## 0. 心智模型：Push vs Pull，三件套

`Iterator` / `Future` / `Stream` 是 **Pull** 模型——消费者主动索取下一值。
`Observable` 是 **Push** 模型——生产者把值推给消费者（点击、消息、定时器、逐项 token 发射）。
编译器管道本质是"源把产物推过一串算子、直到终端"，因此选 Push 组合。

rxRust 程序围绕三件套：

1. **Observable（源）**：随时间的惰性计算。**没订阅就什么都不发生**；调用算子时
   原 Observable 被 **move**（消费）进新 Observable（Rust 所有权特有）。
2. **Operators（管道）**：`map`/`filter`/`scan_map`/`merge`… 纯函数，把一条流变形为另一条。
3. **Subscription（连接）**：`subscribe` 建立源↔消费者的连接并**启动执行**，返回可取消句柄。

> "ReactiveX = Observer 模式 + Iterator 模式 + 函数式编程的最佳结合。"

---

## 1. 统一架构：三层 trait + Context 环境载体（源码事实）

rxRust v1.0 的核心哲学是 **Unified Logic, Adaptive Context**：算子逻辑只写一遍、
与环境无关，由 `Context` 在编译期注入执行环境（Local/Shared）。三个基础 trait：

```text
ObservableType          纯类型暴露(声明发射什么)：
                          type Item<'a> where Self:'a;   ← GAT，不是关联类型！
                          type Err;
  ▲
CoreObservable<O>       订阅内核(Publisher)：
                          type Unsub: Subscription;
                          fn subscribe(self, observer: O) -> Self::Unsub;
  ▲
Context                 执行环境载体(依赖注入)：
                          type Scope: Scope;
                          type Inner;                              ← Inner 才是 core observable
                          type Scheduler: Clone + Default;
                          type RcMut<T>:  From<T>+Clone+RcDerefMut<Target=T>;   // 可变共享原语
                          type RcCell<T: Copy+Eq>: SharedCell<T>;                // Copy 状态原语
                          type BoxedSubscription / BoxedObserver<'a,Item,Err>
                          type BoxedCoreObservable<'a,Item,Err> (+MutRef/Clone 变体)
                          type With<T>: Context<Inner=T, Scope=Self::Scope,
                                               Scheduler=Self::Scheduler>;        // 类型传播
                          fn from_parts/new/lift/transform/swap/into_inner/scheduler
  ▲
Observable              用户 API：blanket impl
                          (T: Context where T::Inner: ObservableType)
                          map/filter/scan_map/flat_map/... 全部挂在这里
```

**要点（务必内化）：**

- **`Item` 是 GAT** `type Item<'a>`（`observable.rs` L131）。值流 `Item<'a> = T`；
  mut-ref 流 `Item<'a> = &'a mut T`。后果：**泛型函数无法直接约束
  `Obs: Observable<Item = T>`（E0107）**，只能写 `for<'a>` HRTB 闭包边界，或
  在单函数内联组装 / 返回 `box_it` 擦除别名（§7）。
- **`Self::With<T>` 是类型传播的引擎**：每上一个算子产出新 Context 但保留 `Scheduler`
  与 `Scope`，所以链条第 10 级的 `delay` 仍能拿到正确调度器，全程零显式传参、零运行时开销
  （编译期依赖注入）。
- 类型别名（`context.rs` L358/361）：
  `Local<T> = LocalCtx<T, LocalScheduler>`，`Shared<T> = SharedCtx<T, SharedScheduler>`。
  **`T` 是 Inner（core observable），不是 Item。**
- 用户写的每条算子链类型 = `LocalCtx<深层嵌套核心类型>`（如
  `Local<Map<Filter<FromIter<..>,..>,..>>`），**不可命名**，跨函数边界必须 `.box_it()`。
- **`Observable::subscribe(f)`（L141）要求 `Err = Infallible`**：闭包被包进
  `FnMutObserver<F>: Observer<Item, Infallible>`，故**只有错误通道为 `Infallible` 的流
  才能用闭包 subscribe**。这不是"静默吞错"，而是编译期约束——它正是 qrass 把编译器错误
  统一为**流内 `Result` 数据项 + `Err = Infallible`** 协议的根因（§6/§8）。
  需要处理非 `Infallible` 的 error/complete，用 `subscribe_with(Observer)`（§6）。

---

## 2. Context：Local vs Shared（Book《Context》章）

`Context` 回答三个编译期/运行期问题：**内存策略**（`Rc` vs `Arc`）、**调度**（主循环 vs
线程池）、**线程安全**（`!Send` vs `Send+Sync`）。两个内置 Context：

| Context | 环境 | 共享原语 | 调度器 | 约束 |
|---|---|---|---|---|
| `Local` | 单线程（主线程 / WASM / 嵌入式） | `Rc<RefCell<T>>` | `LocalScheduler`（当前线程 loop） | `!Send`，编译器阻止跨线程 |
| `Shared` | 多线程（后台 worker / 服务器） | `Arc<Mutex<T>>` | `SharedScheduler`（线程池） | `Send + Sync`，可跨线程移动 |

**⚠ Context ≠ 自动并发**：用 `Shared` 不会给每个算子 spawn 线程，它只是**启用**线程安全
保证。默认 `Shared::of(1).map(..)` 仍在当前线程执行（只是换成线程安全原语）。要真正跨线程
必须显式用调度算子 `subscribe_on`/`observe_on`（§5.7）。

### qrass 规约（设计 D1 分层）

- **阶段内核一律 `Local`**：lex/parse/eval/render 这类单线程逐项闭包状态机，无 Send/Sync
  需求，避免每项锁开销。`SharedCtx::transform` 会把每个算子状态包进 `Arc<Mutex<..>>`，
  next 走 `Arc::clone + Mutex::lock`——逐字符/逐 token 高频流若上 Shared 每项付锁成本。
- **模块编排层用 `Shared` + tokio multi_thread**：仅当出现两个结构性需求（不容绕升）：
  1. **模块单例**（sass 规范：同一 canonical URL 只实例化一次、被 N 父引用）= Subject /
     Connectable 的多播定义；多播载体 `Local::subject`（单线程扇出）/ `Shared::subject`
     （跨线程，内部即 `Arc`）。
  2. **@use 闭包互不相交的独立分支并发实例化**：`Shared::flat_map`（inner 并发
     `usize::MAX`）+ `merge_observables` 是响应式并发的标准形态。qrass 已按该**结构形态**落地
     （`src/branch_fanout.rs`：resolve / acquire 两段 `Shared` 管道 + `mpsc` 终端 + 批次索引重排），
     但**不开真线程**——红线 6 禁用 `observe_on`/`subscribe_on`，且实测该段仅占模块编排 ~5%
     （directives 子集 6.95 ms / 32.96 ms / 60.80 ms），线程化收益上限即此。取源工作无重复可摊销
     （607 次 fan-out 对应 607 次实例化，无重复分支），故此层的价值的架构一致性而非性能。
- **`Arc<Mutex>` 仅允许多播/缓存协调的框架管线，禁止作业务数据载体**（§11 红线）。
- 缓存共享用 `Arc<不可变 Module>` 只读多播。
- 切换上下文：`ctx.into_shared()` / `ctx.into_local()`（`context.rs` L450/462，
  **仅换包装层，不改变执行模型**）。Local→Shared 会要求全链 `Send+Sync+'static`，
  qrass 阶段内核含 `Rc`/非 Send 值会编译失败——保持 Local。

---

## 3. Scheduler 与订阅生命周期（Book《Scheduler》《Core Concepts》章）

### 3.1 Scheduler 是"句柄"不是 runtime

`LocalScheduler`/`SharedScheduler` 实例只是指向底层执行器的**轻量句柄**：
`LocalScheduler::default()` **不创建新线程/loop**，同线程所有实例共享同一 thread-local
executor；所有 `SharedScheduler` 派发进同一全局线程池。所以算子可廉价即时构造调度器。
默认调度器依赖 tokio；关 `scheduler` feature 可实现自定义调度器（Book《Custom Scheduler》，
qrass 不用）。

**qrass 禁用一切定时能力**：只有定时源（`timer`/`interval`）与定时算子
（`delay`/`buffer_time`/`debounce`/`sample`/`throttle`）会调用 Scheduler（Local→需
tokio runtime）。qrass 是同步编译管道，一律不用（§4/§5 表中标注）。

### 3.2 Subscription：Drop 不自动退订，需显式控制

`Subscription::unsubscribe(self)`（`subscription.rs` L100）**消费 self**；且 Subscription
被 drop 时**不会**自动退订（与某些 Rx 实现不同），给你显式生命周期控制。要 RAII 自动退订，
显式取 guard 并持有：

```rust
let sub = Local::interval(period).subscribe(|_| /* … */);
let _guard = sub.unsubscribe_when_dropped();   // _guard 离开作用域即取消流
```

qrass 同步内联执行下 subscribe 返回即执行完毕，通常无需退订；此规则主要用于定时/async 源。

### 3.3 observe_on vs subscribe_on（跨线程搬移）

- `observe_on(sched)`：改变**其下游**所有发射的处理位置（消费者侧）。
- `subscribe_on(sched)`：改变**整条链订阅 + 源执行**的位置（生产者侧 + 初始 setup）。

**规则**：Local 创建的流（持 `Rc`/`!Send` 数据）**不能** `observe_on(SharedScheduler)`
搬到后台线程——编译器阻止。要跨线程必须**从一开始就是 Shared**（或只携带 `Send` 数据）。

---

## 4. 工厂方法（`factory.rs` `ObservableFactory`，blanket 提供）

Context 充当 Observable 工厂，确保流从正确环境起步。`Local::*` / `Shared::*` 同签名：

```text
Local::create<Item, Err, F, U>(f)   F: FnOnce(&mut dyn Emitter<Item,Err>) -> U, U: Subscription
Local::of(v)                        单值 + complete
Local::empty()                      立即 complete
Local::never()                      永不发射（需 take/drop 控制）
Local::throw_err(e)                 立即 error
Local::from_iter(iter)              I: IntoIterator，逐项同步发射
Local::from_fn(f)                   每次订阅调 f 产 Item（Fn()，无 Emitter）
Local::defer(f)                     每订阅重建一条链: F: Fn() -> O(Context)
Local::subject::<Item,Err>()        多播热流（见 §9）
Local::subject_mut_ref::<Item,Err>()多播，mut-ref 分发
Local::behavior_subject(initial)    带最新缓存的多播（Item: Clone）
Local::behavior_subject_mut_ref(..) 同上，mut-ref 分发
Local::merge_observables(vec)       并发订阅合并多条同类流
Local::concat_observables(vec)      顺序订阅多条同类流
—— 以下 async/定时源，qrass 禁用 ——
Local::from_future(fut) / _with(sched)              单值 async 源（需 runtime）
Local::from_future_result(fut) / _with(..)          Future<Output=Result<I,E>> → 值/错分流
Local::from_stream(st) / _with / _result / _result_with   Stream → 多值 async 源
Local::timer(dur) / _with / _at(instant) / _at_with 定时单发（需 runtime）
Local::interval(period) / _with                      定时多发（需 runtime）
```

**`create` 的闭包参数是 trait 对象 `&mut dyn Emitter<Item, Err>`**
（`rxrust::observer::Emitter`，**不在 prelude**，必须显式 `use`）：

```rust
pub trait Emitter<Item, Err> {
  fn next(&mut self, value: Item);
  fn error(&mut self, err: Err);
  fn complete(&mut self);
}
```

三个方法都是 `&mut self`（**区别于 `Observer` 的 error/complete 消费 self**）。
闭包返回 `U: Subscription`（同步步源返回 `()` 即可）。

```rust
use rxrust::prelude::*;
use rxrust::observer::Emitter;               // ← 不在 prelude
Local::create(|emitter: &mut dyn Emitter<i32, std::convert::Infallible>| {
  emitter.next(1);
  emitter.complete();
  ()                                          // U: Subscription
}).subscribe(|v| /* … */);
```

---

## 5. 算子签名速查（`observable.rs` 逐条核实）

闭包边界一律 `for<'a> FnMut`（HRTB）。**Item 传值还是传引用由算子决定**。
Book《Operators》章的 marble diagram 语义与 ReactiveX 一致，本节只记 Rust 特有签名。

### 5.1 变换

```text
map(F)          F: for<'a> FnMut(Item<'a>) -> Out          值进值出，按 move
map_to(v)       v: Clone                                   常数映射
filter(F)       F: for<'a> FnMut(&Item<'a>) -> bool         ★ 借引用判断（不能改值）
filter_map(F)   F: for<'a> FnMut(Item<'a>) -> Option<Out>   ★ 值进 Option
tap(F)          F: for<'a> FnMut(&Item<'a>)                 ★ 只读副作用
map_err(F)      错误通道类型变换（不吞错）
```

### 5.2 状态折叠（所有权友好，qrass 核心武器）

```text
scan(Acc, F)          Output: Clone 才可用; F: FnMut(Output, Item) -> Output   每步 emit 中间态
scan_map(Acc, F)      F: for<'a> FnMut(&mut Acc, Item<'a>) -> Output           ★ Acc 无需 Clone！每步 emit
reduce_initial(Out, F)F: for<'a> FnMut(Out, Item<'a>) -> Out                   ★ 只 emit 终值；空流 emit initial
reduce(F)             F: FnMut(Item<'a>, Item<'a>) -> Item<'a>                 无种子；空流不发射
collect::<C>()        C: Default(Vec 等)                                       ★ complete 后 emit 一个 C
collect_into(C, init) 预填集合再收集
last()                只 emit 最后一个值；last_or(default) 空流兜底
pairwise()            emit (前, 后) 元组
max() / min()         Item: PartialOrd，emit 极值（owned Item<'static>）
```

**选择规则**：需要**逐项中间态** → `scan_map`；只要**一个终值** → `reduce_initial`
（非 Clone Acc 亦可）或 `collect`；Acc 非 Clone → 必用 `scan_map`/`reduce_initial`。
`reduce_initial` 空流发 initial（等价 `scan(i,f).last().default_if_empty(i)`），
`reduce`/`max`/`min` 空流不发射——据此选终端兜底策略。

### 5.3 级间衔接（高阶算子 —— 编译器管道的正确连接方式）

```text
flat_map(F)      F: for<'a> FnMut(Item<'a>) -> Inner
                 Inner: Context<Inner: ObservableType<Err = Self::Err>>   ★ inner Err 必须 = outer Err
                 默认并发 usize::MAX（inner 同步源时等价顺序执行）
concat_map(F)    同 flat_map 但 inner 顺序保证（并发=1）
switch_map(F)    新 inner 到来时取消旧 inner
merge_all(n)     对流中流做并发展平（上游本身 emit Observable）
concat_all()     = merge_all(1) 顺序展平
```

flat_map 要求 inner 与 outer **同 Err**——两条分支（正常流 / 错误短路流）必须同 Item 同
Err，这正是把"阶段输出"统一为 `Result<T, E>` 数据项 + `Err = Infallible` 协议的动机（§8）。

**高阶（flat_map 之后）的三条源码事实**（rc.5 实测，qrass `src/branch_fanout.rs` 踩坑记录）：

1. **`.into_local()` 不能接在 `flat_map` 之后**。`into_local` 只改写**外层**上下文的作用域，
   inner 流仍是 `Shared`，合并机随即要求 `MutRc<MergeAllState<..>>: Observer<SharedCtx<..>, _>`
   → E0277。故 `Shared` 高阶段的终端只能是 `Shared` 全程 + `Send` 闭包 subscribe（配 `mpsc`），
   **不能**复用 `into_local().box_it()` 那套 Local 终端。反过来 Local 段接 `into_shared` 同理要谨慎。
2. **`Of<T>::Err` 硬编码 `Infallible`**（`observable/of.rs`：`impl<T> ObservableType for Of<T> { type Err = Infallible; }`）。
   所以 `Shared::of(v)` 作 inner 时，闭包 subscribe 是**安全**的——没有 error 通道可被静默丢弃，
   所有失败必须以 `Result` 走值通道（与 §8 协议天然吻合）。
3. **inner 为同步源时，`Shared` 合并顺序 = 外层顺序**（rxrust 自带 `flat_map_merges_inner_observables`
   钉 `vec![1, 11, 2, 12, 3, 13]`）。但这是"当前实现恰好内联"的副产品，**并发形态的消费者不得依赖**：
   结果自带批次索引 + 消费前排序，才是可移植契约。

### 5.4 过滤/截断/生命周期

```text
take(n) / take_last(n) / skip(n) / skip_last(n) / skip_while(F) / skip_until(N)
take_while(F)          F: FnMut(&Item)->bool;  take_while_inclusive 含触发项
take_until(N) / skip_until(N)   N: Observable<Err = Self::Err>  通知流首值触发
distinct / distinct_key(F) / distinct_until_changed / distinct_until_key_changed(F)
contains(v)
default_if_empty(v)    空流兜底
finalize(F)            F: FnOnce()  任何终止路径(next 完/error/取消)都执行  ★ 收尾 span
on_error(F)            F: FnOnce(Self::Err)   ★ 错误旁路，不改流
on_complete(F)         F: FnOnce()
retry(P)               P: RetryPolicy<Err>: Clone { fn should_retry(&Err, attempt)->Option<Duration> }
```

### 5.5 组合

```text
zip(B)                 B: Observable<Err=Self::Err>, emit (A,B) 配对
with_latest_from(S2)   S2: Observable<Err=Self::Err>, 以 Self 为节拍取 S2 最新值
combine_latest(S2, F)  F: FnMut(A::Item, B::Item) -> Out
merge(S2)              两流合流；S2 的 Item<'a> 必须与 Self 完全同型
start_with(Vec<Item>)  源之前先发射一批值
buffer(N)              通知流 N 发射时把缓冲 emit 成 Vec
buffer_count(n)        每 n 项 emit Vec
group_by(F) / group_by_mut_ref(..)   分组多播
```

### 5.6 聚合/缓冲/定时（定时 qrass 禁用）

```text
collect / scan / reduce_initial / reduce / max / min / average(Item: Averageable)
buffer_time(dur) / debounce(dur) / sample(N) / throttle(dur, ThrottleEdge)   ← 定时，qrass 禁用
delay(dur) / delay_at / delay_with(sched) / delay_subscription(..)           ← 定时，qrass 禁用
observe_on(sched) / subscribe_on(sched)                                      ← 跨线程搬移，qrass 禁用
```

---

## 6. 终端（drain）三种写法

```text
(a) subscribe(closure)          F: for<'a> FnMut(Item)。要求 Err = Infallible（§1）。
                                只处理 next；qrass 用"值流内已编码 Result 错误"满足之。
(b) subscribe_with(Observer)    自实现 Observer trait（next=&mut self, error=self,
                                complete=self, is_closed=&self）；或 FnMutObserver 包装。
                                用于需要显式 error/complete 或 Err 非 Infallible。
(c) into_future()               需 async runtime；Output =
                                IntoFutureResult<T,E> = Result<Result<T,E>, IntoFutureError>
                                  Ok(Ok(v))            恰好一个值
                                  Ok(Err(e))           流发了 error
                                  Err(Empty)           流未发任何值就 complete
                                  Err(MultipleValues)  流发了多于一个值
(d) into_stream()               Observable → Stream<Item=Result<Item,Err>>，用 async 循环消费
```

**qrass 标准终端（同步 + 错误可观测）：mpsc 收口。**

```rust
use std::sync::mpsc;
let (tx, rx) = mpsc::channel::<Result<Out, SassError>>();
stream
    .map(final_extract)                        // Item<'static> = Result<Out, SassError>
    .subscribe(move |r| { tx.send(r).ok(); }); // 同步内联执行，返回即有值
rx.recv().unwrap_or_else(|_| Err(missing_output_error))
```

**非定时源 subscribe 时全内联同步执行**：`of`/`from_iter`/`create`/`defer`/`empty`/`never`/
`throw_err` 及 `map`/`filter`/`scan*`/`reduce*`/`collect`/`flat_map`/`tap`/`zip`/`merge`…
返回后立即 `rx.recv()` 一定能拿到值，无需 runtime。规则：**编译器错误永远不进 `Err` 通道
（=Infallible），而作为流内 `Result` 数据项**传到终端——同时规避 §1 的闭包 Infallible 约束
与 `subscribe(closure)` 吞错问题。需要错误旁路监听用 `on_error`。

---

## 7. 类型擦除：box_it 家族（`observable/boxed.rs`）

跨函数边界传递"一条流"的唯一合法方式（Book《Type Erasure》章）：

```text
.box_it()                → LocalBoxedObservable<'a, Item, Err = ()>
                             = Local<Box<dyn DynCoreObservable<
                                 LocalCtx<BoxedObserver<'a, Item, Err>, LocalScheduler>> + 'a>>
.box_it_clone()          → LocalBoxedObservableClone<...>  （可多次订阅/Clone 的擦除流）
.box_it_mut_ref()        → Item = &'a mut T 分发的擦除流
.box_it_mut_ref_clone()  → mut-ref + Clone
Shared 侧同族：SharedBoxedObservable / ..Clone / ..MutRef / ..MutRefClone（要求 Send）
别名需 feature="scheduler"（默认开启），已 re-export 进 prelude。
```

**完整类型对照（Book）**：

| Context | Cloneable | Item | 别名 |
|---|---|---|---|
| Local | 否 | `T` | `LocalBoxedObservable<'a, Item, Err>` |
| Local | 是 | `T` | `LocalBoxedObservableClone<'a, Item, Err>` |
| Local | 否 | `&mut T` | `LocalBoxedObservableMutRef<'a, Item, Err>` |
| Local | 是 | `&mut T` | `LocalBoxedObservableMutRefClone<'a, Item, Err>` |
| Shared | 否/是 | `T`/`&mut T` | 上表 `Local→Shared` 对应四项 |

选用：分支返回不同流 / 存进 struct 字段 / API 边界 / 异构集合 → `box_it()`；
需要擦除流可 `Clone`（模板流多订阅）→ `box_it_clone()`；mut-ref 分发 → `_mut_ref` 族。

注意 `LocalBoxedObservable` 自带 `Local` 上下文包装，可直接续接算子链、`flat_map` 作 inner、
`subscribe` 终端。生命周期取 `'static`（Item/Err 均 owned）。
`Err = ()` 是别名默认值不是通配——显式写 `Infallible` 与 `create`/闭包 subscribe 保持一致。

**反模式**：试图 `fn stage<O: Observable<...>>` 泛型化签名传链 → E0107（GAT）。
唯一出路是 `box_it`。

---

## 8. qrass 编译器管道配方（本手册落地模板）

阶段 = 一条冷流函数；级间 = `flat_map` 驱动；错误 = 流内 `Result`；
builder 只收集配置；终端一次 mpsc 收口：

```rust
use rxrust::observer::Emitter;
use rxrust::prelude::*;
use std::convert::Infallible;

/// 阶段输出流类型别名：Item 是"下一阶段输入或错误"，错误永不进 Err 通道。
type Stage<T> = LocalBoxedObservable<'static, Result<T, SassError>, Infallible>;

/// 短路：携带错误直达终端（与任意正常阶段流同型，满足 flat_map 的 Err 一致性）
fn fail<T>(e: SassError) -> Stage<T> { Local::of(Err(e)).box_it() }

/// 单阶段模板：入口零参源用 create；上游数据 → 本阶段产物用闭包算子
fn lex_stage(loaded: Loaded) -> Stage<Lexed> {
    let span = tracing::span!(tracing::Level::INFO, "compile",
        stage = "lex", module = %loaded.name, elapsed_ms = tracing::field::Empty);
    let started = std::time::Instant::now();
    let mut carrier = Some(loaded);
    lexer::token_stream(/* 逐 token 发射的源：Local::create(..) 见 §4 */)
        .collect::<Vec<_>>()                        // Vec<Result<Spanned, SassError>>
        .map(move |toks| {
            let _g = span.enter();                  // span 进入实际执行处
            let loaded = carrier.take().expect("once");
            let out = toks.into_iter()
                .try_fold(Vec::new(), |mut acc, r| r.map(|t| { acc.push(t); acc }))
                .map(|tokens| Lexed { loaded, tokens });
            span.record("elapsed_ms",
                &tracing::field::debug(started.elapsed().as_millis() as u64));
            out
        })
        .box_it()
}

/// 编排：全部级间由流发射驱动（flat_map），无顺序语句串联
fn drain(cfg: Config) -> Result<String, SassError> {
    let (tx, rx) = mpsc::channel();
    load_stage(cfg.entry)
        .flat_map(|r| match r { Ok(l) => lex_stage(l),   Err(e) => fail(e) })
        .flat_map(|r| match r { Ok(l) => parse_stage(l), Err(e) => fail(e) })
        .flat_map(|r| match r { Ok(p) => eval_stage(p, cfg.style), Err(e) => fail(e) })
        .flat_map(|r| match r { Ok(v) => render_stage(v), Err(e) => fail(e) })
        .map(|r| r.map(|rendered| rendered.css))
        .subscribe(move |out| { tx.send(out).ok(); });
    rx.recv().unwrap_or_else(|_| Err(SassError::new(ErrorKind::Render, "no output")))
}
```

阶段内部逐 token/逐语句的"细粒度流"（如 lexer 的 `create` 源、render 的
`from_iter + reduce_initial`）保持同风格；`token_stream()` 每次 `emitter.next()` 发一个
`Result<Spanned, SassError>`，出错即 `complete()` 收尾。

---

## 9. Subject 多播（`subject/*`，Book《Subjects》章）

Subject 把"冷流"变"热流"：一个源多播给 N 个订阅者。`Subject` 类型按创建它的 Context
自动选内部存储（Local→`Rc<RefCell<..>>`，Shared→`Arc<Mutex<..>>`）：

```rust
let subject = Local::subject::<i32, Infallible>();
subject.clone().subscribe(|v| /* 订阅者 A */);
subject.clone().subscribe(|v| /* 订阅者 B */);
subject.next(1);          // Subject（经 Context 包装）暴露发射：next/error/complete
subject.next(2);
subject.complete();
```

- `Local::behavior_subject(initial)` 缓存最新值（`Item: Clone`），新订阅者立即收到最近值
  （Redux 风格 state store 的原语，Book Cookbook《State Store》）。
  `subject_mut_ref` / `behavior_subject_mut_ref` 提供 `&mut T` 分发。
- **qrass 定位**：Subject 是框架内部多播管线，不是业务状态容器。仅当一个阶段输出需同时喂
  多条下游链时使用（模块单例 fan-out、后续 watch/HMR）。arena+move 绕开多播是对 fan-out
  拓扑的扭曲表达。
- `Local::empty()` / 工厂源已支持直接 `.subscribe`，无需 Subject 包装。
- Connectable（`fork`/`connect`/`ref_count`，`Connectable` trait 已进 prelude）用到再读。

---

## 10. Async 互操作（Book《Async Interop》章，qrass 主用同步管道）

Push 与 Pull 共存的双向桥：

- `Shared::from_future(fut)` / `from_future_result(fut)`：Future → Observable，
  接入 `timeout`/`retry`/`switch_map` 等算子。
- `Shared::from_stream(st)` / `from_stream_result(st)`：Stream → Observable（如
  `tokio::io::split`、`tokio::sync::mpsc`、`futures::stream`），叠加时间类算子。
- `obs.into_future()`：期望单值的 Observable → Future（配合 `first`/`take(1)`）。
- `obs.into_stream()`：Observable → `Stream<Item = Result<Item, Err>>`，用标准
  `while let Some(r) = stream.next().await` 消费；complete（或发 error 后）Stream 结束。

`SharedScheduler` 在原生环境默认用 `tokio::spawn`；配合 `observe_on`/`subscribe_on` 精确
控制执行位置（§3.3）。qrass 端到端为同步内联，除非接 e2e 异步层，一般不动用。

---

## 11. Nightly 实验特性（Book《Nightly》章，qrass 不用）

稳定版 `map` 签名 `F: for<'a> FnMut(Item<'a>) -> Out` 使 `Out` 为固定类型，**无法表达
输出借用输入生命周期的映射**（`&'a T -> &'a U` / `&'a mut T -> &'a U`，需 type family
`Out<'a>`）。`nightly` feature 为 `map` 提供实验性生命周期相关输出支持（配合
`subject_mut_ref`）：`rxrust = { features = ["nightly"] }` + `cargo +nightly`。
qrass 用 stable，不启用；mut-ref 需求走 `box_it_mut_ref` 而非 nightly map。

---

## 12. 自定义算子（Book《Custom Operators》章，四件套）

"Write Once, Run Everywhere"：从 `CoreObservable<C>`（C 泛型 Context）实现，自动适配
Local/Shared。四步：

1. **Operator Struct**：持 source + 参数（`Map { source, func }`）。
2. **Observer Struct**：包装下游拦截事件（业务逻辑在此）：
   `impl Observer<Item, Err> for XObserver { fn next(&mut self, ..); fn error(self,..);
   fn complete(self); fn is_closed(&self) }`。
3. **ObservableType**：声明 `type Item<'a>; type Err;`。
4. **CoreObservable<C>（glue）**：`type Unsub = S::Unsub;`，
   `fn subscribe(self, context: C) -> Self::Unsub { let wrapped =
   context.transform(|obs| XObserver { observer: obs }); self.source.subscribe(wrapped) }`
   ——`Context::transform` 是关键：解包下游 observer、包进自定义 observer、按同 Context
   族重新装回。

集成：项目内部用**扩展 trait**（`trait XOp: Observable { fn x(self) -> Self::With<X<..>>
{ self.transform(|src| X { source: src }) } }` + `impl<T: Observable> XOp for T {}`）。
qrass 优先用 `scan_map`/`reduce_initial`/`flat_map` 组合既有算子，仅在确有必要时自定义。

---

## 13. 与 RxJS / rxrust 0.15 的关键差异（防旧习）

| 旧习惯 | 1.0（rc.5）现实 |
|---|---|
| `pipe(op1, op2)` | 无 pipe，方法链直写 |
| `and_then` 操作符 | **已删除**。等价物：`flat_map` / `concat_map`（同步源下语义相同）|
| 0.15 `Arc<Mutex<Option<T>>>` + try_unwrap 收口 | mpsc channel 或 `into_future` / `into_stream` |
| 0.15 `Observable<Item=T>` 关联类型 | **GAT `Item<'a>`**，泛型约束写法完全不同（E0107）|
| `share()/publish()` | `Subject` / `ref_count()` / `Connectable` 路径 |
| 0.15 `Observer` 三闭包构造器 | `Observer` trait（next=&mut，error/complete=self）；便捷用 `subscribe(closure)`（Err 须 Infallible）或 `subscribe_with(FnMutObserver(..))`/自实现 |
| `zip(a,b)` 自由函数 | 实例法 `a.zip(b)` / 工厂 `merge_observables(Vec)` |
| drop 自动退订 | **Subscription drop 不自动退订**；显式 `unsubscribe(self)` 或 `unsubscribe_when_dropped()` guard |
| JS `switchMap` 取消语义 | 同名 `switch_map` ✓ 存在 |
| Scheduler 传参 | v1.0 **Scheduler 隐式绑定 Context**；覆盖用 `_with` 后缀算子 |

---

## 14. 所有权红线（项目规则，违反 = 任务失败）

1. **禁止 `Arc<Mutex<_>>` 承载业务数据**（Token 流、AST、源码、累加器）。按值 move：
   `scan_map(&mut Acc, item)` / `reduce_initial(owned_acc, ..)` / `collect` 后整包 move。
   `Arc<Mutex>` 仅限多播/缓存协调的框架管线。
2. **禁止 println!/eprintln!**；跨阶段用 `tracing::span!`（stage/module/elapsed_ms），
   计时写在**实际执行的闭包内**（§8 模板），或 `finalize` 收尾。管道处理必须用 span
   记录上下文与耗时，禁止仅 event! 单日。
3. 闭包捕获 owned 值 + `move`；一次性消费用 `Option<T>::take()` 满足 `FnMut`。
4. src/ 内零测试；测试一律 `#[test]` 放 `tests/*.rs`；单文件 ≤500 行。禁 Python，
   测试用 Rust。
5. 错误协议：编译器错误（SassError）**永远作为流内 `Result` 数据**传递，Err 通道留
   `Infallible`（同时满足闭包 subscribe 约束）；错误旁路监听用 `on_error`。
6. 禁用定时/跨线程算子（`timer`/`interval`/`delay`/`debounce`/`throttle`/`sample`/
   `observe_on`/`subscribe_on`）——qrass 为同步内联管道。

---

## 15. 常见编译错误 → 处置

| 症状 | 根因 | 处置 |
|---|---|---|
| E0107 `Observable<Item=T>` | Item 是 GAT | 改单函数内联组装，或返回 `LocalBoxedObservable` 别名 |
| 闭包 `subscribe` 报 Err 类型不符 | 闭包 subscribe 要求 `Err = Infallible` | 把错误编进流内 `Result`，`Err=Infallible`；否则 `subscribe_with(Observer)` |
| `expected String, found &str` in tx.send | channel 类型推断被 map 闭包遮蔽 | 显式标注 `mpsc::channel::<Result<T,E>>()` |
| flat_map 两分支类型不一致 | inner 流 Item/Err 必须同 outer 同型 | 两分支都 `.box_it()` 到 `Stage<T>` |
| `move out of captured value` in map 闭包 | map 是 FnMut 多次调用 | `Option::take()` 一次性取出 |
| create 闭包找不到 Emitter | Emitter 不在 prelude | `use rxrust::observer::Emitter;` |
| subscribe 后 rx.recv() 无值 | 上游是定时算子/async 源 | 去定时算子或进 runtime；同步源不会缺值 |
| into_shared / into_local 报 Send 不满足 | Shared 要求全链 Send+Sync+'static | 保持 Local（qrass 默认） |
| `filter(\|x\| ..)` 里想改值 | filter 收 `&Item` | 改值语义用 map/filter_map |
| reduce/collect 空流终端 `rx.recv()` 报错 | 空流不发射 | 用 `reduce_initial`/`default_if_empty`/`last_or` 兜底，或 rx 的 `Err(Disconnected)` 分支给默认 |
| drop subscription 后流仍在跑 | Subscription drop 不自动退订 | `unsubscribe(self)` 或持 `unsubscribe_when_dropped()` guard |

---

*手册位置：`docs/rxrust-ai-manual.md`。修订依据 = 官方 Book + `rxrust-1.0.0-rc.5` 源码；
如升级 rxrust 版本，必须重新对照 `guide/` 与源码签名核实并更新本文件。*
