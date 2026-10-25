# 09 · 取消、锁与"终止只投一次"的次序不变量

本篇是逐行读完 rc.6 的 `ops/*` + `subscription/*` + `subject/*` 后**归纳出的跨算子不变量**，
每条都注明出处（实现行号或钉住它的测试名）。写多源/高阶/共享状态的管线前必读——
这些正是"看着能编译、跑起来偶发丢值或双终止"的来源。

## 1. 句柄：先建槽，后订阅

每个"要被取消的上游"都走同一套（`subscription/single_assignment.rs:15-167`）：

```rust
type Handle<C, U> = SingleAssignment<C::RcMut<State<U>>>;
let (install, [slot_for_peer, my_subscription]) = Handle::<C, U>::channel::<2>();
install(source.subscribe(wrapped_observer));   // 订阅完立刻把句柄装进去
```

- `State` 三态 `Pending(wakers) / Assigned(u) / Cancelled`：**取消早到会被记住**（`unsubscribe` 把状态
  换成 `Cancelled`，`single_assignment.rs:149`），随后到达的 `set()` 命中 `Cancelled` 分支会**立即**
  `unsubscribe`（`:120-140`）。这就是"竞态安全"的全部秘密，不需要外部再加标志位。
- `channel::<N>()` 一次给 N 个共享同一状态的句柄：自己拿一个返回给调用方，其余交给对侧算子
  （`take.rs:110-121`、`zip.rs:133-149`、`combine_latest.rs:151-177`、`throttle.rs:478-502`、
  `take_until.rs:58-78`、`skip_until.rs:63-86`）。
- 多源算子的 `Unsub` 是 `TupleSubscription<a, b>`（只有二元），或
  `SourceWithHandle`（源 + 一个调度句柄）、`SourceWithDynamicSubs`（源 + 一批动态句柄）。
  组合顺序固定：**先取消源，再 drain 动态句柄**（`source_with_dynamic.rs:44-49`）。
- `()` 作 `Unsub` = "同步源，无从取消"（`of`/`from_iter`/`create` 等），且 `is_closed()` 恒真
  （`subscription.rs` + `boxed.rs:179-183`）。

## 2. 终止只投一次：`take()` 掉观察者，而不是布尔标志

框架里判断/保证"恰好一次"的手法只有这几种，照抄即可：

| 手法 | 出处 | 用途 |
|------|------|------|
| `Option<O>` / `C::RcMut<Option<O>>` + `take()` | `observer.rs:284-335`；`merge_all`、`ref_count`、`combine_latest`、`zip`、`sample`、`throttle` | 观察者被取走 ⇒ 之后任何路径都发不出去 |
| `C::RcCell<T: Copy+Eq>` + `compare_exchange` | `delay_unsubscription.rs:39-49,66-72` | 用 CAS 决定"谁来投终止"；多源算子则用 `C::RcCell<SubscriptionState>`（`ref_count.rs:131`）而非裸 `bool` |
| `terminated: bool` + `finish()/restore()` | `combine_latest.rs:50,82-101` | 调用用户代码期间**观察者不在位**，此时到达的终止进 `pending_terminal`，归还时补投 |
| `generation: usize` | `switch_map.rs:75,96,136,158-171,223-284`、`throttle.rs:243-273` | 旧内层/旧通知者的任何通知一律作废 |
| `retire`（丢弃槽位而非取消） | `merge_all.rs:175-185` | 上游**已自然终止**时，只丢记账，绝不 `unsubscribe` |

**判据**：如果你打算写 `if !self.done { self.done = true; .. }`，先自问能不能换成
`Option::take()` 或 `C::RcCell` 的 CAS —— 框架里没有一处用裸 `Cell<bool>` 手写终止门。

## 3. 取消只在"提前终止"时发生

多源算子普遍遵守：**已终止的输入不再被取消**，只取消仍活跃的对侧。

- `merge`：`has_terminated_input` 决定是否取消对侧（测试 `terminal_notifications_do_not_cancel_already_terminated_inputs`，2×2 穷举）。
- `merge_all`：内层出错 → 取消"另一内层 + 仍活跃的外层"，不取消自己（测试
  `inner_error_cancels_peers_but_not_naturally_terminated_input`）；外层自己完成 → 不取消外层（`outer_terminated`）。
- `combine_latest`：某侧完成且另一侧已 `completed` → 不取消已完成侧（`completed_a/completed_b` 判定）。
- `with_latest_from`：副侧完成 → **不取消副侧**，也不终止下游。
- `zip`：完成时只取消仍活跃侧（`!completed_a` / `!completed_b` 判定）。
- `take`：够数才取消（`take.rs:70-73`）；源先自然完成/出错时**只转发、不取消上游**
  （句柄随 `active.take()` 被 drop，`take.rs:76-86`）。

推论：**"结束一条流"的正解是让它自然 complete，而不是到处 `unsubscribe`。**

## 4. 锁的纪律（跨源算子共同的加锁窗口）

`merge_all.rs:1-14` 的文件头注释是整套纪律的规范文本，其他多源算子照同一模式实现：

1. 共享状态一律 `C::RcMut<..>`（`Local`=`MutRc`，`Shared`=`MutArc`），**不包业务对象**，只包协调结构；
2. **只在投递给下游 observer 的那一瞬间**持锁（或把观察者 `take` 出锁后再投递）；
3. `MergeAllControl` / `RefCountControl` 这类控制块**绝不在调用用户代码时持有**；
4. 每个上游订阅前先取得 `SingleAssignment` 槽，"取消永远不会漏掉一个正在订阅的上游"；
5. wake / 清理一律在锁外（`single_assignment.rs:86-96,142-154`：先 `mem::take` 出 wakers，drop 锁再 wake）。

`combine_latest.rs:94-101` 的 `restore()` 是第 2 条的极致版本：投递期间观察者不在位，
所以回来的终止必须被缓存再补投，而不是"认为不可能发生"。

## 5. Subject 的再入政策（最容易踩）

`subject.rs:14-104` + `subject_core.rs:245-286`：

- **广播期间不得再次对同一 Subject 调 `next`/`error`/`complete`** —— 会 panic
  （`subject_core.rs:425`（next）、`:436`、`:447`、`:469`、`:480` —— 宏为值版/`&mut` 版各生一处）。
  需要反馈循环就插 `delay(Duration::from_millis(0))` 造异步边界。
- 广播期间 `subscribe`/`unsubscribe` **允许但可能被推迟**（表被借走时改为调度
  `AddState`/`RemoveState` 任务；CAS `Pending→Ready(id)` 失败即回滚，"已取消的添加永不激活"）。
  测试 `test_subject_{subscribe,unsubscribe}_inside_next_is_deferred`、
  `test_subject_unsubscribe_cancels_pending_subscribe` 钉住这三条。
- `Subject::is_closed()` 的语义是"**当前没有订阅者**"，不是"已终止"（`subject_core.rs:452`：
  `self.observers.rc_deref().inner.is_empty()`）。
  需要"是否已 error/complete"请自己上 `C::RcCell<SubscriptionState>`（枚举定义在
  `subject/subject_subscription.rs:10`，`ref_count.rs:131` 正是这么约束的）。
- 广播是 **n-1 次 clone + 最后一次 move**（`subscribers.rs`）。

## 6. 测试即规范（改语义前先读它们）

rxrust 把"最难说清的时序"写成专测，遇到相关需求先去这些地方核对，别自己发明：

| 想搞清的问题 | 去看 |
|--------------|------|
| 高阶算子排队/并发额度 | `merge_all.rs` 测试 `test_merge_all_concurrency`（排队内层的值会丢） |
| 取消能否取消到"正在订阅"的上游 | `merge_all` `close_*`、`synchronous_inner_completion_does_not_retain_or_cancel_late_handle` |
| 回调里取消整条订阅 | `cancellation_from_callback_never_starts_queued_inner`、`cancellation_inside_inner_callback_is_reentrant` |
| 多播重连/引用计数 | `ref_count.rs` 测试 `cancellation_can_reconnect_without_reusing_the_detached_handle`、`natural_termination_and_old_handles_do_not_cancel_new_connection` |
| 虚拟时间下的取消/延迟 | `delay_unsubscription.rs` 测试集 + `scheduler/test_scheduler.rs`（`advance_by`/`flush`/`pending_count`） |
| 观察者被调期间来了终止 | `combine_latest.rs` `reentrant_error_is_delivered_after_in_flight_next` |
| 旧代际通知能否结束新内层 | `switch_map.rs` `old_generation_notifications_cannot_finish_new_inner` |

## 7. 文档 / 注释与实现不一致处（已核对，以实现为准）

1. `observable.rs:1042-1043`：称 `delay` 不延迟 error 与 completion；实现是 error 立刻转发、
   **completion 同样进延迟队列**（`delay.rs:127-143` + `test_delay_subscription_stays_open_until_delayed_complete`）。
2. `observable.rs:982-984`：称 `on_error` "保留原错误流向下游"；实现**消费**错误并把 `Err` 变成
   `Infallible`（`lifecycle.rs:31-40,56` + `test_on_error_consumes_error_type`）。
3. `into_future` 文档只说"以最后一个值 resolve"；实现要求**恰好一个值**，多值判 `Err(MultipleValues)`，
   并且**丢弃订阅句柄**（`into_future.rs:8-15,259-261`）。

（本条清单只收"文档与实现不一致"；`lib.rs:56` 的 `// Main modules (formerly v1)` 只是历史注释，
不构成废弃声明 —— `Observer`/`Subscription` 仍是权威契约。）

## 8. 给使用侧的三条硬结论

1. **不要再造 `Stage`/`fail`/`drain` 这类控制流抽象**：跨阶段用 `flat_map`，取消用 `SingleAssignment`，
   终止用 `Option::take()`/CAS，共享用 `C::RcMut`——框架每一样都只有一种写法。
2. **别用 `collect` + `sort` 恢复顺序**：要顺序就 `concat_map`/`concat_all`/`concat_observables`；
   `collect` 在错误路径上不发射容器（`collect.rs:54`），会静默丢结果。
3. **异步收口交给句柄**：`from_stream`/`from_future`/`observe_on`/`subscribe_on`/`timer` 的 `Unsub` 是
   `TaskHandle`，而 `TaskHandle` 本身就 `impl Future`（`scheduler.rs:429`）；所以
   `let h = obs.subscribe(f); h.await;` 就是框架自己的"等终止"，或用 `into_future()`
   （注意 §7.3 的限制），别用 `std::sync::mpsc` 套同步壳。
