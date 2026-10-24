# scss-builtins Specification

## ADDED Requirements

### Requirement: 命名空间核心模块
内置函数 MUST 经 `@use "sass:math|string|list|map|color|selector|meta"` 以 `ns.fn(...)` 命名空间
调用可达，每个都有金样的 arity 与命名副参集。全局（legacy）别名 MAY 存在，但 MUST NOT 遮蔽或破坏
同名裸函数在 plain-CSS 里的直通（如 CSS 的 `invert()`、`min()`、`max()`）。

#### Scenario: 命名空间解析
- **WHEN** `math.div(6, 3)`
- **THEN** 结果为 `2`

#### Scenario: 裸 CSS 函数名不被全局注册
- **WHEN** 裸 `invert(10%)` 出现在 plain-CSS 输出上下文
- **THEN** 原样直通（不被内置截获）

### Requirement: 逐函数 arity 与参数名
每个内置 MUST 声明完整的参数列表（参数名 + min/max arity），使命名实参精确解析；缺失/多余实参，
以及同一槽既位置又命名，MUST 逐字抛出金样消息。

#### Scenario: 未知命名实参
- **WHEN** 固定 arity 内置被以 `$bogus: 1` 调用
- **THEN** 产生金样 `No such argument` 错误

### Requirement: 内置错误文案契约
对每条失败调用路径，消息文本（参数名、值渲染、索引）MUST 逐字匹配金样 `Error:` 行，包括
`is not a number`/`is not a map`/`Expected … to have no units`/角度单位句式，因为金样比较错误文本。

#### Scenario: 要求无单位的参数
- **WHEN** `math.sqrt(4px)`
- **THEN** 产生金样 `$number: Expected 4px to have no units.`
