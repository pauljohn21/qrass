# Proposal

## Why（为什么）

`refactor-rfp-strict` 把 `for`/`while`/`loop` 与 `else if` 逼近了零，但它是在**用共享可变状态
（GC / Java / dart / RxJS）的视角**去理解 rxrust，而不是它的真正内核——**Rust 的所有权与移动
语义**。后果是：叶层数据流被大量改写成 **std 迭代器链**（`.iter().map(..).collect()`、
`try_fold`、`for_each`），`&mut self` 走查被当作"合法载体"保留，递归下降/字符**游标被说成
"不能是 Observable"**而堆进豁免清单。按 `R2`，那些 std 链**从来就是违例**；而"`'static`
闭包碰不了 `&mut self`"并不是 rxrust 的限制——它恰恰是在指示我们：把状态做成**被 move 进流的
owned 累加子**，而不是借用共享可变的 `self`。`style_gate` 根本没抓 std 链，所以这笔债既看不见、
也没被 ratchet 收敛。

本 change 把 rxrust 立为**唯一基底**，以所有权为先：每一次序列迭代、状态迁移、批量变换与阶段编排
都是一条承载 **owned 值** 的 rxrust `Observable`；可变状态发生在被 move 进来的累加子上
（`scan_map`/`reduce_initial`）；错误走流内 `Result` 值通道；唯一的同步终点是 `drain`。**零豁免**。

## What Changes（改什么）

- **新能力 `rxrust-stream-substrate`**：所有权先的流契约——Observable 是唯一的数据流形态；
  替代数据流的 std 迭代器链是违例；可变状态是 owned 累加子而非 `&mut self`/`Arc<Mutex>`；游标是
  逐输入状态迁移器；`drain` 是唯一终端；确实不可表达者是**须上报的阻塞项**，绝非豁免。
- **撤销沿用的豁免授权**：`refactor-rfp-strict` 的 R4"登记白名单"退路与 `style-exemptions.md`
  豁免清单作废；其中记录的 C1–C6 被升级为待改造项。
- **扩展 `style_gate` 以看见 std 链**：新增一维，逐文件标注候选"std 迭代器数据流替身"（集合构造型
  `.map/.filter/.fold/try_fold/…/.collect()`、被当作有状态折叠用的 std `try_fold`/`for_each`），
  以"只降不升"棘轮逼向零，让此前隐形的欠账显性化、可度量。
- **改造残余**（行为逐字节不变，每批以全量 hrx A/B `regress=0` 把关）：叶层 std 链 →
  `Local::from_iter(..).map(..)`；有状态 std 折叠 → `reduce_initial`/`scan_map`；字符/游标
  （`lexer_detail`、`builtins/selector.rs` `simple_selectors`、`parser_if`）→ 承载 owned `State`
  的 `scan_map` 迁移器，逐输入产 0..N 输出（`lexer.rs` 的 `feed` 先例）；`&mut self` 求值走查 →
  owned 累加器流；`main.rs` `parse_args` → 对 owned `Vec` 做折叠（先补 CLI 测试台使其可验证）。
- **无对外行为变化**：不改动任何对外可观察行为、API 或输出文本——这是对内核的基底/所有权重构 +
  一处测试门禁改动。

## Capabilities（能力）

### New Capabilities（新增能力）
- `rxrust-stream-substrate`：全库契约——rxrust `Observable` 流（承载 owned 值）是所有
  迭代/状态迁移/批量变换/编排的唯一许可形态；可变状态经 owned 累加子流动；唯一同步终端是 `drain`；
  游标是状态迁移器；不可表达即上报（永不豁免）；且全程机器可校验。

### Modified Capabilities（修改能力）
<!-- `rfp-style-contract`（来自 refactor-rfp-strict）在"豁免"这点上被本 change 取代；
     该 change 仍在进行中且其 R4 本会话已改写为零豁免，故此处不再另写 delta 以免竞争性
     编辑——本 change 新增的是"所有权基底"与"std 链强制"。 -->

## Impact（影响面）

- **代码**：散布于 `crates/qrass/src/`——叶层变换（`css.rs`、`value.rs`、`builtins/*`、`error.rs`、
  `span.rs`、`selector*`）、被后置的游标（`lexer_detail.rs`、`parser_if.rs`、`builtins/selector.rs`）、
  `&mut self` 求值/loader 走查，以及 `builtins/color_transform.rs`（C5 本会话已作为范式验证完成）。
- **工具**：`crates/qrass/tests/style_gate.rs` 增加 std 链维度 + `tests/golden/std-chain-baseline.txt`；
  `tests/golden/style-exemptions.md` 退役。
- **测试**：新增 `tests/` 里针对 `main.rs` CLI 解析的测试台（使 C1 可验证）；每处改造以既有 hrx A/B
  （`reports/hrx.db`）、e2e、unit、clippy、wasm32 把关。
- **依赖**：不新增；`rxrust =1.0.0-rc.6`、`Local` 上下文、wasm 契约保持不变。
- **风险**：触碰金样敏感的 lex/parse/eval 内核——以"逐字节 A/B 0 回归"红线 + 单文件可回退 commit 缓解。
