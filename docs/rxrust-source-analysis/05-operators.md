# 05 · 算子全目录（约 70 个）

出处：`observable.rs` 里 `Observable` trait 的方法（签名行号见括注），实现落 `ops/*.rs`。
每条给：**签名约束 / item 传递 / 发光时机 / 错误·完成传播 / 附加要求**。
所有算子统一遵循"源在下游、算子在上游"的 `transform`+下传组合（见 `01 §3`、`03`）。

## A. 订阅 / 收口（`observable.rs:144,157`）
- `subscribe(closure)`：**仅 `Err=Infallible`**；用 `FnMutObserver`，丢 error/complete（`03 §一.3`）。
- `subscribe_with(observer)`：给自定义 `Observer`/`Subject`，全事件可达。
- `into_future()`（`:2122`，`ops/into_future.rs`）：把流变 `Future<Output=Result<Item,Err>>`——
  取**完成时的最后一个值**；空→Err；首个 error→Err。是"异步收口 + 拿错误"的正规口子。
- `into_stream()`（`:2159`，`ops/into_stream.rs`）：把流变 `futures_core::Stream`（逐项 await）。

## B. 无状态变换
- `map(f)`（`:170`，`map.rs`）：`F: FnMut(Item)->Out`，**Item 按值**，1:1 变换；`Err` 透传；`Out` 即新 `Item<'a>`。
- `map_to(v)`（`:197`）：每项映射成常量 `v`（`v: Clone`）。
- `filter(p)`（`:227`，`filter.rs`）：谓词 `FnMut(&Item)->bool`，observer 内部仍收 owned；假项直接吞、不发光；完成/错误照传。
- `filter_map(f)`（`:370`，`filter_map.rs`）：`FnMut(Item)->Option<Out>`，合并 map+filter。
- `tap(f)`（`:1462`，`tap.rs`）：旁路观察（`FnMut(&Item)`），不改数据不改时序——埋点/日志用。

## C. 有状态（累加子在 observer 内，禁 `Arc<Mutex>`）
- `scan_map(acc0, f)`（`:405`，`scan_map.rs`）：`FnMut(&mut Acc, Item)->Output`，**Acc 无需 Clone**，
  每 item 发一个 Output。状态与输出解耦 → **首选的"带状态映射"**。
- `scan(out0, f)`（`:437`）：`FnMut(Output, Item)->Output`，`Output: Clone`，发**滚动累加值**（每步）。
- `reduce(f)`（`:2204`，`reduce.rs`）：`FnMut(Item,Item)->Item`，**无种子**，首项当种；
  **只在 complete 时发一次**最终值；空源→不发光（`reduce.rs:110-127`）。
- `reduce_initial(seed, f)`（`:2250`）：**有种子**；空源发种子。`sum`/`count`/`max`/`min`/`average`
  皆由此特化（`observable.rs:2278-2436`）。
- `collect::<C>()`（`:1573`，`collect.rs`）：`C: Default + Extend<Item>`，**complete 时发一次容器 `C`**。
- `collect_into(init)`（`:1603`）：往给定容器 extend，完成时发出。
- `pairwise()`（`:823`，`pairwise.rs`）：发相邻二元组 `(prev, cur)`，需 `Item: Clone`。
- `last()`（`:576`）/`first()`（`:483`＝`take(1)`）/`last_or(d)`/`first_or(d)`：取端点值，发一次。

## D. 计数/条件裁剪
- `take(n)`（`:463`，`take.rs`）：前 n 个后 complete（触发 `is_closed`，让上游 `break`）。
- `take_last(n)`（`:558`）/`skip(n)`（`:535`）/`skip_last(n)`（`:629`）：缓冲式尾裁剪 / 头部跳过。
- `take_while(p)`（`:714`）/`take_while_inclusive(p)`（`:741`）/`skip_while(p)`（`:652`）：谓词裁剪。
- `contains(target)`（`:687`）：`Item: PartialEq`，发一个 `bool`。
- `start_with(vals)`（`:1518`）：在原序列前插若干值。
- `default_if_empty(d)`（`:1544`）：空源则发默认值。
- `finalize(f)`（`:1491`，`finalize.rs`）：无论正常完成/出错/取消，都执行 `f`（teardown）。

## E. 去重
- `distinct()`（`:252`）：全局去重，`Item: Eq+Hash+Clone`（内部 `HashSet`）。
- `distinct_key(kf)`（`:281`）：按 key 去重。
- `distinct_until_changed()`（`:305`）：只去**相邻**重复，`Item: PartialEq+Clone`。
- `distinct_until_key_changed(kf)`（`:335`）：按 key 的相邻去重。

## F. 高阶 / 合流（流中流——项目"把 loop 交给 rxrust"的正解）
- `flat_map(f)`（`:2458`，`flat_map.rs`）：`F: FnMut(Item)->Inner`，
  `Inner: Context<Inner: ObservableType<Err=Self::Err>>`。**实现 = `MergeAll{source:Map{source,f},
  concurrent:MAX}`**（`flat_map.rs:46-49`）：即"map 成内层流再全并发合并"。内层共享外层 `Err`；
  跨内层**不保证顺序**；任一层 error 终结下游。→ **循环/嵌套展开就用它，别手写 for/while**。
- `concat_map(f)`（`:2486`）：同上但 `concurrent:1` → 顺序拼接、保内层序（＝`map(f).concat_all()`）。
- `merge_all(concurrent)`（`:907`，`merge_all.rs`）：把"发 observable 的流"压平；`concurrent`
  限并发（`usize::MAX`/`1`/`n`）。`concat_all()`＝`merge_all(1)`（`:916`）。
- `merge(other)`（`:832`，`merge.rs`）：两流并发合流，全完成才完成。
- `zip(other)`（`:1436`，`zip.rs`）：按序配对 `(a,b)`，其一结束即结束。
- `combine_latest(other, f)`（`:851`，`combine_latest.rs`）：任一更新时用另一最新值算，需各自已至少发一值。
- `with_latest_from(other)`（`:887`）：源发时用 other 的最新值组合。
- `switch_map(f)`（`:2519`，`switch_map.rs`）：内层来新就**取消上一个内层**，只跟最新支（`Inner` 需 `'static`）。
- `group_by(keyf)`（`:944`，`group_by.rs`）/`group_by_mut_ref`（`:968`）：按键分流为 `GroupedObservable`。

> 合流/高阶算子的取消协调见 `merge_all.rs`：`MergeAllControl{closed,limit,queue:VecDeque,
> outer:Option<H>, inners:DynamicSubscriptions}`；每个上游订阅前先拿 `SingleAssignment` 槽
> （`03 §二.3`），回调/清理都在锁外，"取消不漏掉正在订阅的上游"。

## G. 时间 / 调度（**会走 Scheduler，需 runtime**；wasm32/Local 用 `LocalScheduler`）
- `delay(d)`/`delay_at`/`delay_with(sch)`（`:1044-1064`）：整体推迟发光。
- `delay_subscription*`（`:1072-1091`）：推迟**订阅**上游的时刻。
- `delay_unsubscription*`（`:1135-1147`）：推迟取消。
- `debounce(d)`（`:1175`）/`debounce_with`：静默期后才发最新值。
- `throttle(f)`/`throttle_time*`（`:1321-1364`，`throttle.rs`，`ThrottleEdge`）：开窗节流。
- `sample(notifier)`（`:1408`，`sample.rs`）：按 notifier 节拍取源的最新值。
- `observe_on(sch)`（`:1200`）：下游观察改到别的调度器执行。
- `subscribe_on(sch)`（`:1214`）：上游订阅改到别的调度器发起。
- `buffer(notifier)`（`:1631`）/`buffer_count(n)`（`:1658`）/`buffer_time(d)`/`buffer_time_max`（`:1678-1762`）：
  攒批发 `Vec`（`Item` 变 `Vec<原 Item>`）。

## H. 错误 / 生命周期 / 重试
- `on_error(f)`（`:985`，`lifecycle.rs`）：消费错误、把 `Err` 变 `Infallible`（之后可 `subscribe` closure）。
- `on_complete(f)`（`:1033`）：完成时执行副作用。
- `map_err(f)`（`:1023`，`map_err.rs`）：转换错误类型。
- `retry(policy)`（`:1282`，`retry.rs`，`RetryPolicy`）：按策略重新订阅上游。

## I. 多播 / 类型擦除
- `multicast(subject)`（`:1819`）/`multicast_mut_ref`（`:1874`）：接 `Subject` 得 `ConnectableObservable`。
- `publish()`（`:1927`）＝`multicast(Subject::default())`；`publish_mut_ref`（`:1975`）。
  之后 `.fork()` 多路订阅、`.connect()` 手动启动、`.ref_count()` 首订阅自动连/末退订自动断（见 `06`）。
- `box_it()`（`:2002`，`box_it.rs`）：擦成 `LocalBoxedObservable<'_,Item,Err>`；
  `box_it_clone()`（`:2025`）可多次订阅；`box_it_mut_ref*`（`:2039/2071`）对应 `&mut Item` 链。

## J. 与异步世界互转
- 入向：`from_future*`/`from_stream*`（`04`）。
- 出向：`into_future`/`into_stream`（本篇 A）。

---

## 选算子速查（写管线时）

| 需求 | 用 |
|------|----|
| 逐项转换 | `map` / `filter_map` |
| 只留满足的 | `filter` |
| 带状态、每步出值 | `scan_map`（Acc 免 Clone，首选） |
| 折叠成一个终值 | `reduce` / `reduce_initial` / `collect` |
| 一项变多项 / 循环 / 嵌套块 | `flat_map`（并发）/ `concat_map`（保序） |
| 只跟最新一支 | `switch_map` |
| 两份产出喂多下游 | `publish().ref_count()` / `Subject`（`06`） |
| 跨函数边界传流 | `box_it()`（`07`） |
| 收口拿结果+错误 | `into_future()` 或 `on_error`+`subscribe` 或终点 mpsc（`08`） |
| 时间/节流/缓冲 | G 组（记得需要 scheduler） |
