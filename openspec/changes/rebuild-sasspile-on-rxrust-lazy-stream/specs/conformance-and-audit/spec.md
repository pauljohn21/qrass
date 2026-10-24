# conformance-and-audit Specification

## ADDED Requirements

### Requirement: 金样驱动与行为隔离
系统 MUST 以本地 `sass-spec/*.hrx`（input→output.css/error）为唯一行为权威，并以 `element-plus` 语料辅助；MUST NOT 以 dart-sass 源码/文档推断行为。所有测试 MUST 位于 `crates/sasspile/tests/`（`#[test]`，禁内联、禁 Python/脚本）。

#### Scenario: 金样回归判定
- **WHEN** 对每个 `.hrx` case 运行 `compile_string`
- **THEN** 与 `output.css`/error 逐字比对（尾空白外），计为 pass/fail 存入 ledger

### Requirement: ledger A/B 且 regress=0 才合入
系统 MUST 用 `ledger` 把每次运行的 per-case pass/fail 快照入库，`ledger diff <base> <run>` 报告 `regress`/`net`。任何合入 MUST 满足 **`regress = 0`**；无法保持者 MUST 回退。

#### Scenario: 合入前 A/B
- **WHEN** 一个改动完成
- **THEN** 先 `ledger diff`，`regress>0` 即不得提交/推送，回退到绿色基线

### Requirement: 隔离→白名单→守门接线生长纪律
新增算子/函数 MUST 先做成 **pub 独立件 + 金样单测**、不接主链；接线 MUST 只在无歧义处触发（值含 `/ * %` 跳过、函数走白名单、未知名→原样兜底），并逐增量 A/B 验证。

#### Scenario: 先测后接
- **WHEN** 要加一个内置函数
- **THEN** 先在独立件写金样单测钉死，再守门接线，全程 `regress=0`

### Requirement: sasslint 机器审核契约
`sasslint` MUST 把本契约规则化为可 grep 的确定性检查（`crates/sasslint`，Rust、无脚本），提供 CLI（命中非零退出=CI 闸）与 MCP `review` 工具；对 `crates/sasspile/src` MUST 命中即阻断。规则集 MUST 至少含：`no-tree`(`Vec<Rule>`/`children`/`.children`)、`no-recursive-descent`(自调用/`resolve*`/`write_rule`)、`no-relay-vec`(`fn -> Vec<..>` 中转)、`no-string-machine`(`subst`/`stmt_text`)、`no-imperative-loop`(行首 `for`/`while`/`loop`)、`no-from-fn-driver`(`from_fn(`)、`no-work-queue`(`VecDeque<`)；注释行 MUST 跳过。

#### Scenario: 算子外衣的状态机被识别
- **WHEN** 源码用 `from_fn` 驱动可变 work-queue 冒充"流"
- **THEN** `no-from-fn-driver` 命中、`regress`/CI 阻断

#### Scenario: 门控测试在清零后启用
- **WHEN** `crates/sasspile/src` 经 `sasslint` 达 0 命中
- **THEN** `gate_sasspile_is_clean` 取消 `#[ignore]`，使任何同类违规令测试失败
