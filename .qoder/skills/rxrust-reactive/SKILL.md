---
name: rxrust-reactive
description: rxrust 官方（1.0.0-rc.6）响应式流编码教义，以中文维护。直接使用其算子编写 Rust 响应式流——flat_map 管线、Subject 多播、in-stream Result 错误、box_it 类型擦除、单一终点 subscribe。构建或审查 rxrust 代码（算子、链式阶段、多播、把流 drain 成 owned 结果、Observable 错误）时使用。用所有权取代 Arc<Mutex>，禁止任何自制的 Stage/fail/drain 框架层。
---

# rxrust 响应式流（Rust、所有权优先、直接用算子）

rxrust 是 ReactiveX 的 Rust 移植。它的核心是 **Rust 所有权**：元素以*按值*方式
经过算子流动。直接使用算子 —— 管线**就是**一条算子链。**不要**在 rxrust 之上
搭一层框架抽象、再让 rxrust 去跑*你的*设计。

版本钉（关键）：

```toml
rxrust = "=1.0.0-rc.6"   # 裸 `cargo add rxrust` 会解析到旧的 0.15 线
```

保留默认 features（`scheduler` 控制 `Local`/`Shared` 与 `LocalBoxedObservable`
别名）。1.0 API 与 RxJS 和 rxrust 0.15 均根本不同。权威的逐文件走读与
do/don't 教义住在 **`docs/rxrust-source-analysis/`**（01–09；`09-cancellation-and-terminal-ordering.md`
持有跨算子不变量 —— slot-before-subscribe、terminate-exactly-once、
cancel-only-on-early-termination、锁纪律、Subject 重入）；深入 API 笔记在
[reference.md](reference.md)；`docs/rxrust-ai-manual.md` 只是留作历史的旧 rc.5 副本。

## 那条不断被违反的规则

把控制流**表达为** rxrust 算子；不要重新实现它再包一层：

## 先选动词再写（这是我总跳过的步骤）

用框架自己的清单决定「哪个元素、经哪个算子流动」—— 读 rc.6 的 `src/observable.rs` +
`src/factory.rs`，不要凭记忆：`map map_to filter filter_map scan scan_map reduce reduce_initial
collect collect_into take take_while take_while_inclusive take_until skip skip_while skip_until
skip_last take_last first first_or last last_or average count max min sum distinct distinct_key
distinct_until_changed distinct_until_key_changed contains pairwise merge merge_all concat_all
concat_map flat_map switch_map group_by zip combine_latest with_latest_from sample throttle
throttle_time debounce delay delay_at delay_subscription delay_unsubscription buffer buffer_count
buffer_time buffer_time_max observe_on subscribe_on start_with default_if_empty tap finalize retry
on_error on_complete map_err multicast publish ref_count into_future into_stream box_it` ——
外加 factory 源（`of from_iter from_fn create defer empty never throw_err timer interval
from_future* from_stream* subject* behavior_subject* merge_observables concat_observables`）。
rc.6 里**没有 `share()`、没有 `and_then`、没有 `take_until_by`** —— 由 `grep "fn share"` 核实。
**如果没有一个算子能表达该步骤，那是一个要上报的阻塞点 —— 不是发明一种机制的许可证。**
先写后查源码，正是本项目反复返工的方式。

| 需要 | 用（直接用） | 绝不（事后改造） |
|------|----------------|------------------|
| 循环 / 一项 → 多项（`@each`、嵌套扇出） | `from_iter(items).flat_map(|i| sub_stream(i))` | 把 body 缓冲下来再用 `for`/`while`/`for_each` 重放；`from_fn`+`VecDeque` 工作队列 |
| 每个输入产 0..N 项的**编译相**（词法、结构化、渲染） | 写一个真算子内核（见 §把相写成内核）并逐项 `observer.next(..)` | `scan_map` 返回 `Vec` + `flat_map(from_iter)` —— 把框架的逐项发射塞进容器 |
| **必须以值携带的状态**（env、选择器前缀） | `scan(seed, \|acc, item\| -> acc)` —— 发射项**就是**累加值，所以状态作为数据在下游读/移 | 状态藏在 `Acc` 对象里的 `scan_map`；逐项把 `Arc<Env>` 式快照焊到数据上伪装「状态跟着流走」 |
| 纯私有状态、只要一个输出 | `scan_map(seed, \|&mut acc, item\| -> out)` / `reduce_initial` —— 不需要 `Clone` | `Arc<Mutex<S>>` 累加器；用 push/pop + 控制旗标（`chain_taken`）*驱动*遍历的 `Acc` |
| 一产 → 多消（`@use`、`@mixin`） | `Shared::subject` / `behavior_subject`+`peek` / `publish().ref_count()` —— bus 用法是 `let mut e = bus.clone(); e.next(v)` 然后 `bus.complete()`（消耗型）。**绝不在订阅回调里重发**：对同一 Subject 的重入 `next`/`error`/`complete` **会 panic**；在两者之间插 `delay(Duration::ZERO)` | collect 成 `Vec` 再逐个喂消费者 |
| 两条**独立**流的最新值要相遇 | `with_latest_from` / `combine_latest` | 手工配对缓冲 |
| ~~事件自身位置的状态~~ | **没有任何动词能做到** —— 实测（`tests/multicast.rs`）：`publish().ref_count()` + `with_latest_from` 仍给出 `[(1,6),(2,6),(3,6)]`，每个事件都配到*最终*状态，因为同步源会 drain 进先订阅的分支。改用 `scan`（上一行）。 | 求助配对/多播来修、配对失败再偷运快照 |
| 分支（`@if`） | 对移动后的输入算出的值做 `filter` | 手工穿线的可变 `chain_taken`/状态旗标 |
| 每项链一个阶段 | `flat_map`（**没有 `and_then`**） | 顺序 `run_stage` 语句 |
| 有序并 vs 并发并 | 顺序内生的 → `concat_map`/`concat_all()`/`concat_observables()`（`concurrent: 1`，已验证保序）。真扇出 + 确定性输出 = 一次全局 pass（`collect` + 以确定性 `order` 字段为键的纯 fold）—— 绝不许在每次 merge 后散放 `sort_by_key` | `flat_map` 然后「抱怨顺序」；丢值，因为 `merge_all(n)` 会**丢弃仍在排队的内部源的发射**（已验证：`test_merge_all_concurrency`） |
| 签名确实需要类型擦除处 | `.box_it()` | 为不可名状链型起名（E0107） |
| 错误 | **Err 通道是真的**：`Err = SassError` + `map_err` / `on_error`，或 in-stream `Result<T,E>` 且 `Err = Infallible`；用 `into_future()` 关闭 → `Result<Result<T,E>,_>` | `.ok()` 把失败吞进兜底；`.unwrap_or_else(verbatim)` 当错误策略；依赖 `.subscribe(closure)` —— 它**静默丢弃** `error`/`complete` |

## 把相写成内核（框架自己的形状 —— 不是框架层）

这是 `crates/sasspile/src/phases.rs` 为 `Lexer` / `Structurize` / `Exec` / `Render` 所做的；
当一个相每输入产 0..N 项时，从 `ops/map.rs` 抄这个形状：

```text
struct Phase<S, Params> { source, params }      // 惰性数据，#[derive(Clone)]，无每订阅状态
struct PhaseObserver<O, State> { observer, state }   // 状态随每次 subscribe 诞生，移动而非共享
impl ObservableType  → type Item<'a> = Out where Self: 'a; type Err = S::Err;
impl CoreObservable<C: Context>
  → subscribe(self, ctx): let w = ctx.transform(|o| PhaseObserver { observer: o, .. }); source.subscribe(w)
  → type Unsub = S::Unsub   （多源/高阶：SingleAssignment<C::RcMut<State<U>>>::channel()，
                             先建槽再订阅；只在提前终止时 unsubscribe）
next(): 用 self.observer.next(..) 逐项发射      error(self)/complete(self): 消耗 + 前传
is_closed(): 委托给下游 —— 源就是这样学会提前停止的
```

一个既能失败又能收尾的内核必须只交付一个终点信号、不是两个：`Observer::error/complete` 接收
`self`，所以把下游 observer 持有为 `Option<O>`、经 `self.observer.take()` 交付
（`crates/sasspile/src/phases.rs` 正是如此）。没有 `Option` 的话，一个相报完错误又走到正常结尾会
把两个都前传，而 rxrust 没有断言能抓住它 —— 第二次交付只是订阅者已关闭的流上一个静默的多余事件。

让 `Err` 保持泛型、任何共享的东西用 `C::RcMut`/`C::RcCell`，这样一个内核能同时给
`Local`（wasm32 / `!Send`）与 `Shared` 编译。当闭包需要 observer 而累加器正被借用时，解构
`let PhaseObserver { observer, state } = self;`。
这是**使用**框架的原语（每个 `ops/*.rs` 文件都有的那三件套），
不是 `Stage`/`fail`/`drain` 编排层 —— 禁令针对的是「用你的控制流替换它」的抽象。

## 不要搭建框架层

不要 `type Stage<T>`、不要 `fn fail<T>`、不要 `fn load_stage`、不要 `fn drain`/`drain_one`、
不要 `run_stage` 编排器。那是把 rxrust 掰成跑你自制控制流的框架（`rust.md` 禁止；它是本项目反
复返工的惯常根源）。直接在源上组合**一条冷算子链**、以**单一** `subscribe` 收尾。
Observable 类型的*函数边界*（`fn lex(input) -> BoxedObservable<Event>` 然后
`fn exec(events: BoxedObservable<Event>) -> BoxedObservable<Rule>`）是同一个层磨去了棱角：
每条这样的缝都要付一个 `box_it()`，把链变成一个只会滴答**我自己的**机器的外壳。让链只住
一处（入口），让 helper 移动**值**、而不是 observable。签名真的无法点名类型时 `box_it()`
依然合法。同样禁止：递归 `stream(node)` 走物化 AST 去构建嵌套 observable —— 那是同一种
事后改造换了个戏服。

## 选 Local 还是 Shared

- `Local` —— 单线程、无 `Send`/`Sync`；与 `!Send` 数据同用。非计时算子就地运行，所以同步链
  不需要 runtime。
- `Shared` —— 要求 Item/Err/闭包/捕获都是 `Send + Sync + 'static`。本项目的 OpenSpec **选择了**它，
  所以跟随这个决定而不是条件反射；并记住「选了它」不等于「得到并发」：native 的
  `SharedScheduler` 是 `tokio::spawn`，但一个没有 `await`、只有微秒级工作的逐项任务是在派生它
  的工作线程上被轮询的（实测：32 项共用一个 `ThreadId`）。元素自身携带真工作时扇出才值回
  票价（directives body、模块编译）—— 用 `tracing` 的 thread id 核验，绝不靠断言。
- `.into_shared()` / `.into_local()` 只换外层包装；非计时源仍就地运行。

## 执行模型（什么同步运行）

- `from_iter`、`of`、`create`、`empty`、`defer`、`from_fn` 及所有非计时组合子
  （`map`/`filter`/`scan_map`/`reduce_initial`/`collect`/`flat_map`/`concat_map`/`merge`/`zip`/
  `tap`…）**在 `.subscribe()` 期间完全就地运行** —— 紧随其后的 `rx.recv()` 无需 runtime 即可工作。
- 只有**计时**算子（`delay`/`timer`/`interval`/`buffer_time`/`debounce`/`sample`/
  `throttle`/`observe_on`/`subscribe_on`）使用 scheduler。**同步管线中避免它们。**

## 元素按值 vs 按引用（写之前检查）

- `map`/`filter_map`/`flat_map`/`scan_map`/`reduce` 按**值**取元素（移动）。
- `filter`/`tap` 取 `&Item`。`scan_map` = 每项 `(&mut Acc, Item) -> Out`；
  `reduce_initial` = `(Out, Item) -> Out` 最终一次。

## 最小源形状（一个基于 `create` 的 lexer，不是 driver）

```rust
use std::convert::Infallible;
use rxrust::observer::Emitter;          // 不在 prelude —— 显式导入
use rxrust::prelude::*;

Local::create::<i32, Infallible, _, _>(|emitter: &mut dyn Emitter<i32, Infallible>| {
    emitter.next(1); emitter.next(2); emitter.complete();
})                                       // 返回 () = 一个 Subscription
.collect::<Vec<_>>()
.subscribe(|v: Vec<i32>| { /* … */ });
```

## 已在本仓定版的单一源（再加一份拷贝就是回归）

下面每一项都曾被复制过，且每次复制都产出一个实测过的错误答案，所以写 helper 前先检查已有实现：

| 法律 | 住在 | 复制它的代价 |
|---|---|---|
| 可换算单位（族 + 因子） | `crate::unit::UNITS`（经 `unit::conversion`） | `math.rs` 保留了十位小数的长度字面量；`hypot(3cm, 4mm*10, 5q*40, …)` → `11.6189500386cm` 无法作答（`q` 只存在于共享表） |
| 单位律下比较两数 | `builtins/math.rs::compare`（+ `comparable` / `compare_named` 两种渲染） | 两种消息形状差点长出两套比较；`min`/`max` 谁都不点名，而 `clamp`/`hypot` 两个参数都点名 |
| `change`/`adjust`/`scale` 通道写入 | `builtins/color/write.rs::route(given, writing)` × `write::Space`（签名层 `COLOR_{CHANGE,ADJUST,SCALE}` 留在 `builtins/color/modify.rs`） | 用 rgb 通道读法读 scale 量会让 `100%` 意为 255 字节、看着像公式错 |
| 某空间自己的通道名、以及每条通道的百分比基数 | `crate::color::channels::{names, index, is_hue, percent_divisor}` —— `Space::percent_full` 就是它 `channel = 0` 那行，`color/text.rs` 的打印后缀也读它 | 基数是每**通道**的、不是每空间的（`lab` 的 lightness 是 `100`，其 `a` 是 `125`），而一个数字写两处正是曾让 `oklab(100% 0 0)` 一度变成 `L = 100` 的原因 |
| 「必须无单位」的拒绝 | `builtins/math.rs::unitless` | 逐函数的措辞漂移 |
| 数字打印 | `value.rs::format_number`（10 位小数、保留 `-0` 的符号）+ 非有限值的 `Unit::calc_product` | `Unit::calc_form` 打印 `NaNdeg`；两条打印法律就是那个 bug |
| 带模块前缀的名字 | `lexer::Tok::MemberVar`（在 `$` 的起始字符处决定） | 拆成 `Word("math.")`+`Var("pi")` 下游无法接回：`math.$pi`、`math . $pi` 与 `5.$pi` 无从分辨 |
| 一个词素把括号深度移动多少 | `lexer::Tok::net`，由 `brackets::depth_of` 在一段 run 上折叠 | 同一条法律五份拷贝，且截参数那份缺 `Punct('(')` 臂 —— 独立分组的 `(` 记 0 而 `)` 记 `-1`，于是 `( … )` 内的逗号可能被读成顶层。语料无一行区分旧读与新读（`changed=0`），这正是法律住在词素上、而非各消费方的原因 |
| hue 是周期的 | `axes::axes_to_rgb`（扇区来自 `(hue / 60).rem_euclid(6.0)`），角度实参在读时由 `color::angle_of` 折叠 | whiteness/blackness 分支把两个已折叠的 hue 相加、把 410 交给转换、取了**最后**一个扇区而不是 90 的 —— 绿与蓝对调了；同一文件已用 `rem_euclid(2.0)` 折叠过，所以回绕是已知的、只是被部分应用 |
| `@for` 的 body 在哪结束 | `structure::MsgAcc{depth, cut}`，它发一个 owned `Event::For{head, body}`；头的两个读者（`loops::is_for` 与 `loops::for_loop`）都经 `brackets::meaningful` 和一个 `for_keyword` | `Exec` 在自探自己的疆域：一个 `Vec<Event>` 缓冲、一个 `open` 花括号计数器和一个重入 `step` 的 `replay`，循环体内的块被当作惰性丢掉（CSS 比源码少、通过数看不见）。读花括号的相已把深度作为数据携带 —— `brackets::Run{level, owner}` 是同形状给圆括号。拆分**不**带来的：逐值展开仍是 `exec::unroll` 里的再喂、不是 body 子流上的 `flat_map` |
| 颜色在色彩空间之间如何转换 | `color::matrix::{to_xyz_d65, of_xyz_d65}` —— 一条经 XYZ(D65) 的路，3×3 逆矩阵由 primaries 表算出 | 每空间一对手抄矩阵是 13×2 张没人能核的表，而本项目自己的回归正是**转译两次**（在 `BRADFORD`（已含白点上移）之上又叠锥体白比）：`to_space/srgb/lab.hrx::middle` 得出 `a = 20.7` 而非 `10.95`。空间作为 `Color` 上一个 owned `InSpace` 携带；`space = None` 让每条 Color-4 之前的法律按原样裁决旧颜色 |

相关纪律（不是关于 rxrust 的，但在此实测）：**一个数字只从打印它的命令引用**，
**一台指向错位仪表的 0 读数不是发现**（`.hrx` 是归档 —— 磁盘上没有 `input.scss` 可供 grep）。
这些守卫应如何挺过新会话，见
`docs/superpowers/specs/2026-10-28-cross-session-anti-gc-regression-design.md`。

## Do / Don't 回顾

- 移动值；`scan`（状态**就是**发射项）或 `scan_map`/`reduce_initial`（私有累加器）承载累加；
  只经框架的终点关闭 —— `collect`/`reduce_initial` + `into_future()`、`into_stream()`，
  或 `await` 订阅句柄。`std::sync::mpsc` 是为需要 `Sender` 的 API 准备的*桥*，
  **不是**终点。
- 一到多 = `Subject` / `publish().ref_count()`（rc.6 **没有 `share()`**）；
  循环 = `flat_map`；分支 = `filter`；状态 = owned 累加器值。
- 错误 = in-stream `Result` + `Err = Infallible`；单用 `subscribe(closure)` 会吞掉 `error`/`complete`。
- **绝不**把 rxrust 包进 `Stage/fail/drain` 框架层、`from_fn`+`VecDeque`/手写状态机或递归 AST
  解释器 —— 那些都意味着「改造框架」，会被拒。
- 无 Python/脚本；无 `println!`（用执行闭包内记录的 `tracing` span）；
  测试在 `tests/`（`#[test]`，无内联）；文件 ≤ 500 行。
