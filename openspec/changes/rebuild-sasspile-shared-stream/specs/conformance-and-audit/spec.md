# conformance-and-audit Specification

## ADDED Requirements

### Requirement: 金样唯一行为权威 + dart-sass 仅 oracle
行为 MUST 只由本地 `sass-spec/*.hrx`（input.scss→output.css/error）+ `element-plus` 决定；MUST NOT 以 dart-sass 源码/文档推断**行为**，更 MUST NOT 以其"物化 AST→遍历→GC"心智作**架构**参照。测试用 `#[test]` 且置于 `crates/sasspile/tests/`。

#### Scenario: 回归判定
- **WHEN** 逐 `.hrx` 跑 `compile_string`
- **THEN** 与 `output.css`/error 比对（尾空白外），入 `ledger`

### Requirement: ledger A/B 且 regress=0 才合入
每次运行以 `ledger` 快照 per-case 结果，`ledger diff <base> <run>` 报 `regress`/`net`；合入 MUST `regress=0`，否则回退到绿色基线。新流式核心替换旧核期间，以旧核通过集为参照保 `regress=0`。

#### Scenario: 替换期保序
- **WHEN** 重写核心并对照旧核
- **THEN** 旧核已过用例在新核 MUST NOT 回归（regress=0）方可合并

### Requirement: 隔离→金样单测→守门接线生长纪律
新增算子/函数 MUST 先 pub 独立件 + 金样单测钉死、不接主链；接线只在无歧义处触发；每步 A/B。

#### Scenario: 先测后接
- **WHEN** 加内置函数
- **THEN** 先独立件单测，再守门接线，全程 regress=0

### Requirement: sasslint 覆盖"算子外衣下的状态机"
`sasslint`（Rust，`crates/sasslint`，CLI=CI 闸 + MCP `review`）规则 MUST 至少含 `no-tree`(`Vec<Rule>`/`children`)、`no-recursive-descent`(自调用/`resolve*`/`write_rule`)、`no-relay-vec`(`-> Vec<..>`)、`no-string-machine`(`subst`/`stmt_text`)、`no-imperative-loop`(行首 `for/while/loop`)、`no-from-fn-driver`、`no-work-queue`；并 MUST 增启发式检测"**可变累加子驱动遍历**"（`scan_map`/`reduce` 的 `Acc` 含 `Vec<Frame>`/栈/`chain` 等跨条目可变驱动字段），命中即阻断，作为 `regress=0` 之外的结构闸。

#### Scenario: 帧栈累加子被识别
- **WHEN** `Acc` 里出现用于跨条目驱动的可变帧栈/标志
- **THEN** `sasslint` 报"mutable-accumulator-drives"并阻断

#### Scenario: 清零后启用门
- **WHEN** `sasspile/src` 达 0 命中
- **THEN** 取消 `gate_sasspile_is_clean` 的 `#[ignore]`，使任何同类违规令测试失败
