# rxrust-stream-substrate Specification

## Purpose

定义全库基底契约：在 `crates/qrass/src` 中，承载 **owned 值** 的 rxrust `Observable` 流是
一切序列迭代、状态迁移、批量变换与阶段编排的唯一许可形态；可变状态表达为被 move 进流的累加子
（绝不是共享的 `&mut self` / `Arc<Mutex<_>>`），错误是流内值通道，同步终端只有一个（`drain`），
游标是逐输入状态迁移器。该契约的存在是为了：在内部表达统一为响应式流的同时，编译器的对外可观察
行为逐字节不变；且强制是机器校验的，而非人工放行。

## ADDED Requirements

### Requirement: 行为逐字节不变（最高约束）

本基底重构 SHALL 不改动编译器的任何对外可观察行为：sass-spec hrx 通过率不降、每条错误文本不变、
四个 element-plus e2e 金样输出逐字节相等、既有全部 `#[test]` 断言继续通过。任何批次若无法同时满足
`0 回归` 与基底契约，MUST 回退该批次并记录阻塞；MUST 不以行为换形态，也 MUST 不以形态换豁免。

#### Scenario: 每批 A/B 硬闸

- **WHEN** 任一改造批次被提交
- **THEN** 针对该批父 commit 所取快照做全量 hrx A/B 报 `regress=0`，且 e2e / unit / clippy
  （`-D warnings`）/ `wasm32-unknown-unknown` 全绿

#### Scenario: 不可表达即阻塞上报，不作豁免

- **WHEN** 经实测证明某构造在不改行为的前提下无法等价表达为 owned 累加子的 Observable 流
- **THEN** 暂停该批次，写出带最小复现与 A/B 证据的阻塞项；绝不登记为豁免，也绝不把循环形态悄悄留在原地

### Requirement: Observable 是唯一数据流形态（std 链即违例）

凡"对序列迭代、批量变换、逐元素状态迁移、阶段编排"的操作，MUST 是 rxrust `Observable` 链
（`Local::from_iter` / 上游流，经 `map`/`filter`/`filter_map`/`scan_map`/`reduce_initial`/
`flat_map`/`take`/`take_while`，终于 `collect`/`drain`/订阅）。SHALL NOT 存在任何 `for`/`while`/`loop`。
SHALL NOT 存在**替代此类数据流的 std 迭代器链**（如 `x.iter().map(..).collect()`、
`.filter(..).collect()`、`.fold(..)`、`.try_fold(..)`、`.for_each(..)`）——std 链不是可接受的载体，
同步与否也从不是改用它的理由。不属于数据流的单值谓词/访问器（`matches!`、直接字段读取、对**单个值**
的 `Option`/`Result` 组合子）在本规则之外；凡"遍历并重建一个集合"者都在本规则之内。

#### Scenario: 叶层 map/collect 变流

- **WHEN** 某叶层通过 map/filter 一个输入序列来构造输出集合
- **THEN** 使用 `Local::from_iter(input).map(..).filter(..).collect::<C>()`（或归约为单值的
  `reduce_initial`），而非 `input.iter().map(..).collect()`

#### Scenario: 有状态折叠用 reduce_initial，非 std try_fold

- **WHEN** 某折叠需跨元素携带累加子（去重、分组、求和、first-wins、短路式校验）
- **THEN** 折叠为 `Local::from_iter(items).reduce_initial(seed, |acc, item| ..)`（或 `.scan_map`），
  累加子是被 move 过流的 owned 值——不是架在外部 `let mut` 上的 std `.try_fold`/`.fold`

### Requirement: 可变状态是 owned 累加子，而非 `&mut self`

任何逐元素的可变操作 MUST 作用于被流拥有的值，并作为 `scan_map`/`reduce_initial` 的累加子携带。
作为载体被禁止：在循环里改共享 `&mut self`、穿一个 `&mut`/索引游标变量、`Arc<Mutex<_>>`/`RefCell`
共享可变的业务状态。`'static` 闭包要求通过**把 owned 输入 move 进折叠**来满足，绝不通过放弃 Observable
形态退回 std 链。若某函数现为跨序列累加而收 `&mut self`，重构需把所需的 owned 状态（clone 或 take）
移进流，并返回产出的 owned 值。

#### Scenario: `&mut self` 走查改穿为 owned 状态

- **WHEN** 一个曾在 `self` 上跨序列累加的走查被改造
- **THEN** 被改的数据成为流的 owned 累加子、经 `scan_map`/`reduce_initial` 移动；`'static` 约束由
  "把 owned 输入移进来"满足；折叠路径里不再残留 `&mut self`/游标

#### Scenario: 提前错误经值通道短路

- **WHEN** 某折叠需停在第一个失败元素并返回该错误（旧的提前 `return Err`）
- **THEN** 累加子携带一个 latched `Option<E>`（或该阶段发 `Result<_, E>` 项），首错经 `drain`/`?`
  返回——全程无 `for`/`while`，也不依赖 rxrust 那条（未被使用的）错误通道

### Requirement: 唯一同步终端——`drain`

跨阶段 / 跨函数的流被装箱（`box_it()`），并终于恰好一个同步终端：crate 的 `drain`（收进 `Vec`、
对首个流内错误短路）或 `drain_one`（取 `reduce_initial` 的那唯一一个累加值）。MUST NOT 新增逐调用点的
临时 `subscribe`-写入本地、`into_future().await` 或 `block_on`；`Observable` 不得越过终端外泄，
同步 API 只经 `drain`/`drain_one` 拉取其结果。该桥接 MUST 在无异步运行时下运行（非定时源在
`subscribe` 期间完全 inline 执行），以守住 `Local` + wasm32 契约。

#### Scenario: 值被移出，回调不四散

- **WHEN** 一个同步 API 需要某阶段流的 owned 结果
- **THEN** 它调用 `drain`/`drain_one`（唯一终端），而非在调用点自搭 `subscribe`/channel/receiver

### Requirement: 分支与守卫 return 消除

`if` / `else if` 控制流链与堆叠的守卫 `return` MUST 被消除：值选择用单表达式 `match` 或组合子
（`then_some`/`then`/`map_or`/`ok_or_else`/`and_then`/`unwrap_or_else`/`is_some_and`）；提前失败用
`?` / `ok_or_else` / `or_else` 链承载；函数体只留单一尾部出口。保留的合法非命令式形态：(a) 对
enum/`Result`/`Option` 的**单表达式 `match` 分派**（取值语义）；(b) 对**单个值**的 `Option`/`Result`
组合子链。`?`-传播本身永不算违例。

#### Scenario: 守卫堆叠折成分类 match

- **WHEN** 一个函数用 ≥2 条语句级提前 `return Err/Ok` 守卫承载多路分支
- **THEN** 分支收进返回域 enum 或元组 `match` + 单一尾部分派，值与错误在各臂内构造，且**不引入 `else if` 链**

### Requirement: 游标是逐输入状态迁移器

字符/词法游标（`refactor-rfp-strict` 的欠账：`lexer_detail.rs` 的引号/插值扫描、
`builtins/selector.rs` 的 `simple_selectors`、`parser_if.rs` 的递归下降 token 游标）MUST 重新表达为
一条把输入逐个推进 owned 状态机的 rxrust 流——`Local::from_iter(inputs).scan_map(owned State,
|st, input| st.step(input))`，每输入产 0..N 个输出（`lexer.rs` 的 `feed`/`scan_map` + `LexPunct`
先例），用 `collect`/`drain` 收口。"变步长游标不能是 Observable"**不可接受**：那正是本 change 要作废的
共享可变游标视角。真正不可表达的扫描器按上文"阻塞上报"规则处理，绝不豁免。

#### Scenario: 切词器在边界处产出

- **WHEN** 某扫描器跨输入累积一个 token、在分隔符/边界处发出它
- **THEN** `scan_map` 把"进行中的 token + 位置"作为累加子携带，在边界处发出一个完成 token（每输入
  0..N 个），全程无 `while`/`for`、无外部索引游标

### Requirement: 强制机器可校验，且覆盖 std 链

`tests/style_gate.rs` MUST 保留逐文件 `(loops, else-if, let-mut, guard-return)` 的只降不升棘轮，并
新增**std 链替身**维度：逐文件标注候选数据流 std 链（集合构造型 `.map/.filter/.take/…/.collect()`、
std `.fold`/`.try_fold`/`.for_each`），基线写入 `tests/golden/std-chain-baseline.txt`，只降不升逼向零。
新引入的违例链使门禁失败。该 gate MUST NOT 读取任何豁免白名单（不存在）；已识别的单值
`match`/组合子合法形态不计入。

#### Scenario: 新增 std 链触发棘轮失败

- **WHEN** 新的源码改动以 std 迭代器链构集合、替代了本应做的 Observable
- **THEN** 该文件的 std 链维度计数高于基线、门禁失败，直至其被改造成 Observable 或某既有计数被消除

#### Scenario: 改造同时降低两维

- **WHEN** 某处从 std 链 / 裸循环改造成 owned 累加子流
- **THEN** 其 loop 与 std-chain 计数下降，基线被 re-bless 到更低值

### Requirement: 风险升序分批，单轮只做一边

改造 MUST 按行为风险升序推进——(B1) 叶层纯 std 链、(B2) 有状态 std 折叠、(B3) 自包含字符/token 游标、
(B4) `&mut self` 求值/loader 走查、(B5) CLI `main.rs` 入口（须先有 `tests/` 测试台使其可验证）——逐文件
commit，使每次提交独立可回退。与 `converge-sass-spec-to-100` 主线穿插时，单轮工作只做一边（重构 或 收敛），
以保持 A/B 归因链纯净。

#### Scenario: 归因不混叠

- **WHEN** 一个 commit 同时含基底改造与行为收敛改动
- **THEN** 视为流程违例，须拆分重提

### Requirement: 撤销沿用的豁免

`refactor-rfp-strict` 的豁免授权与 `tests/golden/style-exemptions.md` 放行清单 MUST 作废；每个曾被"豁免"的
命令式构造改判为本契约下的跟踪改造项（C1–C6 集），且 `rfp-style-contract` 冻结门禁以"loops/else-if/std-chain
全零、无白名单"重新基线。

#### Scenario: 不再有放行文件

- **WHEN** 强制需要"许可"某命令式构造
- **THEN** 不是"给它登记豁免"——它要么被改造，要么带证据升级上报为阻塞项；基线只会收缩
