# conformance-harness Specification

## ADDED Requirements

### Requirement: hrx 金样 runner
工装 MUST 把 `sass-spec` 现代套件的每个 `.hrx`（排除套件外的 `non_conformant` 与
`libsass*` 目录）解析成段（块以 `<===> <relpath>` 界定，段之间由裸 `<===>` 行或一串 `=` 分隔），把每段的
`input.scss` 经 `sasspile::compile_string` 编译，并与该段的 `output.css` 比较（错误用例则与期望的
`Error:` 文本比较）。用例仅在精确匹配（尾随空白除外）时 PASS。

#### Scenario: 判定一个 css/plain 用例
- **WHEN** 某段有 `input.scss` 与 `output.css`
- **THEN** runner 记 PASS 当且仅当编译输出等于金样，否则 FAIL

### Requirement: A/B ledger 存储
每次 run MUST 把其 PASS/FAIL 集按 run 名记入 SQLite `ledger`（`reports/hrx.db`），回归/net 增益
MUST 可由 `ledger diff <base> <run>` 算作集合差（regress = fails(new) − fails(base)）。

#### Scenario: 回归即非空集合差
- **WHEN** 某用例在 base 通过、在新 run 失败
- **THEN** `ledger diff` 把它列在 `REGRESS` 下

### Requirement: 零回归验收闸门
任一能力变更的验收 MUST 要求一次对齐当前基线的全量 hrx run 报 `regress=0`；新通过的用例即净增益。
基线的 run 名与 pass 数 MUST 被记录以供下次比较。

#### Scenario: 有回归则批次被拦
- **WHEN** `ledger diff` 显示任一 `REGRESS` 项
- **THEN** 该变更批次不通过验收

### Requirement: 仅金样的行为权威
工装与所有规格 MUST 把本地 `sass-spec` hrx 金样与 `element-plus` 语料当作唯一行为权威，且 MUST NOT
查阅 dart-sass 源码或文档来决定行为。

#### Scenario: 不符即当作 bug
- **WHEN** sasspile 输出与金样不符
- **THEN** 对着金样修复，而不是用其它实现的说法来正当化
