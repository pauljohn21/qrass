# 05 · 算子全目录（`ops.rs` 实测 51 个模块）

出处：`observable.rs` 里 `Observable` trait 的方法（**签名行号已逐条核对**，rc.6），实现落 `ops/*.rs`
（`ops.rs:32-82` 列出 **51** 个模块；旧版本此处写"约 70 个"是错的）。
每条给：**约束 / item 传递 / 发光与完成时机 / 取消与错误传播 / 附加要求**。

统一骨架（每个算子都长这样，见 `01 §3`）：`struct Op{source, params}`（惰性数据、`Clone`）
+ `struct OpObserver{observer, params}`（包住下游）+ 三个 impl（`ObservableType` / `CoreObservable<C>`）；
方法体一律 `self.transform(|core| Op { source: core, .. })`。

## A. 订阅 / 收口

- `subscribe(closure)`（`observable.rs:144-151`）：**只对 `Err = Infallible` 可用**（文档 `:141-143` 明说），
  内部 `swap(FnMutObserver(f))`（`:149`）→ **静默丢弃 `error`/`complete`**（`observer.rs:266-273`）。
- `subscribe_with(observer)`（`:157`）：给自定义 `Observer`/`Subject`，三事件齐全。
- `into_future()`（`:2122`，`ops/into_future.rs`）：**不是"取最后一个值"**。语义（`into_future.rs:8-15`、
  状态机 `State::{Empty,HasValue,MultipleValues,Error}`）：
  恰好一个值 → `Ok(Ok(v))`；有错误 → `Ok(Err(e))`；零值完成 → `Err(Empty)`；**多于一个值 → `Err(MultipleValues)`**。
  进入 `MultipleValues`/`Error` 后 `is_closed()` 变真，源头停发（`:220-224`）。
  ⚠ 它**丢弃订阅句柄**（`:259-261` 注释 "we currently drop the subscription handle"）⇒ 不能取消。
- `into_stream()`（`:2159`，`ops/into_stream.rs`）：产出 `Stream<Item = Result<Item, Err>>`；
  错误发一次后结束（`Some(Err)` → `None`）。⚠ **`Drop` 才 `unsubscribe`**（`into_stream.rs:123-129`），
  不 Drop 就一直挂着订阅。

## B. 无状态变换

- `map(f)`（`:170`）stable 分支 `F: for<'a> FnMut(Self::Item<'a>) -> Out`；`nightly` 分支
  `FnMut<(Self::Item<'a>,)>` 以支持**借用型输出**（`ops/map.rs:49-107`；`lib.rs:1-6` 解释为何不换成自造 callable trait）。
- `map_to(v)`（`:197`）：`B: Clone`，每项换成常量。
- `filter(p)`（`:227`）：谓词 `for<'a> FnMut(&Self::Item<'a>) -> bool`（收 **`&Item`**），
  owned 值只在放行时转发（`filter.rs:54-63`）；`error/complete/is_closed` 直通。
- `filter_map(f)`（`:370`）：`FnMut(Item) -> Option<Out>`，`None` 直接吞。
- `map_err(f)`（`:1023`）：`F: FnOnce(Err) -> OutErr`；**惯用法**：把 `Infallible` 源对齐到公共 `Err`
  以并入合流（`map_err.rs:100-107` 的测试就是这么做的）。
- `start_with(vals)`（`:1518`）：**不包 observer**，直接 `context.inner_mut()` 先插发（边发边查
  `is_closed`），再 `source.subscribe(context)`（`start_with.rs:53-66`）。

## C. 有状态（累加子在 observer 内）

- `scan_map(acc0, f)`（`:405`）：`FnMut(&mut Acc, Item) -> Output`，**`Acc` 免 Clone**；每 item 发一个 Output。
- `scan(out0, f)`（`:437`）：`FnMut(Output, Item) -> Output`，`Output: Clone`（每步要 clone 累加值，
  `scan.rs:68-71`）——**累加值本身就是要下发的数据**时用它。
- `reduce(f)`（`:2204`）：**无初值**（首项当种），只在完成时发一次；空源**不发**。
- `reduce_initial(seed, f)`（`:2250`）：有初值；空源发种子。`sum`/`count`（`:2347`/`:2382`）就是它的特化。
- `collect::<C>()`（`:1573`）/`collect_into(init)`（`:1603`）：`C: Extend<Item>`（`Default` 只是便利入口）。
  ⚠ **`error` 时不发容器**，只转发错误（`collect.rs:54` + 测试 `:139-150`）；容器在 `complete` 时才发。
- `last()`（`:576`）/`first()`（`:483`＝`take(1)`）/`first_or`/`last_or`（`:510`/`:605`）：
  `first_or`/`last_or` 直接嵌 `DefaultIfEmpty{source: Take{..}|Last{..}, ..}` 结构体组合，不是新算子。
- `pairwise()`（`:823`）：发 `(prev, cur)`，`Item: Clone`。

## D. 计数 / 条件裁剪

- `take(n)`（`:463`）：状态与**上游句柄同处一个 observer**：`active: Option<(U, O)>`，够数就
  `observer.complete()` + `upstream.unsubscribe()`（`take.rs:70-73`）；`n==0` **根本不订阅源头**（`:113-117`）。
- `take_last(n)`（`:558`）环形缓冲；`skip(n)`（`:535`）；`skip_last(n)`（`:629`）。
- `take_while(p)`（`:714`）/`take_while_inclusive(p)`（`:741`）：同一结构体的 `inclusive: bool` 两入口。
- `take_until(notifier)`（`:770`）/`skip_until(notifier)`（`:799`）：**notifier 的 error/complete 被忽略**，
  只有它"发一个值"才起作用；下游观察者与 notifier 句柄放 `C::RcMut<Option<Inner>>` +
  `done: C::RcCell<bool>`（`take_until.rs:58-78`），故两侧终止不会互相误取消（`notifier_done` 判定 `:101-110`）。
  `skip_until` 用同一个 `RcCell<bool>` 表达"还在跳过吗"（`skip_until.rs:63-86`）。
- `contains(target)`（`:687`）：命中即发 `true` 并 complete + 取消上游；源自然完成发 `false`。
- `default_if_empty(d)`（`:1544`）、`finalize(f)`（`:1491`：观察者与 `Unsub` 共用
  `C::RcMut<Option<F>>` ⇒ **恰好一次**；链式 finalize 是**外层先执行**，`finalize.rs:221-233`）。

## E. 去重

- `distinct()`（`:252`）`Item: Eq+Hash+Clone`（`HashSet` 在 observer）；`distinct_key`（`:281`）。
- `distinct_until_changed()`（`:305`）/`distinct_until_key_changed`（`:335`）：只比相邻，
  比较走 `self.last.as_ref() != Some(&value)`（免克隆比较、只在需要时存，`distinct_until_changed.rs:66-71`）。

## F. 高阶 / 合流

- `flat_map(f)`（`:2458`）＝**结构体拼装**：`FlatMap::new(source, f, usize::MAX)`，其
  `subscribe` 里直接 `MergeAll{ source: Map{source, func}, concurrent }.subscribe(ctx)`
  （`flat_map.rs:46-49`）。`concat_map`（`:2486`）只差 `concurrent: 1`。
- `merge_all(concurrent)`（`:907`）：内层必须是 `Context<Inner: ObservableType>`，故发出的是"带 scheduler 的流"；
  `Item<'a>` 取内层的 `Item`（`merge_all.rs:56-66`）。⚠ **排队期间内层的值会丢**：并发满时第三个内层
  只入队，等某个内层完成才订阅（测试 `test_merge_all_concurrency` 钉住 `s3.next(3)` 丢失）。
  锁纪律（`merge_all.rs:1-14` 文件头）：下游 observer 只在投递时持锁；`MergeAllControl`
  **从不在调用用户代码时持有**；每个上游（外层与每个内层）先取 `SingleAssignment` 槽再订阅。
  `retire`（丢弃槽位 ≠ 取消）与 `outer_terminated`（外层自己结束就不取消它）分别在 `:175-185`。
- `merge(other)`（`:832`，两路）：全完成才完成；错误只取消**仍活跃**的对侧（测试
  `terminal_notifications_do_not_cancel_already_terminated_inputs` 用 2×2 穷举钉住）。N 路用
  `merge_observables` / `concat_observables`（`factory.rs:984-1063`，同样拼 `MergeAll`）。
- `zip(other)`（`:1436`）：两侧 `VecDeque` 缓冲；完成判定是"**某侧已完成且其缓冲为空**"
  （`zip.rs:65-75`），不是"任一侧结束就结束"。
- `combine_latest(other, f)`（`:851`）：任一侧更新即算；两侧都完成才完成；
  **某侧完成且它没有 latest → 立即完成并取消对侧**（`should_complete`，`:77-81` + 测试）。
  投递下游时观察者被临时取走，此期间到达的终止进 `pending_terminal`，`restore` 时补发（`:82-101`）。
- `with_latest_from(other)`（`:887`）：只在主源发光；副源没值就丢弃主源值；
  **副源完成既不终止下游也不取消副源**（`:199-202` + 测试 `secondary_completion_keeps_value_without_cancelling_it`）。
- `switch_map(f)`（`:2519`）：`generation`（`switch_map.rs:75,96,136,158-171`）隔离旧内层——**旧代际的
  通知既不能发光也不能结束新内层**（`:223-284` + 测试 `old_generation_notifications_cannot_finish_new_inner`）；
  内层同步完成只 `retire` 不取消（`synchronous_inner_completion_retires_late_handle`）。
- `group_by(keyf)`（`:944`）/`group_by_mut_ref`（`:968`）：每个 key 一个 `Subject`；
  发下去的元素是 `Ctx::With<GroupedObservable<..>>`（自带 scheduler）；外层 error/complete
  会先广播给所有分组再终结自己（`group_by.rs:146-186`）。

## G. 时间 / 调度（走 `Scheduler`，需 runtime；`wasm32` 亦可用）

- `delay(d)`/`delay_at`/`delay_with`（`:1044/1054/1064`）：每条 `next` 一个 Task，
  `DynamicSubscriptions<TaskHandle>` 记账。⚠ **文档与实现不一致**：`observable.rs:1042-1043` 说
  "error/completion NOT delayed"，实现是**`error` 立即转发（先取消在飞任务）**、
  **`complete` 也进延迟队列**（`delay.rs:127-143`，测试
  `test_delay_subscription_stays_open_until_delayed_complete` 显示句柄在完成延迟到点前 `is_closed()` 为假）。以实现为准。
- `delay_subscription*`（`:1072/1081/1091`）：推迟"订阅源头"这一动作；句柄是
  `C::RcMut<EitherSubscription<TaskHandle, S::Unsub>>`——先持 TaskHandle，真正订阅后**换成源句柄**
  （`delay.rs:205-257`），因此延迟期内取消也能生效。
- `delay_unsubscription*`（`:1135/1147`）：取消到达即**停投递**，把**上游 `unsubscribe()` 变成一个延迟任务**；
  嵌套延迟各自从"取消到达本算子"起计时；自然终止后再取消则立刻透传；普通 `drop` 不取消（`delay_unsubscription.rs` 测试集）。
- `debounce(d)`（`:1175`）/`debounce_with`：每次发光取消上一个待发任务并重排；
  源 `complete` 时**同步补发**尾部值再完成（`debounce.rs:111-123`）。
- `throttle(f, edge)` / `throttle_time*`（`:1321/1351/1364`）：窗口由 notifier 决定
  （时间版就是把 `C::RcMut<Option<Duration>>` 当参数、内部造 `Timer`）；`ThrottleEdge::leading/trailing/all`；
  trailing 时源完成会**延后到窗口结束并补发**；notifier error 取消源；notifier complete 关窗并开新窗。
- `sample(notifier)`（`:1408`）：notifier 每次发光取一次最新值（**无值就什么都不发**，也不发空）；
  notifier 完成 → 补发缓存值并完成；**源完成 → 直接完成，不补发**（`sample.rs:170-209`）。
- `observe_on(sch)`（`:1200`）：把**每次投递**排到该调度器；每投递一个 Task，句柄进
  `DynamicSubscriptions<TaskHandle>`，处理器内自删；排任务前先 `observer.is_closed()` 短路。
  `Unsub = SourceWithDynamicSubs<..>`。
- `subscribe_on(sch)`（`:1214`）：把**订阅动作本身**排到该调度器（`Task<Option<(S, C)>>` +
  `fn subscribe_task`），`Unsub = TaskHandle`；未执行前取消则源永不启动（测试钉住）。
- `buffer(notifier)`（`:1631`）：notifier 发光即发一批（**空批也发 `vec![]`**）；源完成发剩余并取消 notifier；
  notifier 先完成则补发剩余；**源已终止时 notifier 压根不会被订阅**（`buffer.rs:224-233`）。
  `buffer_count(n)`（`:1658`）按计数；`buffer_time`/`buffer_time_max`/`*_with`（`:1678-1762`）用自续
  `TaskState::Sleeping` 周期刷。

## H. 错误 / 生命周期 / 重试

- `on_error(f)`（`:985`）：`F: FnOnce(Err)`，**消费**错误 ⇒ 流的 `Err` 变 `Infallible`（`lifecycle.rs:31-40` +
  测试 `test_on_error_consumes_error_type` 用 `assert_infallible` 钉住）。⚠ 方法文档 `:982-984`
  说"保留错误流向下游"，**与实现相反**，以实现为准。
- `on_complete(f)`（`:1033`）：完成时执行副作用，链式多层都触发（内→外顺序）。
- `retry(policy)`（`:1282`）：`RetryPolicy<Err>` 可由 `usize` / `RetryConfig`（`count`/`delay`/`reset_on_success`）
  / 自定义策略实现；**`RetryObserver` 自带 `source` + `subscribe_fn: fn(Self)`**（注释：避开循环约束），
  串行句柄 `C::RcMut<Option<C::BoxedSubscription>>` 既当 `Unsub` 又负责"重订阅前取消上一次"。

## I. 多播 / 类型擦除

- `multicast(subject)`（`:1819`）/`multicast_mut_ref`（`:1874`）/`publish()`（`:1927`）/`publish_mut_ref`（`:1975`）
  → `ConnectableObservable`（**不是 Observable**：`fork()` 取支路、`connect()` 才启动；见 `06`）。
- `box_it()`（`:2002`）/`box_it_clone()`（`:2025`）/`box_it_mut_ref()`（`:2039`）/`box_it_mut_ref_clone()`（`:2071`）。
  擦的是 **kernel**：`BoxedCoreObservable<'a, Item, Err, Scheduler>`（`observable/boxed.rs:86-116`），
  **scheduler 类型也编进类型里**；`Clone` 变体仅在底层链 `Clone` 时存在；`Local*`/`Shared*` 别名在 `:427-470`。
  `DynCoreObservable` 收的是**整个 Ctx**（不是裸 observer），注释 `:22-27` 说明这是为了擦除边界上不漏 scheduler。

## J. 与异步世界互转

入向：`from_future`/`from_future_result`、`from_stream`/`from_stream_result`（`04`）；
出向：`into_future`/`into_stream`（本篇 A）。⚠ 只有 `SharedScheduler` 会 `tokio::spawn`（真多线程），
`LocalScheduler` 走 `spawn_local`；且 `interval_with` 等文档提醒（`factory.rs:594-599`）：
**挂 `SharedScheduler` 时整条链（含 `take` 这类持 `Rc` 状态的算子）必须 `Send`**。

---

## 选算子速查

| 需求 | 用 | 别用 |
|------|----|------|
| 逐项转换 | `map` / `filter_map` | 手搓 `for` |
| 带状态且每步出值 | `scan_map`（Acc 免 Clone） | `Arc<Mutex<..>>` |
| 状态即数据、要下发 | `scan`（`Output: Clone`） | 藏进对象 |
| 折成一个终值 | `reduce` / `reduce_initial` / `collect` | 中途 `collect` 再排序 |
| 一项变多项 / 循环 | `flat_map`（并发）/ `concat_map`（保序） | work-queue、`from_fn` 驱动 |
| 只跟最新一支 | `switch_map` | 手工"取消上一轮"标志位 |
| 一份产出喂多下游 | `publish().ref_count()` / `Subject` | 缓存后逐个重放 |
| 跨函数边界存链 | `box_it()` | 自造 `type Stage<T>` |
| 异步收口 | `collect()`+`subscribe`、`into_future().await`、`await` 订阅句柄（`from_*` 返回的 `TaskHandle`） | `std::sync::mpsc` 套壳 |
| 时间/节流/缓冲 | G 组（需要 scheduler） | 手写 `Instant` 轮询 |

> 取消、锁与"终止只投一次"的次序不变量单列在 `09-cancellation-and-terminal-ordering.md`。
