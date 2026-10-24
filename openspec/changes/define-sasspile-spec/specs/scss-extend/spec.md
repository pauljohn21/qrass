# scss-extend Specification

## ADDED Requirements

### Requirement: @extend 并入被匹配的规则
在规则内写 `@extend <selector>` MUST 把被扩展选择器的 compound 加入每一个选择器被目标匹配的现有
规则，并按金样顺序产出归并后的选择器列表。extend 一个不存在的 class MUST 报出金样的 "target
selector was not found" 错误。

#### Scenario: extend 占位符
- **WHEN** `%e { x: y } .a { @extend %e }`
- **THEN** 输出为 `.a { x: y }`（占位符自身不发射）

### Requirement: 占位符选择器
`%name { … }` MUST 定义一条永不直接发射、只经 `@extend` 出现的规则。

#### Scenario: 占位符不单独发射
- **WHEN** 输入仅含 `%p { a: b }`
- **THEN** 输出里没有 `%p` 的规则

### Requirement: @extend !optional 与 compound 归并
`@extend <sel> !optional` 在目标缺失时 MUST NOT 报错；extend 一个 compound 选择器 MUST 按金样
选择器代数行为丢弃公共 subject 来归并（与 `is-superselector`/`unify` 一致）。

#### Scenario: 可选 extend 缺失目标
- **WHEN** `.a { @extend .missing !optional }`
- **THEN** 不报错
