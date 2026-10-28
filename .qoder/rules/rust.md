---
trigger: always_on
alwaysApply: true
---

## 技术栈与语言
- Rust edition 2024，toolchain 1.99。所有对话用中文，代码/命令/文件名/注释保持 English。
- 以 **rxrust 为核心**（见「铁律」）。版本钉 `=1.0.0-rc.6`。

## 🧭 新会话起手（先读这几处再写码，否则必返工）
1. `docs/rxrust-source-analysis/`（rc.6 逐行取证，01–09；**09** = 取消/锁/终止投递次序不变量 + “库自带注释与实现不一致”清单）；
2. `.qoder/skills/rxrust-reactive/SKILL.md`（动手前“先选动词”清单 + 本仓库踩过的 GC 形态）里的
   「Single sources already fixed here」表——**六条已定死的单一
   真源**（单位表/比较/色彩 route/无单位拒语/打印律/`Tok::MemberVar`）；新增第二份副本就是回归本身，先合并。
3. `sasspile-start` skill（已落地）：起手四步必须实跑命令并**逐字复述读数**（`ledger runs`、
   意外错误计数、`sasslint` 命中数、`openspec status`）；细则在它自己的 `reference/` 三份里。设计文档
   `docs/superpowers/specs/2026-10-28-cross-session-anti-gc-regression-design.md` 的 S1+S2 已实施（两条新闸
   +`defence_fitness` 体检+子代理+本文件瘦身），**但它的“冷启动三次实测”验收尚未跑** —— 见同一
   change 的 plan 末尾「实施结果与遗留」。两条纪律先从纪律层面生效：
   **读数只能来自产生它的那条命令**；**陈旧摘要/旧 dump 的记忆不是证据**（本轮 10 次“旧断言前提变了”同一成因）。

## 📌 基线（唯一副本；别处只准放指针）
   已得结论**不得凭记忆重推，也不得重做**（四个内核、真 `Err` 通道、结构化选择器/属性名、四条铁律机器闸、
   色彩三族与 math 域的已定定律均已成文，见 `state-of-the-port.md`）。当前基线 hrx **1845/11,533**（16.0%，**实质通过 1680** + 空期望蒙对 165；
   A/B 记录 = `reports/hrx.db` 里的具名 run `trig-shell`；该库中 `readonly-module`/`defence-s1s2` 为
   旧基线，`bare-calc-fns`/`bare-abs` 是**已回退批次作废读数**，均不得当 diff 的 base）；无效错误长期基线 53 → **20** → **18**
   （那 33 行全部是 `math.$pi` 这类带命名空间的引用被当成未绑定变量报成 `Undefined variable.`，
   本轮把它做成词素后消失；曾因定长 arity 把
   Microsoft-filter 重载报成致命错而变成 55，靠 `ledger failures … --count` 拦下）。
   唯余 0′.5（共享态 `C::RcMut`/`C::RcCell`，现无可改之物，触发点在 P4 `@content` / P5 模块多播）。

**三条不可协商（过程门禁；技术禁令见下面「硬性禁止」，不在此重复）**
1. 动手前必过 `sasspile-start` 的起手四步，并复述它打出的读数（复述不上 = 没读）。
2. 批次后必跑 `sasslint`（要 `count=0`）与 `gc-auditor`（finding 必须带 `file:line`）。
3. 不得放松闸、不得改名换形状绕闸、不得用"先记 TODO"顶替修复。

## 硬性禁止（违反 = 任务失败）
1. **禁 Python/脚本**：不用 python3/pip，一律 Rust；测试用 `#[test]` 且放 `tests/`（`src/` 保持纯生产码，禁内联测试）。
2. **禁 `println!/eprintln!`**：一律 tracing 宏；跨函数/阶段的管道必须用 `tracing::span!` 记上下文与耗时（字段 `stage`/`module`/`id`/`elapsed_ms`），不得仅用单条 `event!`。
3. **单文件 ≤ 500 行**（源码、测试分别计），超出必拆。
4. **禁 GC/命令式思维**（细则与反例索引见「铁律」与
   `.qoder/skills/sasspile-start/reference/gc-counterexamples.md`）。

## Git 规范
- 推送用 SSH：`git push github main`（remote `github` → `pauljohn21/qrass`）。只提交不推送时，`commit` 后等用户确认。

## 🔬 调试协议（强制）
禁止凭直觉猜根因；bug 修复须基于 tracing 证据链：① 在疑似路径入口/出口加 span ② 采集 ③ 引用具体 span+字段定位
④ 修复后清理临时 span。**不得再造临时探针文件**：本轮起诊断面已落地（tasks.md 9.0）——
`HRX_CASE=<key 子串>` 只跑一条用例（与 `HRX_RUN` 互斥：部分测量不得入库），`--features trace` 把现有 span
桥到 `opentelemetry-stdout` 直接打印（无 collector 无网络），入口是
`cargo run -p sasspile --features trace --example trace_pipeline`；声明级 `value` span 给
`module`/`id`/`raw_value`/`resolved`，`calculation read` 与 `call decision` 两条事件给**词素原文与切完的形状**
——“这条声明为何逐字输出”、“我的判据为何匹配不到”看这两行即可。三条约束：① 阶段 span 必须**住在观察者里**，
在 `subscribe` 里 `enter()` 的 span 一返回就关闭，`elapsed_ms` 量的就只是接线而不是流线；② `elapsed_ms` 用**带小数毫秒**，
整数 ms 会把小相报成 0 伪装成“瞬时”；③ `tracing` 宏体内不要写 `Value::Xxx` 路径——宏作用域里 `Value` 是
`tracing` 自己的 field trait，判定要在宏外算完再传进去。另：`tracing-opentelemetry` 只导 **span** 不导 event，
给一个工具加 trace 时必须先确认它真的建有 span。代码查询优先 CodeGraph。

## ⛔ 铁律：以 rxrust 为核心 = 用 Rust 所有权写程序
rxrust 不是包在命令式代码外的响应式库，而是 **Rust 所有权的一种良好表达**；顺它写可消掉大批 GC 问题（共享可变、对象图别名、边遍历边改、谁负责释放、空引用/生命周期）。**rxrust 本身就是引擎**，不是给状态机套的壳。

**思维顺序（治本，每次动手前默念）**：
1. 先只描述 **哪些 owned 值、沿哪条流、被算子怎么 move / 展开 / 丢弃**——让所有权与数据形状决定分解，而不是让"我要维护的可变状态"决定。
2. 一冒出"我需要维护一个跨元素的可变状态（栈 / 队列 / 缓冲 / 游标 / 万能 `Acc`）"→ **那是 GC 报警**：停下 **重造数据**（嵌套做进值里、展开=`flat_map` 对 owned 集合、选支=`filter`、一到多=`Subject`），**不是**给状态机包一层算子。`from_fn`+可变 `VecDeque`/`Vec<Frame>`、`scan_map(Acc)` 里 Acc 手搓跨元素状态——**都是穿了算子外衣的 GC 逃逸**。
3. 控制流若"非要有手动栈/缓冲/重放"才能表达 → 几乎一定是 **分解错了**，不是 rxrust 不够用；别改造/迁就 rxrust 来喂命令式结构。
4. **不许用结果（net/过测/分数）给 GC 结构背书**；思维正确性优先，宁可删了重来，也不留"算子外衣下的状态机"。

**绕过 rxrust 不便时先假设模型错**：想绕 `'static`/GAT/要 `box`/不能返回引用——正解是把状态改成被 move 的 owned 累加子、喂 owned 值进流（`from_iter` 给 owned 值、跨签名用 `box_it`），**不是** clone 凑数、不是退回命令式、不是加封装。

**程序即一条流**：from_iter/create/from_stream → scan(状态即 item)/scan_map(私藏 acc)/reduce_initial(owned 累加子) → flat_map 展开 / filter 选支 → collect → 公开入口唯一一次收口，**只用框架原生口子**：collect/reduce_initial + `into_future().await`（`into_future` 要求恰好一个值：空→`Empty`、多→`MultipleValues`，且它丢弃订阅句柄=不可取消）、`into_stream()`（只在 `Drop` 时取消）、或 await 订阅句柄（from_stream/timer/observe_on 等返回的 `TaskHandle` 本身就是 Future）。`std::sync::mpsc` **不是终点**，仅用于必须把 `Sender` 交给外部/跨线程的桥接；禁在异步核心外套 mpsc+block_on std 壳。状态=被流携带的 owned 累加子；错误=流内 `Result<T,SassError>`（`Err=Infallible`）；一到多才用 `Subject`/`behavior_subject`+`peek`/`publish().ref_count()`（rc.6 **无 `share()`**；`Subject` 不是共享可变的挡箭牌，且**回调内重入发射会 panic**，需 `delay(0)` 造异步边界）；深度是数据（栈在累加子里）不是调用栈。注：`Shared::from_stream` 单条保序，切勿对每条 item 各起一个 `from_stream`（并发→乱序）；并发额度满时 `merge_all` 会**丢弃仍在排队的内层**期间的发射（`sort` 无法恢复），要不丢就只用 `concat_map`/`concat_all`。`@if` 链作为 `scan_map` 累加子里 move 的值传递，不是每 item 独立可变标志。

**值与控制建模**：值建模成 `Expr`/`Value`，`eval(Expr, owned Env)`（`and`/`or`/`if` 惰性短路），从结构化 token 一次成型，禁"拼串再 char 扫"。嵌套/树必须流式平展成文档序 `Vec<Leaf>`（render=对 leaf 的 `map`、循环=`flat_map`、条件=`filter`），禁 `Vec<Rule>`/`children` 递归下降。`if/else if` 链、多守卫 `return`、`for/while/loop`、自造 `Stage/fail/drain` 框架层、std `.iter().map/fold/collect()` 当数据流替身——全禁；确不可等价表达 → 上报阻塞项，不退回 std。风格：链式 + 闭包(`FnMut`) + fluent builder；例外仅纯值 `Option/Result` 组合子与单个无 `else` 的 `if` guard。


## 📦 已迁出本节的内容（指针，不再在本文件复述）
- 反例索引（动手前逐条 grep、命中即回根上重构）：`.qoder/skills/sasspile-start/reference/gc-counterexamples.md`
- 工具级红线（验收读数也会骗人 / A-B 记录 / 量具覆盖面）：`.qoder/skills/sasspile-start/reference/tooling-redlines.md`
- 移植现状长叙述（每相/每族做到哪、为何这么判）：`.qoder/skills/sasspile-start/reference/state-of-the-port.md`
  —— 逐条实测证据在 `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md`，
  架构决策在其同目录 `design.md`，A/B 读数在 `reports/hrx.db` 的具名 run。
## 🔌 接线纪律（新功能）
新算子/函数先在 `expr.rs`/`value.rs` 做成 **pub 独立件 + 金样单测** 钉死，**不接 `compile`**；接线时只在无歧义处触发：含 `/ * %` 的值跳过（护 `font:12px/1.5`、`grid:1/2` 等 CSS 斜杠分隔符），函数走**白名单** `call_builtin`，未知名 `eval→Err→` 原样兜底（`url/calc/min/max` 等 CSS 原生函数永不当 Sass 内置求值）；会动全局 truthiness 的比较/布尔单列一步、先护选择器（`a > b` combinator 不得当比较）。每步 `ledger diff` 确认 **`regress=0`** 才提交，否则回退。

## ✅ 验收 / 参照
项目 = clean-room `crates/sasspile`（配 `ledger` A/B 库、`sasslint` 铁律审核）；`crates/qrass` 已删除，**不得**以其代码为模板。行为**唯一**参照 = 本地 `sass-spec/*.hrx`（input→output.css/error）+ `element-plus`，**严禁**参照 dart-sass 源码/文档推断行为。每步跑 hrx 快照 + `ledger diff <base> <run>` 守 **`regress=0`**。
