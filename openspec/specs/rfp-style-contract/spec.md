# rfp-style-contract Specification

## Purpose
TBD - created by archiving change refactor-rfp-strict. Update Purpose after archive.

## Requirements

### Requirement: R1 行为不变量（重构的最高约束）

风格重构 SHALL 保持编译器的全部可观察行为逐字节不变：sass-spec 金样套件（9161+ 例）通过率不降、错误文案不变、element-plus 端到端金样（4 案）不变、既有 unit 断言全部通过。任何批次若无法同时满足「0 回归」与风格契约，MUST 回退该批次并记录阻塞，MUST NOT 以行为换取风格或以风格豁免行为。

#### Scenario: S1 批次验收

- **WHEN** 任一重构批次完成并提交
- **THEN** 全量 hrx 对拍（前态 vs 后态）回归集为空，且 e2e、unit、clippy、wasm 目标检查全绿

#### Scenario: S2 回归即回退

- **WHEN** 批次验证出现任何一例回归
- **THEN** 该批次全部改动回退，归因结论写入报告，修复验证后方得重提

### Requirement: R2 流形态统一（Observable 唯一表达）

代码库内所有「对序列的迭代、状态迁移、批量变换、阶段编排」MUST 以 rxrust Observable 链为唯一表达形态（`Local::from_iter`/既有上游流 起始，经 `map`/`filter`/`filter_map`/`scan_map`/`flat_map`/`take` 等算子，终于 `collect`/`drain`/订阅终点）。SHALL NOT 存在 `for`/`while`/`loop` 形式的迭代或累加循环，亦 SHALL NOT 存在以 std 迭代器组合子链替代 Observable 的同型数据流（同步与否不作为豁免理由）。既有管线（lex→parse→eval→render 的 `flat_map` 衔接、单一 drain 终端、in-stream `Result` 错误协议）为该契约的既有范式锚。

#### Scenario: S1 存量循环清洗

- **WHEN** 于任一源文件 grep 迭代/累加类 `for`、条件堆叠类 `else if` 链
- **THEN** 除 Requirement.4 登记的豁免项外命中为零

#### Scenario: S2 状态迁移经 scan_map

- **WHEN** 变换需要逐元素携带状态（字符状态机、计数、聚合缓冲）
- **THEN** 状态迁移以 `scan_map(seed, |acc, x| ..)` 闭包表达，累加器不以外层 `let mut` 变量泄漏到循环体

### Requirement: R3 分支消除（闭包优先）

`if`/`else if` 控制流链与守卫堆叠 MUST 消除：值选择以 `match` 单表达式（仅 enum/结果分派，见 Requirement.4）、组合子（`then_some`/`map_or`/`unwrap_or_else`/`filter`/`any`/`is_some_and`）与表达式返回承载；提前失败以 `ok_or_else(..)?`/`and_then`/`or_else` 错误链承载。嵌套 `if` 判定与布尔拼装不得以语句形态出现。

函数体内的多路提前 `return Err(..)`/`return Ok(哨兵)` 守卫堆叠（同一函数 ≥2 条语句级提前 return 承载分支判定）MUST 消除：以返回域 enum 的 `match` 单表达式分类 + 尾部分派承载，值与错误在 `match` 臂内构造。合法例外：`?` 错误传播本身、闭包体内的提前 return（fold/map 错误联合载体）、单出口早退（整个函数只有一个 guard return，如arity过滤）。

#### Scenario: S1 文案矩阵分派

- **WHEN** 内置函数按参数形态分派多种错误文案
- **THEN** 分派以链式闭包/单表达式 `match` 完成，不出现 `if .. else if .. else` 语句链

#### Scenario: S2 守卫 return 堆叠

- **WHEN** 一个函数用多条语句级 `return Err/return Ok(None)` 提前承载多路分支
- **THEN** 分支收进 `match`（或返回域 enum）单表达式，函数只剩单一尾部出口；`?` 传播不受此限

### Requirement: R4 零豁免（rxrust 为唯一基底）

Observable 流是本代码库控制流的第一性基底，不是可选项。仅两类形态本就落在“非命令式”范畴、无需任何登记：① 对 enum/`Result`/`Option` 的**单表达式 `match` 分派**（取值语义，非控制流）；② 对**单值**（非序列）的 `Option`/`Result` 组合子链。除此之外，任何 `for`/`while`/`loop`、任何以 std 迭代器组合子链替代同型数据流的写法、任何 `if/else if` 链与守卫 `return` 堆叠，MUST 无条件改造为 rxrust `Local::from_iter` + `map`/`filter`/`filter_map`/`scan_map`/`flat_map`/`reduce_initial`/`collect` + 单一 `drain` 终端；把“原地 `&mut self`/游标”迁移为流内 **owned 累加器状态**（`scan_map(&mut acc, …)` / `reduce_initial`），把错误迁移为 in-stream `Result`。**不存在豁免清单**：若某构造经实测确实无法等价表达为 Observable 链，该事实是 MUST 上报的阻塞项（含最小复现证据与回归对照），MUST NOT 以登记白名单的方式放行，也 MUST NOT 以行为换取风格或以“同步/更清楚”为借口退回 std 链。

#### Scenario: S1 游标不可表达即阻塞上报

- **WHEN** 某字符/token 游标经实测无法以 Observable 链等价表达且代价不可接受
- **THEN** 该情形作为阻塞项写入报告并暂停该批次，附最小可复现证据；MUST NOT 登记为豁免、MUST NOT 悄悄降级为 std 链或裸循环

### Requirement: R5 持续防漂移（门禁可验证）

风格契约 MUST 机器可验证：crate 级 clippy deny 集（至少 `needless_range_loop`、`explicit_iter_loop`）长期生效；提供 Rust 实现的扫描工具，对 `src/` 输出迭代类/条件链类/可变累加构造的计数报表（对照基线只降不升，批次目标为零）；新代码（含并行推进的收敛 change 切片）自契约批准起即时受约。

#### Scenario: S1 新代码合规

- **WHEN** 契约批准后任一 change 提交新增源改动
- **THEN** 扫描报表对新增行命中为零（豁免清单条目除外）

### Requirement: R6 分批推进与穿插纪律

存量清洗按回归风险升序分四批（S1 叶层纯函数 → S2 eval 参数层 → S3 extend 引擎 → S4 parser/lexer），每批独立满足 Requirement.1/2 验收；与 `converge-sass-spec-to-100` 主线穿插执行时，单轮工作 MUST 只做一边（重构或收敛），保持 A/B 归因链纯净。

#### Scenario: S1 归因不混叠

- **WHEN** 一次提交同时含风格重构与行为收敛改动
- **THEN** 视为流程违例，须拆分重提
