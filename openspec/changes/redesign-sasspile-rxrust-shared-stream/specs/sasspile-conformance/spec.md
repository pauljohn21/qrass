# Spec Delta

## Purpose

约定 `sasspile` 的验收与回归纪律：行为唯一由 `sass-spec` 金样与 `element-plus` 语料决定，每个增量
以 `ledger diff regress=0` 与 `sasslint` 0 命中为合入闸，测试为 `#[test]` 且置于 `crates/sasspile/tests/`。

## ADDED Requirements

### Requirement: 行为唯一参照为 sass-spec 与 element-plus

`sasspile` 的行为 MUST 仅由 `sass-spec`（`.hrx` 金样的 `input.scss`→`output.css`/`error`，及
`input.scss`/`_expected.css`/`error` 目录用例）与 `element-plus` 的真实 SCSS 语料决定。MUST NOT
以 dart-sass 源码/文档推断行为，MUST NOT 以其"物化 AST→遍历→GC"心智作架构参照，MUST NOT 以已删除的
旧 `qrass` 代码/结论为依据。`sass-spec` 中被标记 `:todo:` 含 `dart-sass`/`libsass` 或位于
`non_conformant/`、`libsass-*` 的用例 MUST NOT 作为通过判据。

#### Scenario: 参照集界定
- **WHEN** 确定某特性期望输出
- **THEN** 取自 `sass-spec` 对应金样或 `element-plus` 编译结果，而非 dart-sass 文档描述

#### Scenario: 排除非金样用例
- **WHEN** 计算通过率
- **THEN** `non_conformant/`、`libsass-*` 及被 `:todo:` 标为待实现的用例不计入回归基线

### Requirement: 回归闸 regress=0

每个合入增量的 hrx 通过集 MUST 相对基线零回归。`sasspile` MUST 经 `crates/ledger` 记录 per-case
通过/失败快照，合入前 MUST 运行 `ledger diff <base> <run>` 且 `regress` MUST 为 `0`；达不到 MUST
回退到最近绿色基线，MUST NOT 提交带回归的改动。

#### Scenario: 回归即回退
- **WHEN** `ledger diff` 报告 `regress > 0`
- **THEN** 该增量不得合入，回退到基线后重新以更小增量推进

### Requirement: 铁律以 sasslint 确定性把关

`sasspile` 源码 MUST 通过 `crates/sasslint`（CLI/CI 闸，含 `mutable-accumulator-drives`、
`no-from-fn-driver`、`no-work-queue`、`no-tree`、`no-recursive-descent`、`no-relay-vec`、
`no-string-machine`、`no-imperative-loop`）命中数为 `0`，作为合入前置条件。

#### Scenario: 命中即阻断
- **WHEN** 改动引入上述任一违规（如帧栈式累加子驱动、手搓工作队列、递归遍历树）
- **THEN** `sasslint` 命中数 > 0，CI 阻断合入

### Requirement: 测试置于 tests 目录

`sasspile` 的测试 MUST 用 Rust `#[test]` 且放在 `crates/sasspile/tests/`；`src/` MUST 保持纯生产码、
MUST NOT 内联测试；MUST NOT 使用 Python/脚本充当测试或构建步骤。

#### Scenario: 金样回归测试落位
- **WHEN** 新增一条 hrx 覆盖
- **THEN** 以 `#[test]` 置于 `crates/sasspile/tests/`，`cargo test -p sasspile` 运行

### Requirement: element-plus 端到端可用

`sasspile` MUST 能端到端编译 `element-plus` 的 SCSS 语料，其输出 MUST 与参照实现产物在 `ledger`
归一化比较下一致；element-plus MUST 作为真实世界集成回归的一部分随每次核心变更验证。

#### Scenario: 整库编译
- **WHEN** 编译 `element-plus` theme-chalk 入口 SCSS
- **THEN** 产出 CSS 通过 element-plus 语料回归判据，无 panic、无回归

### Requirement: 增量生长守门纪律

新算子/内置函数/值类型 MUST 先做成 `pub` 独立件并以 `sass-spec` 金样 `#[test]` 钉死、暂不接入
`compile_string`；随后在**无歧义**处守门接线（非 Sass 内置的 CSS 函数原样保留、含 `/ * %` 的值
跳过算术），逐增量验证 `regress=0` 且 `sasslint` 0 命中后才提交。

#### Scenario: 先独立件后接线
- **WHEN** 引入一个内置函数
- **THEN** 先以金样单测在独立件上通过，再接线并以 `ledger diff` 确认 `regress=0`
