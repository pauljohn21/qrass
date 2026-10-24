# scss-values Specification

## ADDED Requirements

### Requirement: 值类型
求值器 MUST 建模 Sass 值类型：Number（带单位）、String（带引号 vs 不带）、Color、List
（逗号/空格/斜杠分隔，可带方括号）、Map、Bool 与 Null，每个都是沿求值流 move 的 owned 值。

#### Scenario: null 渲染为空
- **WHEN** 某声明的值求值为 `null`
- **THEN** 该声明从输出中省略

### Requirement: 数值运算符与单位代数
算术（`+ - * / %`）MUST 遵循 Sass 单位规则：同单位可加减，`*`/`/` 产生或抵消单位，单位不兼容的
加减 MUST 抛出与金样匹配的错误。两条裸 number 之间的 `/` 在声明值里 MUST NOT 自动相除（除非用
`math.div`，否则保留为斜杠分隔值）。

#### Scenario: 同单位相加
- **WHEN** 求值 `10px + 5px`
- **THEN** 结果为 `15px`

#### Scenario: 单位不兼容相加报错
- **WHEN** 求值 `1px + 1s`
- **THEN** 产生与金样"不兼容单位"文案匹配的错误

### Requirement: 相等与强制转换
值相等 MUST 区分类型，对 number 还须按可归一单位比较；带引号字符串 MUST NOT 等于不带引号字符串；
单元素 list 与其元素 MUST 按"len≤1 时分隔符无关"的金样规则比较。向某类型强制转换失败时 MUST
报出金样的 "is not a …" 消息并带上参数名。

#### Scenario: 带引号与不带引号字符串不等
- **WHEN** 比较 `"a"` 与 `a`
- **THEN** 二者不相等

### Requirement: List 与 Map 构造
求值器 MUST 支持逗号与空格 list、方括号 list，以及 `(k: v, …)` map，`map.get`/list 索引（1-based）
语义与金样一致。

#### Scenario: 索引空格 list
- **WHEN** `nth((a b c), 2)`
- **THEN** 结果为 `b`
