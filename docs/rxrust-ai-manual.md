# rxrust 1.0.0-rc.5 AI 编码手册（精读源码版）

> 本手册基于对 `rxrust-1.0.0-rc.5` 源码逐文件精读（observable.rs / context.rs /
> factory.rs / observer.rs / ops/* / observable/boxed.rs），所有签名均从源码核实，
> 非凭记忆。供 AI 在本项目（qrass）中编写/审查响应式代码时直接遵循。
> 版本钉死：`rxrust = "=1.0.0-rc.5"`（裸 `cargo add` 会解析到旧 0.15 线）。
> 保留 default features（`scheduler` feature 提供 `LocalScheduler`/`SharedScheduler`
> 与 `LocalBoxedObservable` 等别名）。

---

## 1. 架构总览：三层 trait + Context 包裹

```text
ObservableType          纯类型暴露:  type Item<'a> where Self:'a;  type Err;
  ▲
CoreObservable<O>       订阅内核:    type Unsub: Subscription;
                                     fn subscribe(self, observer: O) -> Self::Unsub;
  ▲
Context                 执行上下文:  type Inner; type Scheduler: Clone+Default;
                                     type With<T>; type BoxedCoreObservable<'a,Item,Err>;
                                     fn lift/transform/swap/into_inner/scheduler
  ▲
Observable              用户 API:    blanket impl<T: Context where T::Inner: ObservableType>
                                     map/filter/scan_map/flat_map/... 全部挂在这里
```

要点（源码事实）：

- **`Item` 是 GAT**（`type Item<'a>`），不是关联类型。值流 `Item<'a> = T`；
  mut-ref 流 `Item<'a> = &'a mut T`。这导致**泛型函数无法直接约束
  `Obs: Observable<Item = T>`**（E0107），只能写 `for<'a>` HRTB 闭包边界或在
  单函数内联组装。
- `Local<T> = LocalCtx<T, LocalScheduler>`，`Shared<T> = SharedCtx<T, SharedScheduler>`
  （context.rs L358/361）。`T` 是 Inner（core observable），不是 Item。
- 用户写的每条算子链的类型 = `LocalCtx<深层嵌套核心类型>`，**不可命名**，
  跨函数边界必须 `.box_it()`（见 §7）。
- `Observable::subscribe(f)` 的 `f: for<'a> FnMut(Self::Item<'a>)`，
  **只消费 next，error/complete 被静默丢弃**（observable.rs L141-150）。

## 2. 决策表：Local vs Shared

| 场景 | 选择 | 约束 |
|---|---|---|
| 阶段内核：单线程逐项闭包状态机（lex/parse/eval 发射/render） | `Local` | 无 Send/Sync 要求，避免每项锁开销 |
| 模块编排：同一模块被 N 父引用（单例=多播）、独立分支并发实例化 | `Shared` + tokio multi_thread | Item/Err/闭包捕获值需 `Send+Sync+'static`；缓存用 `Arc<不可变 Module>` 只读共享 |
| 切换上下文 | `ctx.into_shared()` / `ctx.into_local()`（context.rs L450/462，仅换包装层，不改变执行模型） | |

qrass 规约（设计 D1 分层）：**阶段内核一律 Local；模块图/多播/并发层用 Shared**。两个结构性需求不容绕升：
1. 模块单例（sass 规范：同一 canonical URL 只实例化一次，被 N 父引用）就是 Subject/Connectable 的多播定义，用 arena+move 绕开是对 fan-out 拓扑的扭曲表达；多播载体：`Local::subject`（单线程内扇出）/ `Shared::subject`（跨线程，内部即 Arc）；`BehaviorSubject` 提供“当前值+后续变更”。
2. 组件间 @use 闭包互不相交时，`Shared::flat_map`（inner 并发 usize::MAX，非同步源下各 inner 在其他线程推进）+ `merge_observables` 是响应式并发的标准形态。
注意 Shared 的真实成本：每个算子状态被 `Arc<Mutex<…>>` 包裹（SharedCtx::transform），next 走 `Arc::clone + Mutex::lock`——高频逐项流（如逐字符 lexer）必须留在 Local，否则每项付锁开销。`Arc<Mutex>` 仅允许作为多播/缓存协调的框架管线（非业务数据载体）。

## 3. 执行模型（源码语义）

- `create/of/from_iter/empty/never/throw_err` 及全部非定时算子（map/filter/
  scan*/reduce*/collect/flat_map/tap/zip/merge…）在 `subscribe` 时**全内联同步执行**。
  因此 subscribe 返回后立即 `rx.recv()` 一定能拿到值，无需 runtime。
- 只有定时源（`timer/interval`）与定时算子（`delay/buffer_time/debounce/sample/
  throttle`）调用 Scheduler（Local→`spawn_local`，需 tokio runtime）。qrass 禁用。
- `flat_map` 并发度默认 `usize::MAX`（inner 同步源时等价顺序执行）；
  需要严格顺序语义用 `concat_map`（同一 FlatMap 实现，concurrent=1）；
  丢弃旧 inner 用 `switch_map`。

## 4. 工厂方法（factory.rs `ObservableFactory`，blanket 提供）

```text
Local::create<Item, Err, F, U>(f)      F: FnOnce(&mut dyn Emitter<Item,Err>) -> U, U: Subscription
Local::of(v)                           单值+complete
Local::empty()                         立即 complete
Local::never()                         永不发射（需 take/drop 控制）
Local::throw_err(e)                    立即 error
Local::from_iter(iter)                 I: IntoIterator，逐项同步发射
Local::from_fn(f)                      每次订阅调用 f 产 Item（Fn()，无 Emitter）
Local::defer(f)                        每订阅重建一条链: F: Fn() -> O(Context)
Local::subject::<Item,Err>()           多播（见 §9）
Local::subject_mut_ref::<Item,Err>()   多播，mut-ref 分发
Local::behavior_subject(initial)       带最新缓存的多播（Item: Clone）
Local::from_future(fut)                单值 async 源（需 runtime）
Local::from_future_result(fut)         Future<Output=Result<Item,Err>> → 值/错误分流
Local::from_stream(st)                 Stream → 多值 async 源
Local::timer(duration) / timer_at(instant)      定时单发（需 runtime）
Local::interval(period)                定时多发（需 runtime）
Local::merge_observables(vec)          合并多条同类流（并发订阅）
Local::concat_observables(vec)         顺序订阅多条同类流
```

`create` 的闭包参数是 **trait 对象** `&mut dyn Emitter<Item, Err>`
（`rxrust::observer::Emitter`，**不在 prelude**，必须显式 `use`）。
`Emitter: { fn next(&mut self, Item); fn error(&mut self, Err); fn complete(&mut self); }`
——三个方法都是 `&mut self`（区别于 `Observer` 的 error/complete 消费 self）。
闭包返回 `U: Subscription`（同步步源返回 `()` 即可）。

## 5. 算子签名速查（observable.rs 逐条核实）

闭包边界一律 `for<'a> FnMut`（HRTB）。**Item 传值还是传引用是算子决定**的：

### 5.1 变换

```text
map(F)                 F: for<'a> FnMut(Item<'a>) -> Out               值进值出，按 move
map_to(v)              v: Clone                                        常数映射
filter(F)              F: for<'a> FnMut(&Item<'a>) -> bool             ★ 借引用判断
filter_map(F)          F: for<'a> FnMut(Item<'a>) -> Option<Out>       ★ 值进 Option
tap(F)                 F: for<'a> FnMut(&Item<'a>)                     ★ 只读副作用
map_err(F)             错误通道映射（不吞错）
```

### 5.2 状态折叠（所有权友好，qrass 核心武器）

```text
scan(Acc, F)           Output: Clone 才可用的旧式; F: FnMut(Output, Item) -> Output
scan_map(Acc, F)       F: for<'a> FnMut(&mut Acc, Item<'a>) -> Output  ★ Acc 无需 Clone！
                            每步 emit Output（中间态流）
reduce_initial(Out, F) F: for<'a> FnMut(Out, Item<'a>) -> Out          ★ 只 emit 终值一个
reduce(F)              F: FnMut(Item<'a>, Item<'a>) -> Item<'a>        无种子；空流不发射
collect::<C>()         C: Default（Vec<T> 等）; complete 后 emit 一个 C ★ 终值单项
last()                 只 emit 最后一个值
pairwise()             emit (前, 后) 元组
```

选择规则：需要**逐项中间态** → `scan_map`；只要**一个终值** →
`reduce_initial` 或 `collect`；Acc 非 Clone → 必用 `scan_map`/`reduce_initial`。

### 5.3 级间衔接（高阶算子 —— 编译器管道的正确连接方式）

```text
flat_map(F)            F: for<'a> FnMut(Item<'a>) -> Inner
                       Inner: Context<Inner: ObservableType<Err = Self::Err>>
                       上游每 emit 一个值 → F 产一条新流并订阅，inner 的 next 直通下游
concat_map(F)          同 flat_map 但顺序保证（inner=1 并发）
switch_map(F)          新 inner 到来时取消旧 inner
merge_all(n)           对流中流做并发展平（上游本身 emit Observable）
```

flat_map 的 inner Err 必须等于 outer Err —— **两条分支（正常流/错误短路流）
必须同 Item 同 Err**，这正是把"阶段输出"统一为 `Result<T, E>` 数据项 +
`Err = Infallible` 的协议动机（§8 配方）。

### 5.4 截断/生命周期

```text
take(n) / take_last(n) / skip(n) / skip_while(F) / skip_until(N)
take_while(F)          F: FnMut(&Item)->bool;  take_while_inclusive 含触发项
take_until(N)          N: Observable<Err = Self::Err>   通知流首个 next/error 触发截断
default_if_empty(v)    空流兜底
finalize(F)            F: FnOnce()  任何终止路径（next 完/error/取消）都执行 ★ 收尾 span
on_error(F)            F: FnOnce(Self::Err)   ★ 错误旁路，不改流
on_complete(F)         F: FnOnce()
retry(P)               P: RetryPolicy<Err>: Clone { fn should_retry(&Err, attempt)->Option<Duration> }
```

### 5.5 组合

```text
zip(B)                 B: Observable<Err=Self::Err>, emit (A,B) 配对
with_latest_from(S2)   S2: Observable<Err=Self::Err>, 以 A 为节拍取 B 最新值
combine_latest(S2, F)  F: FnMut(A::Item, B::Item) -> Out
merge(S2)              两流合流；S2 的 Item<'a> 必须与 Self 完全同型
```

### 5.6 聚合/缓冲

```text
buffer_count(n)        每 n 项 emit Vec
average(Item: Averageable) 等数值聚合
distinct / distinct_until_changed
group_by(...)          分组多播
```

## 6. 终端（drain）三种写法

```text
(a) subscribe(closure)          只有 next。适合"值流里已编码 Result 错误"的场景
(b) subscribe_with(Observer)    自实现 Observer trait（next=&mut self,
                                error=self, complete=self），或 FnMutObserver 包装
(c) into_future()               需要 async runtime；Output =
                                IntoFutureResult<T,E> = Result<Result<T,E>, IntoFutureError>
                                （外层 Err = 未收到任何值/多值等管道错误）
```

**qrass 标准终端（同步 + 错误可观测）**：mpsc 收口。

```rust
use std::sync::mpsc;
let (tx, rx) = mpsc::channel::<Result<Out, SassError>>();
stream
    .map(final_extract)                 // Item<'static>=Result<Out,SassError>
    .subscribe(move |r| { tx.send(r).ok(); });   // 同步内联执行，返回即有值
rx.recv().unwrap_or_else(|_| Err(missing_output_error))
```

规则：**错误永远不进 `Err` 通道（=Infallible），而是作为流内 `Result` 数据项**
传递到终端，避免 `subscribe(closure)` 吞错问题（§1 事实）。

## 7. 类型擦除：box_it 家族（observable/boxed.rs）

跨函数边界传递"一条流"的唯一合法方式：

```text
.box_it()               → LocalBoxedObservable<'a, Item, Err>
                            = Local<Box<dyn DynCoreObservable<
                                LocalCtx<BoxedObserver<'a, Item, Err>, LocalScheduler>> + 'a>>
.box_it_clone()         → LocalBoxedObservableClone<...>（可多次订阅的擦除流）
.box_it_mut_ref()       → Item = &'a mut T 分发的擦除流
Shared 侧: SharedBoxedObservable<...>（要求 Send）
别名需 feature="scheduler"（默认开启），已 re-export 进 prelude。
```

注意 `LocalBoxedObservable` 自带 `Local` 上下文包装，可直接续接算子链、
`flat_map` 作 inner、`subscribe` 终端。生命周期取 `'static`（Item/Err 均 owned）。
`Err = ()` 是默认值不是通配 —— 显式写 `Infallible` 与 create 源保持一致。

**反模式**：试图 `fn stage<O: Observable<...>>` 泛型化签名传链 → E0107（GAT）。
唯一出路就是 box_it。

## 8. qrass 编译器管道配方（本手册的落地模板）

阶段 = 一条冷流函数；级间 = flat_map 驱动；错误 = 流内 Result；
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
    lexer::token_stream(/* 逐 token 发射的源 */)     // Local::create(...) 见 §4
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

阶段内部逐 token/逐语句的"细粒度流"（如 lexer 的 create 源、render 的
from_iter+reduce_initial）保持同风格；`token_stream()` 每次 `emitter.next()`
发一个 `Result<Spanned, SassError>`，出错即 `complete()` 收尾。

## 9. Subject 多播（subject.rs）

```rust
let subject = Local::subject::<i32, Infallible>();
subject.clone().next(1);                 // Subject 本身实现 Emitter：next/error/complete
subject.clone().subscribe(|v| ...);      // 订阅者克隆共享内层 Rc 状态
subject.complete();
```

- `Subject` 是**框架内部多播管线**，不是业务状态容器。qrass 中仅当
  一个阶段输出需要同时喂多条下游链时使用（如后续 watch/HMR）。
- `behavior_subject(initial)` 缓存最新值（Item: Clone）。
- `Local::empty()`/工厂源在 rc.5 已支持直接 `.subscribe`，无需 Subject 包装。
- Connectable（`fork/join`、`ref_count`）存在于 connectable.rs，用到再读。

## 10. 与 RxJS / rxrust 0.15 的关键差异（防旧习）

| 旧习惯 | rc.5 现实 |
|---|---|
| `pipe(op1, op2)` | 无 pipe，方法链直写 |
| `and_then` 操作符 | **已删除**。等价物：`flat_map` / `concat_map`（同步源下语义相同）|
| 0.15 `Arc<Mutex<Option<T>>>` + try_unwrap 收口 | mpsc channel 或 `into_future` |
| 0.15 `Observable<Item=T>` 关联类型 | GAT `Item<'a>`，泛型约束写法完全不同 |
| `share()/publish()` | `Subject` / `ref_count()` / `multicast` 路径 |
| 0.15 `Observer` 三闭包构造器 | `Observer` trait（next=&mut，error/complete=self）；便捷用 `subscribe_with(FnMutObserver(..))` 或自实现 |
| `zip(a,b)` 自由函数 | 实例法 `a.zip(b)` / 工厂 `merge_observables(Vec)` |
| JS `switchMap` 取消语义 | 同名 `switch_map` ✓ 存在 |

## 11. 所有权红线（项目规则，违反=任务失败）

1. **禁止** `Arc<Mutex<_>>` 承载业务数据（Token 流、AST、源码、累加器）。
   按值 move：`scan_map(&mut Acc, item)` / `reduce_initial(owned_acc, ...)` /
   `collect` 后整包 move。
2. **禁止** println!/eprintln!；跨阶段用 `tracing::span!`（stage/module/elapsed_ms），
   计时记录写在**实际执行闭包**内（§8 模板），或 `finalize` 收尾。
3. 闭包捕获 owned 值 + `move`；一次性消费用 `Option<T>::take()` 满足 `FnMut`。
4. src/ 内零测试；测试一律 `tests/*.rs`；单文件 ≤500 行。
5. 错误协议：编译器错误（SassError）**永远作为流内 `Result` 数据**传递，
   Err 通道留 `Infallible`；需要错误旁路监听用 `on_error`。

## 12. 常见编译错误 → 处置

| 症状 | 根因 | 处置 |
|---|---|---|
| E0107 `Observable<Item=T>` | Item 是 GAT | 改在单函数内联组装，或返回 `LocalBoxedObservable` 别名 |
| `expected String, found &str` in tx.send | channel 类型推断被 map 闭包遮蔽 | 显式标注 `mpsc::channel::<Result<T,E>>()` |
| flat_map 两分支类型不一致 | inner 流 Item/Err 必须同型 | 两分支都 `.box_it()` 到 `Stage<T>` |
| `move out of captured value` in map 闭包 | map 是 FnMut 多次调用 | `Option::take()` 一次性取出 |
| create 闭包找不到 Emitter | 不在 prelude | `use rxrust::observer::Emitter;` |
| subscribe 后 rx.recv() 无值 | 上游是定时算子/async 源 | 去掉定时算子或进 runtime；同步源不会缺值 |
| into_shared 报 Send 不满足 | Shared 要求全链 Send+Sync+'static | 保持 Local（qrass 默认） |
| `filter(|x| ...)` 里想改值 | filter 收 `&Item` | 改值语义用 map/filter_map |

---

*手册位置：`docs/rxrust-ai-manual.md`。修订依据=源码；如升级 rxrust 版本，
必须重新精读签名并更新本文件。*
