# css-output Specification

## ADDED Requirements

### Requirement: expanded 序列化
渲染器 MUST 逐字节匹配金样（尾随换行除外）地发射 expanded CSS：`selector {\n  prop: value;\n}`，
两空格缩进、每行一条声明，多条声明/规则按源顺序串联。

#### Scenario: 两条声明
- **WHEN** `.foo { a: b; c: d; }`
- **THEN** 输出为 `.foo {\n  a: b;\n  c: d;\n}`

### Requirement: 声明与规则顺序
输出顺序 MUST 在嵌套展开后保留文档顺序；一条规则的声明先于其嵌套规则；`@media`/`@supports` 的分组
与提升 MUST 遵循金样。

#### Scenario: 嵌套选择器展开顺序
- **WHEN** `a { x: 1; b { y: 2 } }`
- **THEN** 先发射 `a { x: 1 }`，再发射 `a b { y: 2 }`

### Requirement: 值级渲染
渲染器 MUST 对每种类型应用金样的值序列化：number 及其单位与舍入、string 保留引号与否、list/map 用
正确的分隔符/方括号（CSS `toCss` 形态，非 inspect 形态），以及 `null` 丢弃该声明。

#### Scenario: 空格 list 以空格渲染
- **WHEN** `a { m: (1 2 3) }`
- **THEN** CSS 值渲染为金样 `1 2 3`
