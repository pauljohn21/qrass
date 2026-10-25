# Tasks — 从零手写 rxrust 教程（V0 → V6，按卷推进；每卷末以 docsizer 报告为闸）

> 阶段可跨多次提交；每课交付前 MUST 完成 `VERIFICATION.tsv` 累计编译验证。
> 未归属行 MUST 排入后续课，不得丢弃（`coverage.unassigned` 单调下降到 0）。

## T0 — 验收工具与账本（先建闸，再写作）—— **已完成**

实测基线：88 文件 / 29,755 行 / **可教 14,116**；`plan --target 300 --max 600` → **58 课 / 103 段**；
`coverage = 100.00%，unassigned=0，duplicates=0`；现存草稿非代码字数 7,613（待按新契约重写）。

- [x] 0.1 新增 `crates/docsizer`（Rust bin，只读；禁 `println!`，用 `tracing`；`src/` 纯产码、测试放 `tests/`）
- [x] 0.2 `docsizer inventory --src <rxrust-1.0.0-rc.6>/src`：可教行清单 + 排除原因计数（test 模块 / 空行 / 纯 `use` / 纯括号，逐类可对账）
- [x] 0.3 `docsizer plan --map MAP.tsv` + `docsizer coverage --ledger COVERAGE.tsv`：`assigned/teachable`、`unassigned`、`duplicates`
- [x] 0.4 `docsizer words --dir docs/rxrust-from-scratch`：非代码字数（剔 ``` 代码块与表格分隔行、不计空白）+ 总字数两口径
- [x] 0.5 `docsizer` 的 `tests/` 用真实小样本断言四类行为（test 块整块排除 / plan→coverage 干净账 / 未归属与重复都能报 / 代码与表分隔不计正文）；bin 名歧义已避（`--bin docsizer`）
- [x] 0.6 建立 `MAP.tsv`（88 文件→ 7 卷）+ `COVERAGE.tsv`（58 课预分配，100% 覆盖）+ `VERIFICATION.tsv` / `GLOSSARY.md` 骨架
- [x] 0.7 验证工装位置：**MUST 放在 workspace 之外**（实测：`/tmp/minirx-check`；课文代码则放
      `docs/…/code/` 入库、`cp` 出去编译）。实测坑：在 `target/` 下 `cargo new` 会被 cargo 改写根 `Cargo.toml` 的
      `workspace.members`（已发生并复原），故验证 crate MUST NOT 建在仓库目录树内

## V0 — 零基础前置卷（约 5 课，≈5 万字；不覆盖源码）

> 密度实测：V0-01 非代码字数 0.74 万 → 本卷按每课 0.7–1 万、共 ≈5 万字计（原 12 万估计偏大，不拿灌水凑）。

- [x] 1.1 所有权与借用：为什么"值被搬走"而不是"值被引用"（为 V1 的 `Item` 按值铺路）——
      `v0-foundations/00-01-ownership-and-borrowing.md`；代码 `code/00-01-ownership*.rs` 已外部验证
      （6 测通过、clippy 0 警）；4 个编译错均为实测原文（E0382×2 / E0308+E0277 / E0106）
- [x] 1.2 trait 与泛型：trait 边界、关联类型；**手写一次再改写成 GAT** 的对照实验——
      `v0-foundations/00-02-traits-associated-types-and-gats.md`；代码外部验证（4 测通过、clippy 0 警）；
      报错链均为实测：E0119 → missing lifetime in associated type → missing required bound on `Row`，
      外加第一版被拦的 redundant closure 与多余 `mut`
- [x] 1.3 闭包三兄弟 `Fn/FnMut/FnOnce`：为什么 `next(&mut self)` 用 `FnMut`、终止方法为何消耗 `self`
- [x] 1.4 `Rc/Arc/RefCell/Mutex` 与内部可变性：共享 vs 共享可变；object-safety 初探（为 V1 `DynObserver` 铺路）——
      `v0-foundations/00-04-shared-ownership-and-interior-mutability.md`；代码外部验证（5 断言 + 1 答案、clippy 0 警）；
      报错均实测：E0277 `Rc` 不能 `Send` / E0599 `derive(Clone)` 要求 `T: Clone` / 运行时
      `RefCell already borrowed`（因此才有 `try_*`）；并逐条核对了 `rc.rs` 的行号引用
- [x] 1.5 宏最小知识：`macro_rules!` 会展开成什么（用 `impl_observer_for_box!` 的真实形态做例子，只看不写）——
      `v0-foundations/00-05-macro-rules-minimum.md`；先实测出全库**只有 7 处** `macro_rules!`
      并列其位置；自写 `impl_sink_for_box!` 复刻 `observer.rs:133-169`（4 次调用在 `:172-178`）；
      5 条错均实测原文（no rules expected ×2 / cannot find macro / E0119 / E0596）+ 拿到
      `coherence_leak_check` 警告从而解释了 `observer.rs:136` 那个属性；代码 6 断言全过、clippy 0
- [x] 1.6 卷末：`docsizer words` 报告 + 术语表并入 + 读者自评清单（能独立回答每课三问）——
      V0 全卷实测 46,149 非代码字（入 README 现状表）；GLOSSARY 已收 V0 五课全部术语；
      自评清单与逐课兑现表写在 `00-05` 末尾“V0 卷末”节

## V1 — 消费者与第一批源头（`observer.rs` 379 行 + `observable/{from_iter,trivial,never,throw_err}.rs`）

- [ ] 2.1 `Observer` 四方法逐行（`observer.rs:19-43`）：终止方法消耗 `self` 的协议意义
- [ ] 2.2 `VecObserver` + 第一个 `from_iter`（`from_iter.rs:29-75`）：发射循环、`is_closed` 早停
- [ ] 2.3 `FnMutObserver`（`observer.rs:255-277`）：`subscribe(closure)` 为何吞 error/complete（本项目实测坑）
- [ ] 2.4 `Emitter` 与 `&mut dyn Emitter`（`observer.rs:49-99`）：零堆分配的类型擦除、防抽象泄漏
- [ ] 2.5 `DynObserver` + blanket impl + `impl_observer_for_box!`（`observer.rs:110-179`）：object-safety 修复逐行
- [ ] 2.6 `IntoBoxedObserver` 与四个 `BoxedObserver*` 别名（`observer.rs:190-244`）
- [ ] 2.7 `Option<O>` 与 `P: RcDerefMut<Target=Option<O>>` 两个 impl（`observer.rs:284-335`）：`take()` 的释放语义
- [ ] 2.8 `of/empty/never/throw_err`（`observable/trivial.rs` 250 行）逐行 + 与 `from_iter` 的差异
- [ ] 2.9 卷末反例节：把 `Observer` 当共享可变 sink 用 / `VecObserver` 进生产码 / 为"复用源头"改成 `&mut self`

## V2 — 订阅与取消（`subscription.rs` 206 行 + `subscription/` 1,048 行）

- [ ] 3.1 `Unsubscribable`/`Subscription`：为什么"取消"是值不是异常
- [ ] 3.2 `SingleAssignment`：状态机（`State`）与"订阅中也能取消"的竞态防护，逐行
- [ ] 3.3 `DynamicSubscriptions`/`MultiSubscription`：组合取消、越界取消安全性
- [ ] 3.4 把 V1 的 `is_closed` 升级为外部 `unsubscribe()`，观察源头真停（含 `tracing` 证据）
- [ ] 3.5 卷末：`type Unsub` 在各源头分别是什么（`()`/`TaskHandle`/…）对照表

## V3 — 线程模型抽象（`context.rs` 732、`rc.rs` 204、`factory.rs` 1,256）

- [ ] 4.1 `RcDeref/RcDerefMut` 与 `MutRc/MutArc`：同一份算子代码在单线程/多线程都成立的根因
- [ ] 4.2 `Context`/`Local`/`Shared`/`IntoContext`：`into_inner`、`lift`、`RcMut` 关联类型逐行
- [ ] 4.3 `factory.rs`：`Local::…`/`Shared::…` 全部构造入口逐个讲（含 `subject`/`from_*`/`create`/`defer`）
- [ ] 4.4 `Send/Sync` 边界实战：让读者故意在 `Shared` 里塞 `Rc` 撞错并读懂报错
- [ ] 4.5 `mut_ref` 家族（`Observer<&mut T>`）：为什么需要、`for<'m>` 从哪来
- [ ] 4.6 卷末：把 `minirx` 升级为 Local/Shared 双模可跑

## V4 — 三层 trait 与算子工厂（`observable.rs` 2,555 行 + 一阶/状态 ops）

- [ ] 5.1 `ObservableType`(GAT) / `CoreObservable<C>` / `Observable: Context` 三层职责逐行
- [ ] 5.2 `transform`/`lift`/`into_inner`：新算子 = 加一对 source+observer 的机械步骤
- [ ] 5.3 一阶算子逐个：`map/map_to/filter/filter_map/take/take_last/take_while(_inclusive)/skip/skip_last/skip_while/first/first_or/last/last_or/contains/distinct*/tap/default_if_empty/start_with/finalize`
- [ ] 5.4 状态算子逐个：`scan`(发射累加值) vs `scan_map`(私藏 Acc) vs `reduce` vs `reduce_initial` vs `collect/collect_into`
- [ ] 5.5 侧带算子：`on_error/on_complete/map_err`（错误通道第一次被真正用起来）
- [ ] 5.6 卷末反例节：把状态藏进自造对象（引用本项目 `scan_map(Fold)` 的返工）+ `.ok()` 吞错

## V5 — 高阶与多播（`ops/{flat_map,merge_all,switch_map,zip,combine_latest,with_latest_from,buffer*,group_by,retry,…}`、`subject*` 2,064+120 行、`observable/connectable.rs` 275 行）

- [ ] 6.1 `merge_all(concurrent)` 全文件逐行：队列、活跃计数、`active_subscriptions`、完成判定
- [ ] 6.2 `flat_map = map + merge_all`（`flat_map.rs`）+ `concat_all=merge_all(1)`；实测顺序/并发差异
- [ ] 6.3 `switch_map` 与取消竞争；`zip`/`combine_latest`/`with_latest_from`（含本项目实测：冷源配到终值）
- [ ] 6.4 `buffer/buffer_count/buffer_time/group_by/retry` 等剩余 ops 逐行
- [ ] 6.5 `Subject` 内核（`subject/subject_core.rs` 2,064 行）：订阅者表、终止后广播、`is_closed`
- [ ] 6.6 `BehaviorSubject`+`peek`、`AsyncSubject`、`ReplaySubject`（如有）逐个与适用场景
- [ ] 6.7 `ConnectableObservable`/`publish`/`ref_count`/`share`/`multicast`：冷→热、连接引用计数逐行
- [ ] 6.8 卷末：`minirx` 实现"一个生产者喂 N 消费者"，与 `Vec`+`for` 手喂做对比实验

## V6 — 调度、类型擦除、异步互操作与组装（`scheduler*` 869+顶层 1,089、`observable/boxed.rs` 470、`from_future` 448、`from_stream` 853、`create/defer/timer/interval`、`ops/{into_future,into_stream,box_it,delay*,observe_on,subscribe_on,sample,throttle,debounce}`、`prelude.rs`、`Cargo.toml [features]`）

- [ ] 7.1 `Task/TaskHandle/TaskState/Schedulable/SleepProvider` 与 `LocalScheduler`(spawn_local)/`SharedScheduler`(tokio::spawn) 逐行
- [ ] 7.2 时间算子：`timer/interval/delay/delay_at/debounce/throttle/sample` —— 为什么只有它们需要 runtime
- [ ] 7.3 `box_it` 与 `BoxedObservable*`（`observable/boxed.rs`）：值擦除 vs 类型别名；`box_it_clone/mut_ref` 全家
- [ ] 7.4 `create` 与 `&mut dyn Emitter`；`defer`；`from_future`/`from_stream`（含 `from_stream_result`）
- [ ] 7.5 `into_future`/`into_stream`：终点如何收口（`IntoFutureError::Empty` 语义），本项目"别再套 mpsc+block_on"的反例落点
- [ ] 7.6 `prelude.rs` 48 行 + `lib.rs` 168 行 + feature 门控（`default=["scheduler"]`、`nightly`）逐个 `pub use`
- [ ] 7.7 测试工装：`test_scheduler`/虚拟时间、`#[rxrust_macro::test]` 形态（读者可用 `#[test]` 等价替代）
- [ ] 7.8 终卷：`minirx` 与 rc.6 的**能力对照表**（模块/算子/别名/差异逐条），"你还差什么"清单

## 收尾（每卷末与整体）

- [ ] 8.1 每卷交付：`docsizer coverage`（`unassigned` 单调下降）+ `docsizer words`（卷下限）+ 累计编译验证记录
- [ ] 8.2 全量交付：`COVERAGE.tsv` 达 `unassigned=0 duplicates=0`；非代码字数 ≥ 1,000,000（预期 ≈135 万，分母 14,116 可教行）
- [ ] 8.3 交叉链接：`docs/rxrust-source-analysis/` 各篇顶部加"配套实战教程见 `docs/rxrust-from-scratch/`"
- [ ] 8.4 不影响产品码：每卷末复核 `sasspile` 的 test/clippy -D/`sasslint count=0`/hrx `regress=0` 仍成立
- [ ] 8.5 README 进度表与实际课次同步；`GLOSSARY.md` 完整可查（首现课号无缺口）
